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

/// Parses a line; the error is the response-ready `RpcError` with the id to answer: the
/// message's own when it is a request whose id can be read (JSON-RPC 2.0 §5: `null` only when
/// the id cannot be determined; AUD-096), else `None`. A message without `method` is never
/// answered with its id: it may be a malformed response, and answering it could loop.
pub(crate) fn parse_message(line: &[u8]) -> Result<Message, (Option<RequestId>, RpcError)> {
    let value: Value = serde_json::from_slice(line).map_err(|e| {
        (
            None,
            RpcError::new(ErrorKind::ParseError, format!("Invalid JSON: {e}.")),
        )
    })?;
    let id = value
        .as_object()
        .filter(|o| o.contains_key("method"))
        .and_then(|o| o.get("id"))
        .and_then(|id| serde_json::from_value::<RequestId>(id.clone()).ok());
    serde_json::from_value(value).map_err(|e| {
        (
            id,
            RpcError::new(ErrorKind::InvalidRequest, "Not a valid JSON-RPC message.")
                .with_details(serde_json::json!({ "reason": e.to_string() })),
        )
    })
}

/// A line longer than [`MAX_LINE`] is a fatal protocol error for the peer (ipc.md §Transport):
/// such a response is replaced by a `too_large` error for its id (AUD-100).
fn too_large(id: Option<RequestId>, len: usize) -> String {
    encode(&Response {
        jsonrpc: JsonRpcVersion::V2,
        id,
        result: None,
        error: Some(
            RpcError::new(
                ErrorKind::TooLarge,
                "The response is larger than the protocol's 64 MiB line limit.",
            )
            .with_details(serde_json::json!({ "bytes": len, "limit": MAX_LINE })),
        ),
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
    let line = encode(&Response {
        jsonrpc: JsonRpcVersion::V2,
        id: Some(id.clone()),
        result: Some(result),
        error: None,
    });
    if line.len() - 1 > MAX_LINE {
        tracing::error!(
            "the response to {id:?} is {} bytes: sent as too_large",
            line.len()
        );
        return too_large(Some(id), line.len() - 1);
    }
    line
}

pub(crate) fn response_err(id: Option<RequestId>, error: RpcError) -> String {
    let line = encode(&Response {
        jsonrpc: JsonRpcVersion::V2,
        id: id.clone(),
        result: None,
        error: Some(error),
    });
    if line.len() - 1 > MAX_LINE {
        return too_large(id, line.len() - 1);
    }
    line
}

/// The notification's line, or `None` if it would exceed [`MAX_LINE`] (it is dropped: a peer
/// would have to close the connection on it).
pub(crate) fn notification_line(method: &str, params: Value) -> Option<String> {
    let line = encode(&WireNotification {
        jsonrpc: JsonRpcVersion::V2,
        method: method.to_string(),
        params: Some(params),
    });
    if line.len() - 1 > MAX_LINE {
        tracing::error!(
            "a {method} notification of {} bytes was dropped",
            line.len()
        );
        return None;
    }
    Some(line)
}
