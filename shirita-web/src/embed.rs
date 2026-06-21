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
pub(crate) fn is_reserved_prefix(path: &str) -> bool {
    path == "/health"
        || path == "/api" || path.starts_with("/api/")
        || path == "/assets" || path.starts_with("/assets/")
        || path == "/static" || path.starts_with("/static/")
}

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
