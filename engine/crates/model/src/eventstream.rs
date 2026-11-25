//! Amazon's event streams, `application/vnd.amazon.eventstream`
//! (smithy.io/2.0/aws/amazon-eventstream.html, read 2026-11-25).
//!
//! However the bytes are cut into chunks, the messages are the same. Nothing of a message is believed
//! before its checksum: its lengths after that of its first twelve bytes, the rest after that of it all.

/// Lengths and their checksum.
const PRELUDE: usize = 12;
/// What a message has beside its headers and its payload.
const FRAME: usize = PRELUDE + 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Bytes(Vec<u8>),
    Text(String),
    /// Milliseconds since 1970.
    Time(i64),
    Uuid([u8; 16]),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// Each name once.
    pub headers: Vec<(String, Value)>,
    pub payload: Vec<u8>,
}
impl Message {
    /// A header that is text.
    pub fn text(&self, name: &str) -> Option<&str> {
        self.headers.iter().find_map(|(n, v)| match v {
            Value::Text(text) if n == name => Some(text.as_str()),
            _ => None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    #[error("a message of the stream is larger than {0} bytes")]
    TooLarge(usize),
    #[error("a message of the stream has lengths that cannot be")]
    Length,
    #[error("a message of the stream is damaged")]
    Checksum,
    #[error("a message of the stream has headers that cannot be read")]
    Headers,
}

/// One answer's messages, until the first error.
#[derive(Debug)]
pub struct Decoder {
    /// Most bytes of one message. The specification has a client hold to none, which memory does not
    /// allow.
    limit: usize,
    /// A message that has not ended.
    held: Vec<u8>,
    /// All of it and its headers, once the prelude is there.
    lengths: Option<(usize, usize)>,
    failed: Option<FrameError>,
}

impl Decoder {
    pub fn new(limit: usize) -> Self {
        Self { limit, held: Vec::new(), lengths: None, failed: None }
    }

    /// Adds the messages that end in `bytes` to `out`, also those before an error. A message that has
    /// not ended when the stream does is not a message.
    pub fn push(&mut self, bytes: &[u8], out: &mut Vec<Message>) -> Result<(), FrameError> {
        let done = match self.failed {
            Some(e) => Err(e),
            None => self.bytes(bytes, out),
        };
        self.failed = done.err();
        done
    }

    fn bytes(&mut self, mut bytes: &[u8], out: &mut Vec<Message>) -> Result<(), FrameError> {
        while !bytes.is_empty() {
            let whole = self.lengths.map_or(PRELUDE, |(total, _)| total);
            let more = bytes.len().min(whole.saturating_sub(self.held.len()));
            let (part, rest) = bytes.split_at(more);
            self.held.extend_from_slice(part);
            bytes = rest;
            if self.held.len() < whole {
                return Ok(());
            }
            match self.lengths.take() {
                None => self.lengths = Some(self.prelude()?),
                Some((_, headers)) => {
                    let message = message(&self.held, headers);
                    self.held.clear();
                    out.push(message?);
                }
            }
        }
        Ok(())
    }

    fn prelude(&self) -> Result<(usize, usize), FrameError> {
        let mut read = Reader(&self.held);
        let (total, headers) = (read.u32()?, read.u32()?);
        if read.u32()? != crc32(&self.held[..8]) {
            return Err(FrameError::Checksum);
        }
        let size = |n: u32| usize::try_from(n).map_err(|_| FrameError::TooLarge(self.limit));
        let (total, headers) = (size(total)?, size(headers)?);
        if total > self.limit {
            return Err(FrameError::TooLarge(self.limit));
        }
        match total.checked_sub(FRAME) {
            Some(rest) if headers <= rest => Ok((total, headers)),
            _ => Err(FrameError::Length),
        }
    }
}

/// A whole message, whose lengths are right.
fn message(all: &[u8], headers: usize) -> Result<Message, FrameError> {
    let (body, sum) = all.split_at(all.len().saturating_sub(4));
    if Reader(sum).u32()? != crc32(body) {
        return Err(FrameError::Checksum);
    }
    let rest = body.get(PRELUDE..).ok_or(FrameError::Length)?;
    let (head, payload) = rest.split_at_checked(headers).ok_or(FrameError::Length)?;
    let mut read = Reader(head);
    let mut headers: Vec<(String, Value)> = Vec::new();
    let mut names = std::collections::HashSet::new();
    while !read.0.is_empty() {
        let length = read.u8()?;
        let name = read.text(length.into())?;
        // "Header names MUST be at least one byte long", "MUST only appear once".
        if name.is_empty() || !names.insert(name.clone()) {
            return Err(FrameError::Headers);
        }
        headers.push((name, read.value()?));
    }
    Ok(Message { headers, payload: payload.to_vec() })
}

struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], FrameError> {
        let (taken, rest) = self.0.split_at_checked(n).ok_or(FrameError::Headers)?;
        self.0 = rest;
        Ok(taken)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], FrameError> {
        self.take(N)?.try_into().map_err(|_| FrameError::Headers)
    }
    fn u8(&mut self) -> Result<u8, FrameError> {
        Ok(u8::from_be_bytes(self.array()?))
    }
    fn u32(&mut self) -> Result<u32, FrameError> {
        Ok(u32::from_be_bytes(self.array()?))
    }
    /// With its length in two bytes before it.
    fn bytes(&mut self) -> Result<&'a [u8], FrameError> {
        let length = u16::from_be_bytes(self.array()?);
        self.take(length.into())
    }
    fn text(&mut self, n: usize) -> Result<String, FrameError> {
        utf8(self.take(n)?)
    }
    fn value(&mut self) -> Result<Value, FrameError> {
        Ok(match self.u8()? {
            0 => Value::Bool(true),
            1 => Value::Bool(false),
            2 => Value::Byte(i8::from_be_bytes(self.array()?)),
            3 => Value::Short(i16::from_be_bytes(self.array()?)),
            4 => Value::Int(i32::from_be_bytes(self.array()?)),
            5 => Value::Long(i64::from_be_bytes(self.array()?)),
            6 => Value::Bytes(self.bytes()?.to_vec()),
            7 => Value::Text(utf8(self.bytes()?)?),
            8 => Value::Time(i64::from_be_bytes(self.array()?)),
            9 => Value::Uuid(self.array()?),
            // "The set of types is fixed": how long another would be cannot be known.
            _ => return Err(FrameError::Headers),
        })
    }
}

fn utf8(bytes: &[u8]) -> Result<String, FrameError> {
    String::from_utf8(bytes.to_vec()).map_err(|_| FrameError::Headers)
}

/// As gzip has it.
fn crc32(bytes: &[u8]) -> u32 {
    !bytes.iter().fold(!0, |crc, b| (crc >> 8) ^ TABLE[usize::from(crc as u8 ^ b)])
}
const TABLE: [u32; 256] = {
    let mut table = [0; 256];
    let mut n = 0;
    while n < 256 {
        let mut crc = n as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
            bit += 1;
        }
        table[n] = crc;
        n += 1;
    }
    table
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checksum() {
        // Its check value (reveng.sourceforge.io/crc-catalogue, CRC-32/ISO-HDLC).
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }
}
