//! stdio transport for MCP.
//!
//! Launches an explicitly configured executable with separate arguments and no
//! shell; passes only the explicit environment entries; reserves stdout for
//! newline-framed JSON-RPC; drains stderr into bounded debug diagnostics.

use std::process::Stdio;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use crate::mcp::parse_result;
use crate::{Error, Result};

pub struct StdioSession {
    child: Child,
    io: tokio::sync::Mutex<StdioIo>,
}

struct StdioIo {
    stdin: Option<tokio::process::ChildStdin>,
    stdout: BufReader<tokio::process::ChildStdout>,
}

impl Drop for StdioSession {
    fn drop(&mut self) {
        // Best-effort cleanup when the run's frozen registry (which owns the
        // session through its Tool handlers) is dropped without a graceful
        // shutdown: kill the child so it cannot linger.
        let _ = self.child.start_kill();
    }
}

/// Read one newline-framed message up to `cap` bytes so a chatty or malicious
/// server cannot make us buffer an unbounded line.
async fn read_bounded_line<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    out: &mut String,
    cap: usize,
) -> std::io::Result<usize> {
    out.clear();
    let mut total = 0usize;
    loop {
        let buf = reader.fill_buf().await?;
        if buf.is_empty() {
            break;
        }
        let newline_pos = buf.iter().position(|&b| b == b'\n');
        let newline_len = newline_pos.map(|i| i + 1).unwrap_or(buf.len());
        if total.saturating_add(newline_len) > cap {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "stdio message exceeded the declared limit",
            ));
        }
        let take = newline_len;
        let text = std::str::from_utf8(&buf[..take])
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "non-utf8 stdio message"))?;
        out.push_str(text);
        reader.consume(take);
        total += take;
        if newline_pos.is_some() {
            break; // consumed through the newline
        }
    }
    Ok(total)
}

impl StdioSession {
    /// Spawn the configured executable and start draining stderr.
    pub async fn spawn(
        command: &str,
        args: &[String],
        env: &[(String, String)],
        _timeout_ms: u64,
    ) -> Result<StdioSession> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Only the explicit environment entries are passed; nothing inherited.
        cmd.env_clear();
        for (key, value) in env {
            cmd.env(key, value);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| Error::Mcp(format!("failed to launch MCP stdio server '{command}': {e}")))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Mcp("MCP stdio child stdin unavailable".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Mcp("MCP stdio child stdout unavailable".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::Mcp("MCP stdio child stderr unavailable".into()))?;

        // Drain stderr so a chatty server cannot block on a full pipe; keep the
        // diagnostics bounded and at debug level (never model context).
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let trimmed = line.trim_end();
                        if trimmed.len() > 8 * 1024 {
                            tracing::debug!(stderr = "…", "mcp stdio diagnostic (truncated)");
                        } else if !trimmed.is_empty() {
                            tracing::debug!(stderr = %trimmed, "mcp stdio diagnostic");
                        }
                    }
                }
            }
        });

        Ok(StdioSession {
            child,
            io: tokio::sync::Mutex::new(StdioIo {
                stdin: Some(stdin),
                stdout: BufReader::new(stdout),
            }),
        })
    }

    /// Send a JSON-RPC request and read until the response with the matching id.
    pub async fn request(&mut self, id: u64, message: &Value) -> Result<Value> {
        let mut io = self.io.lock().await;
        let stdin = io
            .stdin
            .as_mut()
            .ok_or_else(|| Error::Mcp("mcp stdio stdin already closed".into()))?;
        let mut line = message.to_string();
        line.push('\n');
        stdin.write_all(line.as_bytes()).await.map_err(|e| {
            Error::Mcp(format!("mcp stdio write failed (server exited?): {e}"))
        })?;
        stdin.flush().await.map_err(|e| Error::Mcp(format!("mcp stdio flush failed: {e}")))?;
        loop {
            let mut response_line = String::new();
            let n = read_bounded_line(&mut io.stdout, &mut response_line, crate::mcp::MCP_MAX_STDIO_LINE_BYTES)
                .await
                .map_err(|e| Error::Mcp(format!("mcp stdio read failed: {e}")))?;
            if n == 0 {
                return Err(Error::Mcp("mcp stdio stream closed before a response".into()));
            }
            let response: Value = serde_json::from_str(response_line.trim_end()).map_err(|e| {
                Error::Mcp(format!("mcp stdio produced invalid JSON: {e}"))
            })?;
            if response.get("id").and_then(Value::as_u64) == Some(id) {
                return parse_result(&response);
            }
            // Ignore notifications / responses for other ids; we are strictly
            // sequential, but tolerate interleaved server notifications.
        }
    }

    /// Send a JSON-RPC notification (no id, no response expected).
    pub async fn notify(&self, message: &Value) -> Result<()> {
        let mut io = self.io.lock().await;
        let stdin = io
            .stdin
            .as_mut()
            .ok_or_else(|| Error::Mcp("mcp stdio stdin already closed".into()))?;
        let mut line = message.to_string();
        line.push('\n');
        stdin.write_all(line.as_bytes()).await.map_err(|e| {
            Error::Mcp(format!("mcp stdio write failed (server exited?): {e}"))
        })?;
        stdin.flush().await.map_err(|e| Error::Mcp(format!("mcp stdio flush failed: {e}")))
    }

    /// Close stdin (EOF), then terminate and reap the child.
    pub async fn close(&mut self) -> Result<()> {
        let mut io = self.io.lock().await;
        if let Some(mut stdin) = io.stdin.take() {
            let _ = stdin.flush().await;
            drop(stdin); // EOF for the server
        }
        drop(io);
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
        Ok(())
    }
}
