//! LSP base protocol: `Content-Length` headers, then a UTF-8 JSON body.

use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// The largest message accepted from a server.
pub const MAX_MESSAGE: usize = 64 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("bad header: {0}")]
    Header(String),
    #[error("message of {0} bytes is over the limit")]
    TooLarge(usize),
    #[error("bad JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// Reads one message; `Ok(None)` at a clean end of stream.
pub async fn read_message<R: AsyncBufRead + Unpin>(r: &mut R) -> Result<Option<Value>, FrameError> {
    let mut length: Option<usize> = None;
    let mut line = String::new();
    let mut any = false;
    loop {
        line.clear();
        let n = r.read_line(&mut line).await?;
        if n == 0 {
            return if any {
                Err(FrameError::Header("end of stream inside a header".into()))
            } else {
                Ok(None)
            };
        }
        any = true;
        let l = line.trim_end_matches(['\r', '\n']);
        if l.is_empty() {
            break;
        }
        let Some((name, value)) = l.split_once(':') else {
            return Err(FrameError::Header(l.to_string()));
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            length = Some(
                value
                    .trim()
                    .parse()
                    .map_err(|_| FrameError::Header(l.to_string()))?,
            );
        }
    }
    let len = length.ok_or_else(|| FrameError::Header("no Content-Length".into()))?;
    if len > MAX_MESSAGE {
        return Err(FrameError::TooLarge(len));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).await?;
    Ok(Some(serde_json::from_slice(&body)?))
}

pub async fn write_message<W: AsyncWrite + Unpin>(w: &mut W, v: &Value) -> Result<(), FrameError> {
    let body = serde_json::to_vec(v)?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    w.write_all(header.as_bytes()).await?;
    w.write_all(&body).await?;
    w.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn round_trip_and_headers() {
        let mut buf = Vec::new();
        write_message(
            &mut buf,
            &json!({"jsonrpc":"2.0","method":"x","params":{"t":"π"}}),
        )
        .await
        .unwrap();
        write_message(&mut buf, &json!({"id":1})).await.unwrap();
        let mut r = tokio::io::BufReader::new(&buf[..]);
        assert_eq!(
            read_message(&mut r).await.unwrap().unwrap()["params"]["t"],
            "π"
        );
        assert_eq!(read_message(&mut r).await.unwrap().unwrap()["id"], 1);
        assert!(read_message(&mut r).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn other_headers_and_case_are_accepted() {
        let raw = b"content-length: 2\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n{}";
        let mut r = tokio::io::BufReader::new(&raw[..]);
        assert_eq!(read_message(&mut r).await.unwrap().unwrap(), json!({}));
    }

    #[tokio::test]
    async fn oversize_and_missing_length_fail() {
        let raw = format!("Content-Length: {}\r\n\r\n", MAX_MESSAGE + 1);
        let mut r = tokio::io::BufReader::new(raw.as_bytes());
        assert!(matches!(
            read_message(&mut r).await,
            Err(FrameError::TooLarge(_))
        ));
        let mut r = tokio::io::BufReader::new(&b"X-Other: 1\r\n\r\n{}"[..]);
        assert!(matches!(
            read_message(&mut r).await,
            Err(FrameError::Header(_))
        ));
    }
}
