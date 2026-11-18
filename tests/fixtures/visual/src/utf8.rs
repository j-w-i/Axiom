//! Incremental UTF-8 decoding with U+FFFD replacement.

/// Per-stream decoder: holds an incomplete trailing sequence between chunks.
#[derive(Default)]
pub(crate) struct Utf8Chunker {
    carry: Vec<u8>,
}

impl Utf8Chunker {
    /// Decodes `chunk`; an incomplete sequence at its end waits for the next call.
    pub fn push(&mut self, chunk: &[u8]) -> String {
        self.carry.extend_from_slice(chunk);
        let bytes = std::mem::take(&mut self.carry);
        let mut out = String::with_capacity(bytes.len());
        let mut rest = bytes.as_slice();
        loop {
            match std::str::from_utf8(rest) {
                Ok(s) => {
                    out.push_str(s);
                    break;
                }
                Err(e) => {
                    let (valid, after) = rest.split_at(e.valid_up_to());
                    // `valid` is valid UTF-8 by construction of `valid_up_to`.
                    out.push_str(&String::from_utf8_lossy(valid));
                    match e.error_len() {
                        Some(n) => {
                            out.push('\u{FFFD}');
                            rest = &after[n..];
                        }
                        None => {
                            self.carry = after.to_vec();
                            break;
                        }
                    }
                }
            }
        }
        out
    }

    /// At EOF a dangling partial sequence becomes one U+FFFD.
    pub fn finish(&mut self) -> String {
        if self.carry.is_empty() {
            String::new()
        } else {
            self.carry.clear();
            "\u{FFFD}".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_sequence_is_joined() {
        let mut d = Utf8Chunker::default();
        let bytes = "é✓".as_bytes();
        assert_eq!(d.push(&bytes[..1]), "");
        assert_eq!(d.push(&bytes[1..3]), "é");
        assert_eq!(d.push(&bytes[3..]), "✓");
        assert_eq!(d.finish(), "");
    }

    #[test]
    fn invalid_bytes_are_replaced() {
        let mut d = Utf8Chunker::default();
        assert_eq!(d.push(b"a\xffb\xfe"), "a\u{FFFD}b\u{FFFD}");
    }

    #[test]
    fn dangling_partial_at_eof_is_replaced() {
        let mut d = Utf8Chunker::default();
        assert_eq!(d.push(&"✓".as_bytes()[..2]), "");
        assert_eq!(d.finish(), "\u{FFFD}");
    }
}
