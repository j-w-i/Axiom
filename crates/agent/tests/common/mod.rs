//! A loopback HTTP/1.1 server that serves canned responses in order and records requests.
//! Responses are close-delimited (no Content-Length), written in chunks with optional delays,
//! so streaming clients see the bytes arrive over time.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    /// Lower-cased header names.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).expect("request body is JSON")
    }
}

#[derive(Debug, Clone)]
pub struct Canned {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub chunks: Vec<String>,
    pub delay: Duration,
    /// Keep the connection open after the last chunk (until the client goes away).
    pub hang: bool,
}

impl Canned {
    pub fn sse(body: &str) -> Self {
        // One chunk per event, so the client parses incrementally.
        let chunks = body
            .split_inclusive("\n\n")
            .map(String::from)
            .collect::<Vec<_>>();
        Self {
            status: 200,
            headers: vec![("content-type".into(), "text/event-stream".into())],
            chunks,
            delay: Duration::from_millis(1),
            hang: false,
        }
    }
    pub fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: vec![("content-type".into(), "application/json".into())],
            chunks: vec![body.to_string()],
            delay: Duration::ZERO,
            hang: false,
        }
    }
    pub fn header(mut self, k: &str, v: &str) -> Self {
        self.headers.push((k.into(), v.into()));
        self
    }
    pub fn hang(mut self) -> Self {
        self.hang = true;
        self
    }
}

pub struct Server {
    pub url: String,
    pub requests: Arc<Mutex<Vec<Recorded>>>,
}

impl Server {
    pub async fn start(responses: Vec<Canned>) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests: Arc<Mutex<Vec<Recorded>>> = Arc::default();
        let queue = Arc::new(Mutex::new(VecDeque::from(responses)));
        let rec = requests.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let rec = rec.clone();
                let queue = queue.clone();
                tokio::spawn(async move {
                    let Some(req) = read_request(&mut sock).await else {
                        return;
                    };
                    rec.lock().unwrap().push(req);
                    let resp = queue.lock().unwrap().pop_front().unwrap_or(Canned::json(
                        500,
                        r#"{"error":{"message":"no canned response left"}}"#,
                    ));
                    let mut head = format!("HTTP/1.1 {} X\r\nconnection: close\r\n", resp.status);
                    for (k, v) in &resp.headers {
                        head.push_str(&format!("{k}: {v}\r\n"));
                    }
                    head.push_str("\r\n");
                    if sock.write_all(head.as_bytes()).await.is_err() {
                        return;
                    }
                    for c in &resp.chunks {
                        if !resp.delay.is_zero() {
                            tokio::time::sleep(resp.delay).await;
                        }
                        if sock.write_all(c.as_bytes()).await.is_err() {
                            return;
                        }
                        let _ = sock.flush().await;
                    }
                    if resp.hang {
                        let mut b = [0u8; 1];
                        let _ = sock.read(&mut b).await;
                    }
                });
            }
        });
        Server { url, requests }
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }
}

async fn read_request(sock: &mut tokio::net::TcpStream) -> Option<Recorded> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let head_end = loop {
        let n = sock.read(&mut tmp).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_string();
    let path = first.next()?.to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    let len: usize = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let mut body = buf[head_end..].to_vec();
    while body.len() < len {
        let n = sock.read(&mut tmp).await.ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    Some(Recorded {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}
