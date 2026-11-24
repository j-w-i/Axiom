//! Server-sent events, after the WHATWG standard, where that allows (it replaces invalid UTF-8; a
//! replaced byte in a tool call's arguments would be a different call).
//!
//! However the bytes are cut into chunks, the events are the same.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// Empty if the server did not name it.
    pub name: String,
    pub data: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SseError {
    #[error("an event of the stream is larger than {0} bytes")]
    TooLarge(usize),
    #[error("the stream is not UTF-8")]
    NotUtf8,
}

/// One answer's events, until the first error. `id` and `retry` are ignored: an answer is never
/// resumed.
#[derive(Debug)]
pub struct Decoder {
    /// Most bytes of one event, in its lines and its fields.
    limit: usize,
    /// A line that has not ended.
    line: Vec<u8>,
    name: String,
    data: String,
    has_data: bool,
    /// The last byte was a CR; an LF after it belongs to it.
    cr: bool,
    /// No line has ended yet, so there may be a byte order mark.
    first: bool,
    failed: Option<SseError>,
}

impl Decoder {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            line: Vec::new(),
            name: String::new(),
            data: String::new(),
            has_data: false,
            cr: false,
            first: true,
            failed: None,
        }
    }

    /// Adds the events that end in `bytes` to `out`, also those before an error. An event that has
    /// not ended when the stream does is not an event.
    pub fn push(&mut self, bytes: &[u8], out: &mut Vec<Event>) -> Result<(), SseError> {
        let done = match self.failed {
            Some(e) => Err(e),
            None => self.bytes(bytes, out),
        };
        self.failed = done.err();
        done
    }

    fn bytes(&mut self, mut bytes: &[u8], out: &mut Vec<Event>) -> Result<(), SseError> {
        while let Some(&b) = bytes.first() {
            if std::mem::take(&mut self.cr) && b == b'\n' {
                bytes = &bytes[1..];
                continue;
            }
            let end = bytes.iter().position(|b| matches!(b, b'\n' | b'\r'));
            let part = &bytes[..end.unwrap_or(bytes.len())];
            if self.line.len() + self.name.len() + self.data.len() + part.len() > self.limit {
                return Err(SseError::TooLarge(self.limit));
            }
            self.line.extend_from_slice(part);
            let Some(end) = end else { break };
            self.cr = bytes[end] == b'\r';
            bytes = &bytes[end + 1..];
            let mut line = std::mem::take(&mut self.line);
            self.ended(&line, out)?;
            line.clear();
            self.line = line;
        }
        Ok(())
    }

    fn ended(&mut self, line: &[u8], out: &mut Vec<Event>) -> Result<(), SseError> {
        let line = std::str::from_utf8(line).map_err(|_| SseError::NotUtf8)?;
        let line = match std::mem::take(&mut self.first) {
            true => line.strip_prefix('\u{feff}').unwrap_or(line),
            false => line,
        };
        if line.is_empty() {
            let (name, data) = (std::mem::take(&mut self.name), std::mem::take(&mut self.data));
            if std::mem::take(&mut self.has_data) {
                out.push(Event { name, data });
            }
            return Ok(());
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => self.name = value.to_owned(),
            "data" => {
                if self.has_data {
                    self.data.push('\n');
                }
                self.has_data = true;
                self.data.push_str(value);
            }
            // A comment (no field), `id`, `retry`, and what the standard says to ignore.
            _ => {}
        }
        Ok(())
    }
}
