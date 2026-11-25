//! A client of the host, for the CLI, the tests and the benchmark.

use std::{collections::VecDeque, io, time::Duration};

use axiom_proto::*;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{net, paths::Paths};

pub struct Client {
    s: net::Boxed,
    lines: Lines,
    buf: Vec<u8>,
    next: u64,
    /// Messages that arrived while waiting for an answer.
    pub queue: VecDeque<Message>,
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Refused(#[from] WireError),
    #[error("the host sent something unreadable: {0}")]
    Unreadable(String),
    #[error("the host closed the connection")]
    Closed,
}

impl Client {
    pub async fn connect(paths: &Paths, name: &str) -> Result<Self, ClientError> {
        let token = tokio::fs::read_to_string(paths.token()).await?;
        let mut c = Self::raw(paths).await?;
        c.call(Call::Hello { token, client: name.into(), protocol: PROTOCOL }).await?;
        Ok(c)
    }

    pub async fn raw(paths: &Paths) -> Result<Self, ClientError> {
        Ok(Self {
            s: net::connect(paths).await?,
            lines: Lines::default(),
            buf: vec![0; 64 * 1024],
            next: 1,
            queue: VecDeque::new(),
        })
    }

    pub async fn send_raw(&mut self, bytes: &[u8]) -> Result<(), ClientError> {
        Ok(self.s.write_all(bytes).await?)
    }

    pub async fn call(&mut self, call: Call) -> Result<Value, ClientError> {
        let id = self.next;
        self.next += 1;
        let mut b = serde_json::to_vec(&Request { id, call })
            .map_err(|e| ClientError::Unreadable(e.to_string()))?;
        b.push(b'\n');
        self.s.write_all(&b).await?;
        loop {
            match self.read().await? {
                Message::Ok { id: i, result } if i == id => return Ok(result),
                Message::Err { id: i, error } if i == id || i == 0 => return Err(error.into()),
                m => self.queue.push_back(m),
            }
        }
    }

    pub async fn next(&mut self) -> Result<Message, ClientError> {
        match self.queue.pop_front() {
            Some(m) => Ok(m),
            None => self.read().await,
        }
    }

    pub async fn next_within(&mut self, d: Duration) -> Result<Option<Message>, ClientError> {
        match tokio::time::timeout(d, self.next()).await {
            Ok(m) => m.map(Some),
            Err(_) => Ok(None),
        }
    }

    async fn read(&mut self) -> Result<Message, ClientError> {
        loop {
            let line =
                self.lines.next_line().map_err(|e| ClientError::Unreadable(e.to_string()))?;
            if let Some(l) = line {
                return serde_json::from_slice(&l).map_err(|e| {
                    let start: String = String::from_utf8_lossy(&l).chars().take(120).collect();
                    ClientError::Unreadable(format!("{e}: {start}"))
                });
            }
            let n = self.s.read(&mut self.buf).await?;
            if n == 0 {
                return Err(ClientError::Closed);
            }
            self.lines.push(&self.buf[..n]);
        }
    }
}
