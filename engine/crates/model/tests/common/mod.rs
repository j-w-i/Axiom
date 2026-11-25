//! What the tests share: fixtures, decoding them, and a server that behaves as badly as told.
#![allow(dead_code)]

pub mod amazon;

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use axiom_model::{Accumulator, Codec, Error, Event, Quirks, Reply, Scope, codec::Decoder, sse};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Notify,
    time::timeout,
};

/// Long enough for any machine; a test that waits this long has failed.
pub const LONG: Duration = Duration::from_secs(60);
/// What a test waits for that is certain to come: a timeout.
pub const SHORT: Duration = Duration::from_millis(250);

pub fn fixture(path: &str) -> Vec<u8> {
    std::fs::read(format!("{}/fixtures/{path}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// A recorded answer's head and body.
pub fn recorded(name: &str) -> (String, Vec<u8>) {
    let raw = fixture(&format!("lmstudio-recorded/{name}.http"));
    let at = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8(raw[..at].to_vec()).unwrap();
    let mut rest = &raw[at + 4..];
    if !head.to_ascii_lowercase().contains("transfer-encoding: chunked") {
        return (head, rest.to_vec());
    }
    let mut body = Vec::new();
    loop {
        let at = rest.windows(2).position(|w| w == b"\r\n").unwrap();
        let size = std::str::from_utf8(&rest[..at]).unwrap();
        let size = usize::from_str_radix(size, 16).unwrap();
        rest = &rest[at + 2..];
        if size == 0 {
            return (head, body);
        }
        body.extend_from_slice(&rest[..size]);
        rest = &rest[size + 2..];
    }
}

pub fn scope(codec: Codec) -> Scope {
    Scope { codec, provider: "p".into(), model: "m".into() }
}

/// Everything a stream says, however it is cut.
pub fn events(codec: Codec, quirks: &Quirks, pieces: &[&[u8]]) -> (Vec<Event>, Option<Error>) {
    let (mut sse, mut decoder) = (sse::Decoder::new(1 << 20), Decoder::new(codec, quirks));
    let mut out = Vec::new();
    for piece in pieces {
        let mut events = Vec::new();
        sse.push(piece, &mut events).unwrap();
        for event in &events {
            if let Err(e) = decoder.event(event, &mut out) {
                return (out, Some(e));
            }
        }
    }
    let end = decoder.finish(&mut out);
    (out, end.err())
}

pub fn fold(codec: Codec, events: &[Event]) -> Reply {
    let mut all = Accumulator::new(scope(codec));
    for event in events {
        all.push(event).unwrap();
    }
    all.finish()
}

pub fn reply(codec: Codec, quirks: &Quirks, bytes: &[u8]) -> (Reply, Option<Error>) {
    let (events, error) = events(codec, quirks, &[bytes]);
    (fold(codec, &events), error)
}

/// `text` as unnamed events, one a line.
pub fn lines(text: &str) -> Vec<u8> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| format!("data: {l}\n\n"))
        .collect::<String>()
        .into_bytes()
}

#[derive(Clone)]
pub enum Step {
    Send(Vec<u8>),
    /// Until the client closes; then says so.
    UntilClosed(Arc<Notify>),
    /// Until the test says so.
    Wait(Arc<Notify>),
    /// This again and again, until the client closes.
    Forever(Vec<u8>),
    /// Closes without ending what was begun.
    Drop,
}
pub fn send(text: impl AsRef<[u8]>) -> Step {
    Step::Send(text.as_ref().to_vec())
}

#[derive(Debug, Clone)]
pub struct Asked {
    pub head: String,
    pub body: Vec<u8>,
}
impl Asked {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.head.lines().skip(1).find_map(|l| {
            let (n, v) = l.split_once(':')?;
            n.eq_ignore_ascii_case(name).then(|| v.trim())
        })
    }
}

/// Listens on this machine, does the same steps on every connection, keeps what it was asked.
pub struct Server {
    pub addr: SocketAddr,
    pub asked: Arc<Mutex<Vec<Asked>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Server {
    pub async fn new(steps: Vec<Step>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let (addr, asked) = (listener.local_addr().unwrap(), Arc::new(Mutex::new(Vec::new())));
        let seen = asked.clone();
        let task = tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                tokio::spawn(serve(socket, steps.clone(), seen.clone()));
            }
        });
        Self { addr, asked, task }
    }
    pub fn url(&self) -> String {
        format!("http://{}/v1", self.addr)
    }
    pub fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }
}

async fn serve(mut socket: TcpStream, steps: Vec<Step>, seen: Arc<Mutex<Vec<Asked>>>) {
    let mut raw = Vec::new();
    let asked = loop {
        let mut buf = [0; 4096];
        match socket.read(&mut buf).await {
            Ok(n) if n > 0 => raw.extend_from_slice(&buf[..n]),
            _ => {
                // Not HTTP, or gone: kept as it is.
                if !raw.is_empty() {
                    let head = String::from_utf8_lossy(&raw).into_owned();
                    seen.lock().unwrap().push(Asked { head, body: Vec::new() });
                }
                return;
            }
        }
        let Some(at) = raw.windows(4).position(|w| w == b"\r\n\r\n") else {
            // A TLS hello is not answered with more than the steps.
            if raw[0] == 0x16 {
                break Asked { head: String::from_utf8_lossy(&raw).into_owned(), body: Vec::new() };
            }
            continue;
        };
        let mut asked =
            Asked { head: String::from_utf8(raw[..at].to_vec()).unwrap(), body: Vec::new() };
        let length = asked.header("content-length").map_or(0, |l| l.parse().unwrap());
        if raw.len() >= at + 4 + length {
            asked.body = raw[at + 4..].to_vec();
            break asked;
        }
    };
    seen.lock().unwrap().push(asked);
    for step in steps {
        let done = match step {
            Step::Send(bytes) => socket.write_all(&bytes).await,
            Step::Wait(notify) => {
                notify.notified().await;
                Ok(())
            }
            Step::UntilClosed(notify) => {
                let mut buf = [0; 64];
                while matches!(socket.read(&mut buf).await, Ok(n) if n > 0) {}
                notify.notify_one();
                Ok(())
            }
            Step::Forever(bytes) => loop {
                if let Err(e) = socket.write_all(&bytes).await {
                    break Err(e);
                }
            },
            Step::Drop => return,
        };
        if done.is_err() {
            return;
        }
    }
    // The client has read everything before the connection goes away.
    let _ = socket.shutdown().await;
    let _ = timeout(LONG, async {
        let mut buf = [0; 64];
        while matches!(socket.read(&mut buf).await, Ok(n) if n > 0) {}
    })
    .await;
}

pub const STREAM: &str = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nrequest-id: req_head\r\nconnection: close\r\n\r\n";

/// A head for a body that ends with the connection.
pub fn head(status: &str, headers: &str) -> String {
    format!("HTTP/1.1 {status}\r\n{headers}connection: close\r\n\r\n")
}
