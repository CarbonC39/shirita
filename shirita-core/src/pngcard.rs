//! 只读解析 PNG 内嵌的 SillyTavern 角色卡 JSON（tEXt chunk）。纯函数，不解码像素、不触库。
//!
//! PNG = 8 字节签名 + 一串 chunk：`length(4 BE) | type(4) | data(length) | crc(4)`。
//! ST 把卡 JSON 以 base64 存在 tEXt chunk，keyword `chara`（V2）或 `ccv3`（V3）；
//! tEXt data = `keyword\0text`。

use base64::Engine;

use crate::{Error, Result};

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

/// 单个 tEXt chunk 的大小上限（Sanity Limit）：纯文本角色卡远小于此。
/// 超过则报错且**绝不**按声明长度预分配，保护服务器内存。
pub const MAX_TEXT_CHUNK: usize = 8 * 1024 * 1024;

/// 从 PNG 字节提取内嵌角色卡 JSON：扫 tEXt chunk，keyword 优先 `ccv3` 再 `chara`，
/// base64 解码其 text 为 UTF-8 JSON。非 PNG / 无匹配 / 超限 / 解码失败 → Err。
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
        // 越过 length 上限 → 直接报错，不读、不分配。
        if len > MAX_TEXT_CHUNK {
            return Err(Error::Config(format!("PNG chunk too large: {len} bytes")));
        }
        let data_start = pos + 8;
        let data_end = data_start + len;
        if data_end + 4 > png.len() {
            break; // 截断/损坏，停止扫描
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
        pos = data_end + 4; // 跳过 CRC
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

    /// 造一个最小 PNG：签名 + 一个 tEXt(keyword) + IEND。CRC 置零（read 不校验）。
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
        // 两个 tEXt：chara=Old, ccv3=New → 取 ccv3。
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
        // 构造一个声明超大 length 的 tEXt chunk（实际数据不提供）→ 必须在分配前报错。
        let mut png = PNG_SIGNATURE.to_vec();
        png.extend_from_slice(&((MAX_TEXT_CHUNK as u32) + 1).to_be_bytes());
        png.extend_from_slice(b"tEXt");
        // 不追加真实数据：函数应在检查 length 时即 Err，不读到这里。
        assert!(read_card_json(&png).is_err());
    }
}
