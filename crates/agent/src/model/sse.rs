//! Server-sent events (the WHATWG `text/event-stream` format), parsed from a byte stream. Only
//! what model APIs use: `event`, `data` (multi-line joined with `\n`), comments, `\n` or `\r\n`
//! line ends. `id` and `retry` are ignored.

use futures_util::{Stream, StreamExt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    /// The `event:` field, if any.
    pub event: Option<String>,
    pub data: String,
}

/// The longest line, and the most data in one event, a stream may send: past it the stream is
/// broken (or hostile) and ends (AUD-109).
pub const MAX_EVENT_BYTES: usize = 16 * 1024 * 1024;

/// Incremental parser: feed bytes, take complete events.
#[derive(Debug, Default)]
pub struct SseParser {
    buf: Vec<u8>,
    event: Option<String>,
    data: Vec<String>,
    data_bytes: usize,
    overflow: bool,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends bytes and returns every event completed by them. Only the new bytes are
    /// scanned, and the consumed lines are removed once per call.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        let mut out = Vec::new();
        if self.overflow {
            return out;
        }
        let mut scan = self.buf.len();
        self.buf.extend_from_slice(bytes);
        let mut start = 0;
        while let Some(rel) = self.buf[scan..].iter().position(|&b| b == b'\n') {
            let pos = scan + rel;
            let mut line = &self.buf[start..pos];
            if line.last() == Some(&b'\r') {
                line = &line[..line.len() - 1];
            }
            let line = String::from_utf8_lossy(line).into_owned();
            start = pos + 1;
            scan = start;
            if let Some(ev) = self.line(&line) {
                out.push(ev);
            }
            if self.overflow {
                self.buf.clear();
                return out;
            }
        }
        self.buf.drain(..start);
        if self.buf.len() > MAX_EVENT_BYTES {
            self.overflow = true;
            self.buf.clear();
        }
        out
    }

    /// A line or an event was larger than [`MAX_EVENT_BYTES`]: nothing more is parsed.
    pub fn overflowed(&self) -> bool {
        self.overflow
    }

    /// The stream ended: an event without its blank line is still dispatched.
    pub fn finish(&mut self) -> Option<SseEvent> {
        let rest = std::mem::take(&mut self.buf);
        if !rest.is_empty() {
            let line = String::from_utf8_lossy(&rest).into_owned();
            let line = line.strip_suffix('\r').unwrap_or(&line).to_string();
            if let Some(ev) = self.line(&line) {
                return Some(ev);
            }
        }
        self.dispatch()
    }

    fn line(&mut self, line: &str) -> Option<SseEvent> {
        if line.is_empty() {
            return self.dispatch();
        }
        if line.starts_with(':') {
            return None;
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "event" => self.event = Some(value.to_string()),
            "data" => {
                self.data_bytes += value.len() + 1;
                if self.data_bytes > MAX_EVENT_BYTES {
                    self.overflow = true;
                    self.data.clear();
                    self.event = None;
                    return None;
                }
                self.data.push(value.to_string())
            }
            _ => {}
        }
        None
    }

    fn dispatch(&mut self) -> Option<SseEvent> {
        if self.data.is_empty() {
            self.event = None;
            self.data_bytes = 0;
            return None;
        }
        self.data_bytes = 0;
        let data = std::mem::take(&mut self.data).join("\n");
        Some(SseEvent {
            event: self.event.take(),
            data,
        })
    }
}

/// Turns a byte stream into an event stream. A transport error ends it with that error.
pub fn events<S, B, E>(bytes: S) -> impl Stream<Item = Result<SseEvent, E>>
where
    S: Stream<Item = Result<B, E>> + Unpin,
    B: AsRef<[u8]>,
{
    futures_util::stream::unfold(
        (
            bytes,
            SseParser::new(),
            std::collections::VecDeque::new(),
            false,
        ),
        |(mut bytes, mut parser, mut queue, mut done)| async move {
            loop {
                if let Some(ev) = queue.pop_front() {
                    return Some((Ok(ev), (bytes, parser, queue, done)));
                }
                if done {
                    return None;
                }
                match bytes.next().await {
                    Some(Ok(chunk)) => {
                        queue.extend(parser.feed(chunk.as_ref()));
                        // A line or event past the bound: the stream ends here, early.
                        done = parser.overflowed();
                    }
                    Some(Err(e)) => {
                        done = true;
                        return Some((Err(e), (bytes, parser, queue, done)));
                    }
                    None => {
                        done = true;
                        queue.extend(parser.finish());
                    }
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(event: Option<&str>, data: &str) -> SseEvent {
        SseEvent {
            event: event.map(String::from),
            data: data.into(),
        }
    }

    #[test]
    fn sse_parses_fields_comments_and_crlf() {
        let mut p = SseParser::new();
        let got = p.feed(b": ping\r\nevent: a\r\ndata: 1\r\n\r\ndata:2\ndata: 3\n\n");
        assert_eq!(got, vec![ev(Some("a"), "1"), ev(None, "2\n3")]);
    }

    #[test]
    fn sse_handles_events_split_across_chunks_at_every_byte() {
        let input = b"event: message_start\ndata: {\"x\": 1}\n\nevent: ping\ndata: {}\n\n";
        for split in 1..input.len() {
            let mut p = SseParser::new();
            let mut got = p.feed(&input[..split]);
            got.extend(p.feed(&input[split..]));
            assert_eq!(
                got,
                vec![
                    ev(Some("message_start"), "{\"x\": 1}"),
                    ev(Some("ping"), "{}")
                ],
                "split at {split}"
            );
        }
    }

    #[test]
    fn sse_dispatches_a_trailing_event_without_its_blank_line() {
        let mut p = SseParser::new();
        assert!(p.feed(b"data: [DONE]").is_empty());
        assert_eq!(p.finish(), Some(ev(None, "[DONE]")));
    }

    #[test]
    fn sse_ignores_an_event_line_without_data() {
        let mut p = SseParser::new();
        assert_eq!(p.feed(b"event: x\n\ndata: y\n\n"), vec![ev(None, "y")]);
    }

    #[test]
    fn sse_an_endless_line_is_bounded() {
        let mut p = SseParser::new();
        let chunk = vec![b'x'; 1024 * 1024];
        for _ in 0..=MAX_EVENT_BYTES / chunk.len() {
            assert!(p.feed(&chunk).is_empty());
        }
        assert!(p.overflowed());
        assert!(p.buf.capacity() <= 2 * MAX_EVENT_BYTES + chunk.len());
        assert!(p.feed(b"\ndata: a\n\n").is_empty());
    }

    #[test]
    fn sse_many_lines_in_one_chunk() {
        let mut p = SseParser::new();
        let input = "data: x\n\n".repeat(10_000);
        assert_eq!(p.feed(input.as_bytes()).len(), 10_000);
        assert!(p.buf.is_empty());
    }

    #[tokio::test]
    async fn sse_events_adapter_propagates_errors() {
        let chunks: Vec<Result<&[u8], &str>> = vec![Ok(b"data: a\n\n"), Err("boom")];
        let out: Vec<_> = events(futures_util::stream::iter(chunks)).collect().await;
        assert_eq!(out, vec![Ok(ev(None, "a")), Err("boom")]);
    }
}
