//! HTML "card" patching: lets the model edit a previously-rendered full HTML
//! document with compact SEARCH/REPLACE blocks instead of re-emitting the whole
//! document every turn.
//!
//! The raw model output keeps the blocks (so chat history — and thus the next
//! turn's *input* — stays small), while the reconstructed full document is what
//! gets rendered (stored in `display_content`, which the UI already iframe-
//! renders for full HTML docs). This mirrors the `<state_update>` pattern: the
//! engine parses + applies the protocol, and the model is told the protocol via
//! an instruction injected into the prompt whenever a card is in play (see
//! `conversation::assemble_request`).

/// One search/replace edit parsed from a model reply.
#[derive(Debug, Clone, PartialEq)]
pub struct HtmlPatch {
    pub find: String,
    pub replace: String,
}

/// If `s` opens with a markdown fence (``` optionally followed by a bare
/// language tag, then a newline), return the text after that opening line;
/// otherwise return `s` unchanged. A real-world ST "HTML card" greeting is
/// commonly the whole document wrapped in a ```/```html fence rather than
/// starting with the doctype literally — see `isHtmlDocument` in the
/// frontend's markdown.ts, which already unwraps fences via full Markdown
/// parsing before checking; this is the equivalent for raw message text.
fn strip_leading_fence(s: &str) -> &str {
    let Some(rest) = s.strip_prefix("```") else { return s };
    let Some(nl) = rest.find('\n') else { return s };
    let lang_line = rest[..nl].trim();
    if lang_line.chars().all(|c| c.is_ascii_alphanumeric()) {
        &rest[nl + 1..]
    } else {
        s
    }
}

/// A reply is treated as a full HTML document when it (after trimming leading
/// whitespace, and unwrapping a leading ``` fence if present) starts with a
/// doctype or an `<html>` tag — the same rule the UI uses to pick its
/// sandboxed-iframe render path.
pub fn is_html_document(text: &str) -> bool {
    let s = strip_leading_fence(text.trim_start()).trim_start().to_ascii_lowercase();
    s.starts_with("<!doctype html") || s.starts_with("<html")
}

// Aider-style fence markers. The seven-angle-bracket lines effectively never
// occur in real HTML/CSS/JS, so they are safe delimiters around HTML bodies.
const SEARCH_MARKER: &str = "<<<<<<< SEARCH";
const SEP_MARKER: &str = "=======";
const REPLACE_MARKER: &str = ">>>>>>> REPLACE";

/// The instruction injected into the prompt when the conversation already holds
/// an HTML card, telling the model to edit it with SEARCH/REPLACE blocks rather
/// than resending the whole document.
pub const INSTRUCTION: &str = "\
The latest rendered message in this conversation is a self-contained HTML \
document (an interactive \"card\" UI). When your next reply only changes part \
of that card, DO NOT resend the whole document — emit one or more edit blocks \
in exactly this format:

<<<<<<< SEARCH
(text that currently appears in the card, copied verbatim)
=======
(replacement text)
>>>>>>> REPLACE

Rules:
- The SEARCH text must match the current HTML exactly (including whitespace) \
and be long enough to occur only once.
- Use several blocks for several edits; they apply top to bottom.
- Only when you intend to rebuild the card from scratch should you output a \
full new <!DOCTYPE html> document instead of edit blocks.";

/// True if the text contains at least one SEARCH marker line — a cheap test for
/// "this reply carries HTML edit blocks".
pub fn has_patch_blocks(text: &str) -> bool {
    text.lines().any(|l| l.trim_end() == SEARCH_MARKER)
}

/// Parse every well-formed SEARCH/REPLACE block, in order. Markers must sit on
/// their own line; the body between them is taken verbatim. An unterminated
/// block ends parsing (whatever was already collected is returned).
pub fn parse_patches(text: &str) -> Vec<HtmlPatch> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_end() != SEARCH_MARKER {
            i += 1;
            continue;
        }
        i += 1;
        let mut find = Vec::new();
        let mut found_sep = false;
        while i < lines.len() {
            if lines[i].trim_end() == SEP_MARKER {
                found_sep = true;
                i += 1;
                break;
            }
            find.push(lines[i]);
            i += 1;
        }
        if !found_sep {
            break; // unterminated block
        }
        let mut replace = Vec::new();
        let mut found_end = false;
        while i < lines.len() {
            if lines[i].trim_end() == REPLACE_MARKER {
                found_end = true;
                i += 1;
                break;
            }
            replace.push(lines[i]);
            i += 1;
        }
        if !found_end {
            break; // unterminated block
        }
        out.push(HtmlPatch { find: find.join("\n"), replace: replace.join("\n") });
    }
    out
}

