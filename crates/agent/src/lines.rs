//! Newline-delimited messages from a child process, with a bound on one line (AUD-109): a peer
//! that never sends `\n` cannot make the daemon buffer without limit.

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

/// The longest line accepted from an agent or MCP server (the language-server frame limit).
pub const MAX_LINE: usize = 64 * 1024 * 1024;

pub struct Lines<R> {
    reader: BufReader<R>,
    max: usize,
    buf: Vec<u8>,
}

impl<R: AsyncRead + Unpin> Lines<R> {
    pub fn new(r: R, max: usize) -> Self {
        Self {
            reader: BufReader::new(r),
            max,
            buf: Vec::new(),
        }
    }

    /// The next line without its `\n` (or `\r\n`); `None` at the end. A line longer than the
    /// bound, or one that is not UTF-8, is an error: the stream is not to be trusted further.
    /// Cancel-safe: a partly read line is kept for the next call.
    pub async fn next_line(&mut self) -> std::io::Result<Option<String>> {
        loop {
            let avail = self.reader.fill_buf().await?;
            if avail.is_empty() {
                if self.buf.is_empty() {
                    return Ok(None);
                }
                break;
            }
            let (take, done) = match avail.iter().position(|&b| b == b'\n') {
                Some(i) => (i, true),
                None => (avail.len(), false),
            };
            if self.buf.len() + take > self.max {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("a line is longer than {} bytes", self.max),
                ));
            }
            self.buf.extend_from_slice(&avail[..take]);
            self.reader.consume(if done { take + 1 } else { take });
            if done {
                break;
            }
        }
        if self.buf.last() == Some(&b'\r') {
            self.buf.pop();
        }
        String::from_utf8(std::mem::take(&mut self.buf))
            .map(Some)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lines_split_and_a_long_line_ends_the_stream() {
        let mut l = Lines::new(&b"a\r\nbc\n\nlast"[..], 8);
        assert_eq!(l.next_line().await.unwrap().as_deref(), Some("a"));
        assert_eq!(l.next_line().await.unwrap().as_deref(), Some("bc"));
        assert_eq!(l.next_line().await.unwrap().as_deref(), Some(""));
        assert_eq!(l.next_line().await.unwrap().as_deref(), Some("last"));
        assert_eq!(l.next_line().await.unwrap(), None);
        let long = [b'x'; 100];
        let mut l = Lines::new(&long[..], 8);
        assert!(l.next_line().await.is_err());
    }
}
