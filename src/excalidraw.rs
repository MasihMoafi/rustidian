//! Reading and updating Excalidraw documents without discarding plugin data.
use serde_json::Value;
use std::{ops::Range, path::Path};

const JSON_FENCE: &str = "```json";
const COMPRESSED_FENCE: &str = "```compressed-json";

/// Whether a file is an Excalidraw document, including plugin-marked notes.
pub fn is_drawing(path: &Path, text: &str) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if name.ends_with(".excalidraw") || name.ends_with(".excalidraw.md") {
        return true;
    }
    text.strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
        .and_then(|frontmatter| frontmatter.split_once("\n---"))
        .is_some_and(|(frontmatter, _)| {
            frontmatter.lines().any(|line| {
                line.trim_start()
                    .strip_prefix("excalidraw-plugin:")
                    .is_some()
            })
        })
}

/// The original Markdown is retained around the drawing payload.
pub struct Drawing {
    pub data: Value,
    original: String,
    payload: Option<Range<usize>>,
}

impl Drawing {
    pub fn parse(text: &str) -> Result<Self, String> {
        if let Ok(data) = serde_json::from_str::<Value>(text) {
            return Ok(Self {
                data,
                original: text.to_owned(),
                payload: None,
            });
        }
        let fences = [(JSON_FENCE, false), (COMPRESSED_FENCE, true)];
        // Excalidraw embeds its data under this heading; other JSON blocks can appear earlier.
        let start = text.find("# Drawing").unwrap_or(0);
        for (fence, compressed) in fences {
            if let Some(offset) = text[start..].find(fence) {
                let begin = start + offset;
                let content = begin + fence.len();
                let end = text[content..]
                    .find("```")
                    .ok_or("Unclosed drawing block")?
                    + content;
                let encoded = text[content..end].trim();
                let json = if compressed {
                    decompress_base64(encoded)?
                } else {
                    encoded.to_owned()
                };
                let data = serde_json::from_str(&json)
                    .map_err(|e| format!("Invalid drawing JSON: {e}"))?;
                return Ok(Self {
                    data,
                    original: text.to_owned(),
                    payload: Some(begin..end),
                });
            }
        }
        Err("Drawing JSON block not found".into())
    }

    /// Keep unknown JSON properties and all Markdown outside the drawing block.
    pub fn serialize(&self) -> Result<String, String> {
        let json = serde_json::to_string_pretty(&self.data).map_err(|e| e.to_string())?;
        Ok(match &self.payload {
            Some(range) => format!(
                "{}{}\n{}\n{}",
                &self.original[..range.start],
                JSON_FENCE,
                json,
                &self.original[range.end..]
            ),
            None => json,
        })
    }
}

/// LZ-String's Base64 bitstream, decoded to UTF-16 code units as in JavaScript.
fn decompress_base64(input: &str) -> Result<String, String> {
    const ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=";
    let values: Vec<u32> = input
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| {
            ALPHABET
                .find(c)
                .map(|v| v as u32)
                .ok_or_else(|| "Invalid compressed drawing".to_owned())
        })
        .collect::<Result<_, _>>()?;
    let mut index = 0;
    let mut mask = 32;
    let mut read = |count: u32| -> Result<u32, String> {
        let mut result = 0;
        for bit in 0..count {
            let value = *values.get(index).ok_or("Truncated compressed drawing")?;
            if value & mask != 0 {
                result |= 1 << bit;
            }
            mask >>= 1;
            if mask == 0 {
                mask = 32;
                index += 1;
            }
        }
        Ok(result)
    };
    let first = match read(2)? {
        0 => read(8)?,
        1 => read(16)?,
        2 => return Ok(String::new()),
        _ => return Err("Invalid compressed drawing".into()),
    };
    let mut dictionary: Vec<Vec<u16>> = vec![vec![], vec![], vec![], vec![first as u16]];
    let mut previous = vec![first as u16];
    let mut output = previous.clone();
    let mut width = 3;
    let mut remaining = 4;
    loop {
        if width > 24 {
            return Err("Compressed drawing dictionary is too large".into());
        }
        let mut code = read(width)? as usize;
        if code < 2 {
            let character = read(if code == 0 { 8 } else { 16 })? as u16;
            dictionary.push(vec![character]);
            code = dictionary.len() - 1;
            remaining -= 1;
            if remaining == 0 {
                remaining = 1 << width;
                width += 1;
            }
        } else if code == 2 {
            return String::from_utf16(&output)
                .map_err(|_| "Invalid UTF-16 in compressed drawing".into());
        }
        let entry = if let Some(entry) = dictionary.get(code) {
            entry.clone()
        } else if code == dictionary.len() {
            let mut entry = previous.clone();
            entry.push(previous[0]);
            entry
        } else {
            return Err("Invalid compressed drawing dictionary".into());
        };
        output.extend_from_slice(&entry);
        let mut next = previous;
        next.push(entry[0]);
        dictionary.push(next);
        previous = entry;
        remaining -= 1;
        if remaining == 0 {
            remaining = 1 << width;
            width += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lz_string_decodes_and_rejects_invalid_data() {
        assert_eq!(decompress_base64("BIUwNmD2Q===").unwrap(), "Hello");
        assert!(decompress_base64("?").is_err());
        assert!(decompress_base64("B").is_err());
    }
    #[test]
    fn preserves_surrounding_markdown_and_unknown_properties() {
        let source = "---\nexcalidraw-plugin: parsed\n---\n# Text Elements\nhello\n# Drawing\n```json\n{\"elements\":[{\"extra\":42}],\"appState\":{\"future\":true},\"files\":{\"f\":{\"custom\":1}}}\n```\n%%\n";
        let mut drawing = Drawing::parse(source).unwrap();
        drawing.data["elements"][0]["x"] = 3.into();
        let saved = drawing.serialize().unwrap();
        assert!(saved.starts_with(
            "---\nexcalidraw-plugin: parsed\n---\n# Text Elements\nhello\n# Drawing\n"
        ));
        assert!(saved.ends_with("```\n%%\n"));
        let data = Drawing::parse(&saved).unwrap().data;
        assert_eq!(data["elements"][0]["extra"], 42);
        assert_eq!(data["appState"]["future"], true);
        assert_eq!(data["files"]["f"]["custom"], 1);
    }
    #[test]
    fn detects_drawing_paths_and_frontmatter() {
        assert!(is_drawing(Path::new("test.excalidraw"), ""));
        assert!(is_drawing(Path::new("test.excalidraw.md"), ""));
        assert!(is_drawing(
            Path::new("note.md"),
            "---\nexcalidraw-plugin: parsed\n---\n"
        ));
        assert!(!is_drawing(
            Path::new("note.md"),
            "excalidraw-plugin: parsed"
        ));
    }
    #[test]
    fn plain_json_round_trip() {
        let source = r#"{"elements":[],"appState":{"future":"yes"},"files":{}}"#;
        let drawing = Drawing::parse(source).unwrap();
        assert_eq!(
            Drawing::parse(&drawing.serialize().unwrap()).unwrap().data,
            drawing.data
        );
    }
}
