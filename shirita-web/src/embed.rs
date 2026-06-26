//! Embedded frontend serving. The two helpers below are always compiled (so
//! they're unit-tested without the feature or a built `dist/`); the rust-embed
//! struct + handlers are gated behind `embed-ui`.

/// Splice `window.__SHIRITA_RUNTIME__ = { base, token }` into `<head>` so the
/// same-origin browser gets the API base + token without a build-time bake.
/// `</` is neutralized so a token containing `</script>` can't break out of the
/// inline `<script>`.
pub fn inject_runtime(html: &str, token: &str) -> String {
    let rt = serde_json::json!({ "base": "", "token": token })
        .to_string()
        .replace("</", "<\\/");
    let tag = format!("<script>window.__SHIRITA_RUNTIME__={rt};</script>");
    match html.rfind("</head>") {
        Some(i) => {
            let mut s = String::with_capacity(html.len() + tag.len());
            s.push_str(&html[..i]);
            s.push_str(&tag);
            s.push_str(&html[i..]);
            s
        }
        None => format!("{tag}{html}"),
    }
}

/// Paths owned by the API / media / static-chunk / health routes — the SPA
/// fallback returns 404 for these instead of serving `index.html`.
#[cfg(any(feature = "embed-ui", test))]
pub(crate) fn is_reserved_prefix(path: &str) -> bool {
    path == "/health"
        || path == "/api" || path.starts_with("/api/")
        || path == "/assets" || path.starts_with("/assets/")
        || path == "/static" || path.starts_with("/static/")
}

#[cfg(feature = "embed-ui")]
mod serving {
    use super::{inject_runtime, is_reserved_prefix};
    use crate::AppState;
    use axum::extract::{Path, State};
    use axum::http::{header, StatusCode, Uri};
    use axum::response::{Html, IntoResponse, Response};

    /// The built Vue app, embedded at compile time (release) / read from disk
    /// (debug). Path is relative to `shirita-web/Cargo.toml`.
    #[derive(rust_embed::RustEmbed)]
    #[folder = "../shirita-ui/dist"]
    struct Ui;

    fn index_response(state: &AppState) -> Response {
        match Ui::get("index.html") {
            Some(f) => {
                let html = String::from_utf8_lossy(&f.data);
                Html(inject_runtime(&html, &state.config.token_secret)).into_response()
            }
            None => (StatusCode::INTERNAL_SERVER_ERROR, "embedded index.html missing").into_response(),
        }
    }

    /// `GET /` — the SPA shell with the runtime token injected.
    pub async fn serve_index(State(state): State<AppState>) -> Response {
        index_response(&state)
    }

    /// `GET /static/{*path}` — an embedded frontend chunk, content-typed by
    /// extension, immutably cached (filenames are content-hashed).
    pub async fn serve_static(Path(path): Path<String>) -> Response {
        match Ui::get(&format!("static/{path}")) {
            Some(f) => {
                let mime = mime_guess::from_path(&path)
                    .first_or_octet_stream();
                let body = axum::body::Bytes::from(f.data.into_owned());

                (
                    [
                        (header::CONTENT_TYPE, mime.as_ref()),
                        (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
                    ],
                    body,
                )
                    .into_response()
            }
            None => StatusCode::NOT_FOUND.into_response(),
        }
    }

    /// Router fallback — unknown app routes get the SPA shell (history-mode deep
    /// links); reserved prefixes 404 so an unknown API path never returns HTML.
    pub async fn spa_fallback(uri: Uri, State(state): State<AppState>) -> Response {
        if is_reserved_prefix(uri.path()) {
            return StatusCode::NOT_FOUND.into_response();
        }
        index_response(&state)
    }
}

#[cfg(feature = "embed-ui")]
pub use serving::{serve_index, serve_static, spa_fallback};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injects_runtime_before_head_close() {
        let out = inject_runtime("<html><head><title>x</title></head><body></body></html>", "secret");
        let script_at = out.find("__SHIRITA_RUNTIME__").unwrap();
        let head_close = out.find("</head>").unwrap();
        assert!(script_at < head_close, "script spliced before </head>");
        assert!(out.contains(r#""token":"secret""#));
        assert!(out.contains(r#""base":"""#));
    }

    #[test]
    fn neutralizes_script_breakout_in_token() {
        let out = inject_runtime("<head></head>", "a</script>b");
        assert!(!out.contains("a</script>b"), "raw </script> must not survive");
        assert!(out.contains(r#""token":"a<\/script>b""#));
    }

    #[test]
    fn prepends_when_no_head() {
        let out = inject_runtime("<body>hi</body>", "t");
        assert!(out.starts_with("<script>window.__SHIRITA_RUNTIME__="));
        assert!(out.ends_with("<body>hi</body>"));
    }

    #[test]
    fn reserved_prefixes_are_not_spa_routes() {
        for p in ["/health", "/api", "/api/sessions", "/assets", "/assets/x.png", "/static", "/static/app.js"] {
            assert!(is_reserved_prefix(p), "{p} should be reserved");
        }
        for p in ["/", "/book", "/chat/abc", "/settings"] {
            assert!(!is_reserved_prefix(p), "{p} should fall through to SPA");
        }
    }
}
