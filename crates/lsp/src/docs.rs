//! The daemon's copy of the documents clients opened on a server, kept from the sync
//! notifications that pass through (`didOpen`, `didChange` full or incremental, `didClose`).
//! Positions are LSP's: 0-based lines, UTF-16 code units.

use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub uri: String,
    pub language_id: String,
    pub version: i64,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct Documents {
    docs: BTreeMap<String, Document>,
}

/// Byte offset in `text` of an LSP position (clamped to the line's end and the text's end).
pub fn offset_of(text: &str, line: u64, character: u64) -> usize {
    let mut start = 0usize;
    for _ in 0..line {
        match text[start..].find('\n') {
            Some(i) => start += i + 1,
            None => return text.len(),
        }
    }
    let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
    let mut units = 0u64;
    for (i, ch) in text[start..end].char_indices() {
        if units >= character {
            return start + i;
        }
        units += ch.len_utf16() as u64;
    }
    end
}

impl Documents {
    pub fn get(&self, uri: &str) -> Option<&Document> {
        self.docs.get(uri)
    }

    pub fn all(&self) -> impl Iterator<Item = &Document> {
        self.docs.values()
    }

    pub fn len(&self) -> usize {
        self.docs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    /// Applies a sync notification. Other methods are ignored. Returns false when the
    /// notification does not fit the mirror (unknown document, bad range): the caller forwards
    /// it anyway; the server is the authority.
    pub fn apply(&mut self, method: &str, params: &Value) -> bool {
        let uri = params["textDocument"]["uri"].as_str().unwrap_or_default();
        match method {
            "textDocument/didOpen" => {
                let td = &params["textDocument"];
                self.docs.insert(
                    uri.to_string(),
                    Document {
                        uri: uri.to_string(),
                        language_id: td["languageId"].as_str().unwrap_or_default().to_string(),
                        version: td["version"].as_i64().unwrap_or(0),
                        text: td["text"].as_str().unwrap_or_default().to_string(),
                    },
                );
                true
            }
            "textDocument/didClose" => self.docs.remove(uri).is_some(),
            "textDocument/didChange" => {
                let Some(doc) = self.docs.get_mut(uri) else {
                    return false;
                };
                let Some(changes) = params["contentChanges"].as_array() else {
                    return false;
                };
                for c in changes {
                    let Some(new) = c["text"].as_str() else {
                        return false;
                    };
                    let range = &c["range"];
                    if range.is_null() {
                        doc.text = new.to_string();
                        continue;
                    }
                    let pos = |p: &Value| {
                        (
                            p["line"].as_u64().unwrap_or(0),
                            p["character"].as_u64().unwrap_or(0),
                        )
                    };
                    let (sl, sc) = pos(&range["start"]);
                    let (el, ec) = pos(&range["end"]);
                    let a = offset_of(&doc.text, sl, sc);
                    let b = offset_of(&doc.text, el, ec);
                    if a > b {
                        return false;
                    }
                    doc.text.replace_range(a..b, new);
                }
                if let Some(v) = params["textDocument"]["version"].as_i64() {
                    doc.version = v;
                }
                true
            }
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn utf16_offsets() {
        let t = "a😀b\nπx\n";
        assert_eq!(offset_of(t, 0, 0), 0);
        assert_eq!(offset_of(t, 0, 1), 1);
        assert_eq!(offset_of(t, 0, 3), 5); // after the surrogate pair
        assert_eq!(offset_of(t, 0, 99), 6); // clamped to the line end
        assert_eq!(offset_of(t, 1, 1), 9); // π is 2 bytes, 1 unit
        assert_eq!(offset_of(t, 9, 0), t.len());
    }

    #[test]
    fn open_change_close() {
        let mut d = Documents::default();
        let uri = "file:///w/a.rs";
        d.apply(
            "textDocument/didOpen",
            &json!({"textDocument":{"uri":uri,"languageId":"rust","version":1,"text":"fn a() {}\nfn b() {}\n"}}),
        );
        assert!(d.apply(
            "textDocument/didChange",
            &json!({"textDocument":{"uri":uri,"version":2},"contentChanges":[
                {"range":{"start":{"line":0,"character":3},"end":{"line":0,"character":4}},"text":"z"},
                {"range":{"start":{"line":1,"character":0},"end":{"line":1,"character":0}},"text":"pub "}
            ]}),
        ));
        let doc = d.get(uri).unwrap();
        assert_eq!(doc.text, "fn z() {}\npub fn b() {}\n");
        assert_eq!(doc.version, 2);
        d.apply(
            "textDocument/didChange",
            &json!({"textDocument":{"uri":uri,"version":3},"contentChanges":[{"text":"all new"}]}),
        );
        assert_eq!(d.get(uri).unwrap().text, "all new");
        assert!(d.apply(
            "textDocument/didClose",
            &json!({"textDocument":{"uri":uri}})
        ));
        assert!(d.is_empty());
        assert!(!d.apply(
            "textDocument/didChange",
            &json!({"textDocument":{"uri":uri},"contentChanges":[{"text":"x"}]})
        ));
    }
}
