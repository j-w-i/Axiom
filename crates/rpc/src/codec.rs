//! NDJSON framing helpers shared by server and client.

use axiom_protocol::envelope::{JsonRpcVersion, Notification as WireNotification, Response};
use axiom_protocol::{ErrorKind, Message, RequestId, RpcError};
use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

/// Longest accepted line, in bytes (excluding the terminator).
pub(crate) const MAX_LINE: usize = 64 * 1024 * 1024;

#[derive(Debug)]
pub(crate) enum ReadLine {
    Line,
    Eof,
    TooLong,
}

/// Reads one `\n`-terminated line into `buf` (terminator and a trailing `\r`
/// removed), refusing lines longer than [`MAX_LINE`].
pub(crate) async fn read_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    buf: &mut Vec<u8>,
) -> std::io::Result<ReadLine> {
    buf.clear();
    loop {
        let chunk = reader.fill_buf().await?;
        if chunk.is_empty() {
            return Ok(if buf.is_empty() {
                ReadLine::Eof
            } else {
                strip_cr(buf);
                ReadLine::Line
            });
        }
        match chunk.iter().position(|b| *b == b'\n') {
            Some(pos) => {
                buf.extend_from_slice(&chunk[..pos]);
                reader.consume(pos + 1);
                if buf.len() > MAX_LINE {
                    return Ok(ReadLine::TooLong);
                }
                strip_cr(buf);
                return Ok(ReadLine::Line);
            }
            None => {
                let n = chunk.len();
                buf.extend_from_slice(chunk);
                reader.consume(n);
                if buf.len() > MAX_LINE {
                    return Ok(ReadLine::TooLong);
                }
            }
        }
    }
}

fn strip_cr(buf: &mut Vec<u8>) {
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
}

/// Parses a line; the error is the response-ready `RpcError`.
pub(crate) fn parse_message(line: &[u8]) -> Result<Message, RpcError> {
    let value: Value = serde_json::from_slice(line)
        .map_err(|e| RpcError::new(ErrorKind::ParseError, format!("Invalid JSON: {e}.")))?;
    serde_json::from_value(value).map_err(|e| {
        RpcError::new(ErrorKind::InvalidRequest, "Not a valid JSON-RPC message.")
            .with_details(serde_json::json!({ "reason": e.to_string() }))
    })
}

pub(crate) fn encode<T: serde::Serialize>(msg: &T) -> String {
    // Serialising our own wire types cannot fail: all maps have string keys.
    let mut s = serde_json::to_string(msg).unwrap_or_else(|e| {
        format!(
            r#"{{"jsonrpc":"2.0","id":null,"error":{{"code":-32603,"message":"Serialization failed: {}","data":{{"kind":"internal","retryable":false}}}}}}"#,
            e.to_string().replace(['"', '\\'], "'")
        )
    });
    s.push('\n');
    s
}

pub(crate) fn response_ok(id: RequestId, result: Value) -> String {
    encode(&Response {
        jsonrpc: JsonRpcVersion::V2,
        id: Some(id),
        result: Some(result),
        error: None,
    })
}

pub(crate) fn response_err(id: Option<RequestId>, error: RpcError) -> String {
    encode(&Response {
        jsonrpc: JsonRpcVersion::V2,
        id,
        result: None,
        error: Some(error),
    })
}

pub(crate) fn notification_line(method: &str, params: Value) -> String {
    encode(&WireNotification {
        jsonrpc: JsonRpcVersion::V2,
        method: method.to_string(),
        params: Some(params),
    })
}