/// Apply patches to `base` in order, replacing the first occurrence of each
/// `find`. Returns `None` if `patches` is empty, if any `find` is empty, or if
/// any `find` is absent from the (progressively edited) document — the caller
/// then falls back to its normal display path.
pub fn apply_patches(base: &str, patches: &[HtmlPatch]) -> Option<String> {
    if patches.is_empty() {
        return None;
    }
    let mut doc = base.to_string();
    for p in patches {
        if p.find.is_empty() {
            return None;
        }
        let idx = doc.find(&p.find)?;
        doc.replace_range(idx..idx + p.find.len(), &p.replace);
    }
    Some(doc)
}

/// If `reply` carries patch blocks and a `base` card is known, reconstruct the
/// full document. `None` means "not a patch reply" or "a block did not match" —
/// either way the caller should fall back to rendering the raw reply.
pub fn reconstruct(base: Option<&str>, reply: &str) -> Option<String> {
    let patches = parse_patches(reply);
    if patches.is_empty() {
        return None;
    }
    apply_patches(base?, &patches)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARD: &str = "<!DOCTYPE html>\n<html><body><p>HP: 100</p>\n<p>Gold: 5</p></body></html>";

    #[test]
    fn detects_html_documents() {
        assert!(is_html_document("<!DOCTYPE html><html></html>"));
        assert!(is_html_document("  \n<html>x</html>"));
        assert!(!is_html_document("just a normal reply"));
        assert!(!is_html_document("<p>not a full doc</p>"));
    }

    #[test]
    fn detects_a_doctype_wrapped_in_a_markdown_fence() {
        // The common real-world ST card greeting shape: the whole document
        // fenced, sometimes with a bare ``` (no language tag) and CRLF line
        // endings (Windows-authored cards).
        assert!(is_html_document("```html\n<!DOCTYPE html><html></html>\n```"));
        assert!(is_html_document("```\r\n<!DOCTYPE html>\r\n<html></html>\r\n```"));
        assert!(is_html_document("```\n<html>x</html>\n```"));
    }

    #[test]
    fn does_not_misdetect_an_unrelated_fenced_block_as_html() {
        assert!(!is_html_document("```js\nconsole.log('<html>')\n```"));
        assert!(!is_html_document("```\njust some code, no doctype\n```"));
    }

    #[test]
    fn parses_multiple_blocks_in_order() {
        let reply = "Sure, updating.\n\
            <<<<<<< SEARCH\n<p>HP: 100</p>\n=======\n<p>HP: 80</p>\n>>>>>>> REPLACE\n\
            <<<<<<< SEARCH\n<p>Gold: 5</p>\n=======\n<p>Gold: 9</p>\n>>>>>>> REPLACE";
        let patches = parse_patches(reply);
        assert_eq!(patches.len(), 2);
        assert_eq!(patches[0], HtmlPatch { find: "<p>HP: 100</p>".into(), replace: "<p>HP: 80</p>".into() });
        assert_eq!(patches[1].replace, "<p>Gold: 9</p>");
    }

    #[test]
    fn applies_patches_to_base() {
        let patches = vec![
            HtmlPatch { find: "<p>HP: 100</p>".into(), replace: "<p>HP: 80</p>".into() },
            HtmlPatch { find: "<p>Gold: 5</p>".into(), replace: "<p>Gold: 9</p>".into() },
        ];
        let out = apply_patches(CARD, &patches).unwrap();
        assert!(out.contains("<p>HP: 80</p>"));
        assert!(out.contains("<p>Gold: 9</p>"));
        assert!(out.starts_with("<!DOCTYPE html>"));
    }

    #[test]
    fn unmatched_block_fails_so_caller_can_fall_back() {
        let patches = vec![HtmlPatch { find: "<p>MP: 50</p>".into(), replace: "x".into() }];
        assert_eq!(apply_patches(CARD, &patches), None);
    }

    #[test]
    fn reconstruct_round_trips_a_patch_reply() {
        let reply = "<<<<<<< SEARCH\n<p>HP: 100</p>\n=======\n<p>HP: 1</p>\n>>>>>>> REPLACE";
        let out = reconstruct(Some(CARD), reply).unwrap();
        assert!(out.contains("<p>HP: 1</p>"));
        assert!(!out.contains("HP: 100"));
    }

    #[test]
    fn reconstruct_is_none_for_plain_text_or_missing_base() {
        assert_eq!(reconstruct(Some(CARD), "just chatting, no edits"), None);
        let reply = "<<<<<<< SEARCH\nx\n=======\ny\n>>>>>>> REPLACE";
        assert_eq!(reconstruct(None, reply), None); // patch reply but no base card
    }

    #[test]
    fn unterminated_block_is_ignored() {
        let reply = "<<<<<<< SEARCH\n<p>HP: 100</p>\n=======\n<p>HP: 80</p>"; // no REPLACE marker
        assert!(parse_patches(reply).is_empty());
    }
}
