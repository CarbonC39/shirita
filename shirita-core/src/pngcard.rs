//! Read-only parsing of the JSON (text chunk) for SillyTavern character cards embedded in PNG files. A pure function that does not decode pixels or access any libraries.
//!
//! PNG = 8-byte signature + a chunk sequence: `length(4 BE) | type(4) | data(length) | crc(4)`.
//! ST stores the card JSON as Base64 in the tEXt chunk, with the keyword `chara` (V2) or `ccv3` (V3);
//! tEXt data = `keyword\0text`.

use base64::Engine;

use crate::{Error, Result};

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

/// Maximum size of a single text chunk (Sanity Limit): Plain text character cards are much smaller than this.
/// If this limit is exceeded, an error is reported, and memory is **never** pre-allocated based on the declared length, to protect server memory.
pub const MAX_TEXT_CHUNK: usize = 8 * 1024 * 1024;

/// Extract the embedded character card JSON from PNG bytes: Scan the tEXt chunk, prioritizing the keyword `ccv3` followed by `chara`,
/// then decode the text using base64 to UTF-8 JSON. Non-PNG / no match / out of bounds / decoding failure → Err.
pub fn read_card_json(png: &[u8]) -> Result<serde_json::Value> {
    if png.len() < 8 || png[..8] != PNG_SIGNATURE {
        return Err(Error::Config("not a PNG file".into()));
    }
    let mut pos = 8usize;
    let mut chara: Option<String> = None;
    let mut ccv3: Option<String> = None;
    while pos + 8 <= png.len() {
        let len = u32::from_be_bytes([png[pos], png[pos + 1], png[pos + 2], png[pos + 3]]) as usize;
        let ctype = &png[pos + 4..pos + 8];
        // Exceeds the length limit → Throws an error immediately; does not read or allocate.
        if len > MAX_TEXT_CHUNK {
            return Err(Error::Config(format!("PNG chunk too large: {len} bytes")));
        }
        let data_start = pos + 8;
        let data_end = data_start + len;
        if data_end + 4 > png.len() {
            break; // Truncated/corrupted, stop scanning
        }
        if ctype == b"tEXt" {
            let data = &png[data_start..data_end];
            if let Some(nul) = data.iter().position(|&b| b == 0) {
                let keyword = &data[..nul];
                let text = String::from_utf8_lossy(&data[nul + 1..]).to_string();
                if keyword == b"ccv3" {
                    ccv3 = Some(text);
                } else if keyword == b"chara" {
                    chara = Some(text);
                }
            }
        }
        if ctype == b"IEND" {
            break;
        }
        pos = data_end + 4; // skip CRC
    }
    let b64 = ccv3.or(chara).ok_or_else(|| Error::Config("no character card tEXt chunk".into()))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|e| Error::Config(format!("base64 decode failed: {e}")))?;
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| Error::Config(format!("card JSON parse failed: {e}")))?;
    Ok(json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    /// Create a minimal PNG: signature + a text (keyword) + IEND. Set the CRC to zero (no checksum during read).
    fn png_with_text(keyword: &str, text: &str) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&PNG_SIGNATURE);
        let mut data = Vec::new();
        data.extend_from_slice(keyword.as_bytes());
        data.push(0);
        data.extend_from_slice(text.as_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(b"tEXt");
        out.extend_from_slice(&data);
        out.extend_from_slice(&[0, 0, 0, 0]); // crc
        // IEND
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(b"IEND");
        out.extend_from_slice(&[0, 0, 0, 0]);
        out
    }

    fn b64(json: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(json.as_bytes())
    }

    #[test]
    fn reads_chara_v2() {
        let png = png_with_text("chara", &b64(r#"{"spec":"chara_card_v2","data":{"name":"Neo"}}"#));
        let v = read_card_json(&png).unwrap();
        assert_eq!(v["data"]["name"], "Neo");
    }

    #[test]
    fn ccv3_takes_precedence_over_chara() {
        // Two texts: chara=Old, ccv3=New → Select ccv3.
        let mut png = PNG_SIGNATURE.to_vec();
        for (kw, name) in [("chara", "Old"), ("ccv3", "New")] {
            let body = b64(&format!(r#"{{"data":{{"name":"{name}"}}}}"#));
            let mut data = Vec::new();
            data.extend_from_slice(kw.as_bytes());
            data.push(0);
            data.extend_from_slice(body.as_bytes());
            png.extend_from_slice(&(data.len() as u32).to_be_bytes());
            png.extend_from_slice(b"tEXt");
            png.extend_from_slice(&data);
            png.extend_from_slice(&[0, 0, 0, 0]);
        }
        png.extend_from_slice(&0u32.to_be_bytes());
        png.extend_from_slice(b"IEND");
        png.extend_from_slice(&[0, 0, 0, 0]);
        let v = read_card_json(&png).unwrap();
        assert_eq!(v["data"]["name"], "New");
    }

    #[test]
    fn rejects_non_png() {
        assert!(read_card_json(b"not a png at all").is_err());
    }

    #[test]
    fn errors_when_no_text_chunk() {
        let mut png = PNG_SIGNATURE.to_vec();
        png.extend_from_slice(&0u32.to_be_bytes());
        png.extend_from_slice(b"IEND");
        png.extend_from_slice(&[0, 0, 0, 0]);
        assert!(read_card_json(&png).is_err());
    }

    #[test]
    fn errors_on_bad_base64() {
        let png = png_with_text("chara", "!!!! not base64 !!!!");
        assert!(read_card_json(&png).is_err());
    }

    #[test]
    fn rejects_oversized_chunk_without_allocating() {
        // Construct a tEXt chunk with an excessively large length (no actual data provided) → An error must be raised before allocation.
        let mut png = PNG_SIGNATURE.to_vec();
        png.extend_from_slice(&((MAX_TEXT_CHUNK as u32) + 1).to_be_bytes());
        png.extend_from_slice(b"tEXt");
        // Do not append actual data: the function should raise an Err when checking the length and should not execute beyond this point.
        assert!(read_card_json(&png).is_err());
    }
}
