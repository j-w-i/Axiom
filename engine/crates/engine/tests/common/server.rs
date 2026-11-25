//! A provider that answers from a script. Shared with the host's tests.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde_json::{Value, json};

#[path = "../../../model/tests/common/amazon.rs"]
mod amazon;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

/// One answer of the server.
#[derive(Clone)]
pub struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<Vec<u8>>,
    /// The connection ends before the answer does.
    pub cut: bool,
    hang: bool,
}
impl Reply {
    pub fn header(mut self, k: &str, v: &str) -> Self {
        self.headers.push((k.into(), v.into()));
        self
    }
    /// Why it stopped.
    pub fn stop(&mut self, reason: &str) {
        for b in &mut self.body {
            for was in ["stop", "tool_calls"] {
                let (a, z) =
                    (format!("finish_reason\":\"{was}\""), format!("finish_reason\":\"{reason}\""));
                *b = String::from_utf8(b.clone()).unwrap().replace(&a, &z).into_bytes();
            }
        }
    }
    /// After what it says, nothing more comes.
    pub fn hangs(mut self) -> Self {
        self.hang = true;
        self
    }
}

fn data(v: Value) -> String {
    format!("data: {v}\n\n")
}
fn chunk(delta: Value, finish: Value) -> String {
    data(
        json!({"id": "x", "model": "m", "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]}),
    )
}
fn ends(finish: &str) -> Vec<String> {
    vec![
        chunk(json!({}), json!(finish)),
        data(
            json!({"id": "x", "choices": [], "usage": {"prompt_tokens": 50, "completion_tokens": 5, "total_tokens": 55}}),
        ),
        "data: [DONE]\n\n".into(),
    ]
}
fn sse(body: Vec<String>) -> Reply {
    Reply {
        status: 200,
        headers: vec![("content-type".into(), "text/event-stream".into())],
        body: body.into_iter().map(String::into_bytes).collect(),
        cut: false,
        hang: false,
    }
}
pub fn text(parts: &[&str]) -> Reply {
    let mut b: Vec<String> =
        parts.iter().map(|p| chunk(json!({"content": p}), Value::Null)).collect();
    b.extend(ends("stop"));
    sse(b)
}
pub fn ends_with(reason: &str, parts: &[&str]) -> Reply {
    let mut r = text(parts);
    r.stop(reason);
    r
}
pub fn cut(part: &str) -> Reply {
    Reply { cut: true, ..sse(vec![chunk(json!({"content": part}), Value::Null)]) }
}
pub fn calls(all: &[(&str, &str, &str)]) -> Reply {
    let mut b: Vec<String> = all
        .iter()
        .enumerate()
        .map(|(i, (id, name, args))| {
            chunk(
                json!({"tool_calls": [{"index": i, "id": id, "type": "function", "function": {"name": name, "arguments": args}}]}),
                Value::Null,
            )
        })
        .collect();
    b.extend(ends("tool_calls"));
    sse(b)
}
/// The same in Anthropic's form: thinking that is signed, then words or calls.
pub fn anthropic(
    thinking: Option<(&str, &str)>,
    words: &str,
    calls: &[(&str, &str, &str)],
) -> Reply {
    let event = |v: Value| format!("event: {}\ndata: {v}\n\n", v["type"].as_str().unwrap());
    let mut b = vec![event(json!({"type": "message_start", "message": {
        "id": "msg_1", "type": "message", "role": "assistant", "model": "claude-test", "content": [],
        "stop_reason": null, "usage": {"input_tokens": 40, "output_tokens": 1}}}))];
    let mut i = 0;
    let mut block = |start: Value, deltas: Vec<Value>| {
        b.push(event(json!({"type": "content_block_start", "index": i, "content_block": start})));
        for d in deltas {
            b.push(event(json!({"type": "content_block_delta", "index": i, "delta": d})));
        }
        b.push(event(json!({"type": "content_block_stop", "index": i})));
        i += 1;
    };
    if let Some((t, signature)) = thinking {
        block(
            json!({"type": "thinking", "thinking": "", "signature": ""}),
            vec![
                json!({"type": "thinking_delta", "thinking": t}),
                json!({"type": "signature_delta", "signature": signature}),
            ],
        );
    }
    if !words.is_empty() {
        block(
            json!({"type": "text", "text": ""}),
            vec![json!({"type": "text_delta", "text": words})],
        );
    }
    for (id, name, args) in calls {
        let half = args.len() / 2;
        block(
            json!({"type": "tool_use", "id": id, "name": name, "input": {}}),
            [&args[..half], &args[half..]]
                .iter()
                .map(|p| json!({"type": "input_json_delta", "partial_json": p}))
                .collect(),
        );
    }
    let stop = if calls.is_empty() { "end_turn" } else { "tool_use" };
    b.push(event(
        json!({"type": "message_delta", "delta": {"stop_reason": stop, "stop_sequence": null},
        "usage": {"output_tokens": 9}}),
    ));
    b.push(event(json!({"type": "message_stop"})));
    sse(b)
}
/// The same as Bedrock has it.
pub fn bedrock(thinking: Option<(&str, &str)>, words: &str, calls: &[(&str, &str, &str)]) -> Reply {
    let mut lines = vec![json!({"messageStart": {"role": "assistant"}})];
    let mut i = 0;
    let mut block = |start: Option<Value>, deltas: Vec<Value>| {
        if let Some(s) = start {
            lines.push(json!({"contentBlockStart": {"contentBlockIndex": i, "start": s}}));
        }
        for d in deltas {
            lines.push(json!({"contentBlockDelta": {"contentBlockIndex": i, "delta": d}}));
        }
        lines.push(json!({"contentBlockStop": {"contentBlockIndex": i}}));
        i += 1;
    };
    if let Some((t, signature)) = thinking {
        block(
            None,
            vec![
                json!({"reasoningContent": {"text": t}}),
                json!({"reasoningContent": {"signature": signature}}),
            ],
        );
    }
    if !words.is_empty() {
        block(None, vec![json!({"text": words})]);
    }
    for (id, name, args) in calls {
        block(
            Some(json!({"toolUse": {"toolUseId": id, "name": name}})),
            vec![json!({"toolUse": {"input": args}})],
        );
    }
    let stop = if calls.is_empty() { "end_turn" } else { "tool_use" };
    lines.push(json!({"messageStop": {"stopReason": stop}}));
    lines.push(
        json!({"metadata": {"usage": {"inputTokens": 40, "outputTokens": 9, "totalTokens": 49},
        "metrics": {"latencyMs": 1}}}),
    );
    eventstream(lines.iter().map(|l| amazon::stream(&l.to_string())).collect())
}
/// Begins to answer, and then says what went wrong.
pub fn bedrock_exception(words: &str, name: &str, body: &str) -> Reply {
    let begun = format!(
        "{}\n{}",
        json!({"messageStart": {"role": "assistant"}}),
        json!({"contentBlockDelta": {"contentBlockIndex": 0, "delta": {"text": words}}})
    );
    eventstream(vec![amazon::stream(&begun), amazon::exception(name, body)])
}
fn eventstream(body: Vec<Vec<u8>>) -> Reply {
    let mut r = sse(Vec::new());
    r.headers[0].1 = "application/vnd.amazon.eventstream".into();
    Reply { body, ..r }
}
pub fn status(code: u16, body: &str) -> Reply {
    Reply {
        status: code,
        headers: vec![("content-type".into(), "application/json".into())],
        body: vec![body.into()],
        cut: false,
        hang: false,
    }
}

#[derive(Clone)]
pub struct Server {
    pub url: String,
    script: Arc<Mutex<VecDeque<Reply>>>,
    requests: Arc<Mutex<Vec<Value>>>,
    closed: Arc<Mutex<usize>>,
    keys: Arc<Mutex<Vec<String>>>,
}

impl Server {
    pub async fn start(script: Vec<Reply>) -> Self {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let s = Self {
            url: format!("http://127.0.0.1:{}/v1", l.local_addr().unwrap().port()),
            script: Arc::new(Mutex::new(script.into())),
            requests: Arc::default(),
            closed: Arc::default(),
            keys: Arc::default(),
        };
        let me = s.clone();
        tokio::spawn(async move {
            while let Ok((mut c, _)) = l.accept().await {
                let me = me.clone();
                tokio::spawn(async move { me.serve(&mut c).await });
            }
        });
        s
    }
    async fn serve(&self, c: &mut tokio::net::TcpStream) -> Option<()> {
        let mut buf = Vec::new();
        let head = loop {
            let mut b = [0; 8192];
            let n = c.read(&mut b).await.ok().filter(|n| *n > 0)?;
            buf.extend_from_slice(&b[..n]);
            if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let text = String::from_utf8_lossy(&buf[..head]).into_owned();
        let header = |name: &str| {
            text.lines().find_map(|l| {
                let (k, v) = l.split_once(':')?;
                k.eq_ignore_ascii_case(name).then(|| v.trim().to_owned())
            })
        };
        if let Some(k) = header("authorization") {
            self.keys.lock().unwrap().push(k);
        }
        let len: usize = header("content-length").and_then(|n| n.parse().ok()).unwrap_or(0);
        while buf.len() < head + len {
            let mut b = [0; 8192];
            let n = c.read(&mut b).await.ok().filter(|n| *n > 0)?;
            buf.extend_from_slice(&b[..n]);
        }
        if len > 0 {
            self.requests
                .lock()
                .unwrap()
                .push(serde_json::from_slice(&buf[head..head + len]).ok()?);
        }
        let r = self
            .script
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| status(418, "no more answers"));
        let mut h = format!(
            "HTTP/1.1 {} X\r\ntransfer-encoding: chunked\r\nconnection: close\r\n",
            r.status
        );
        for (k, v) in &r.headers {
            h += &format!("{k}: {v}\r\n");
        }
        c.write_all(format!("{h}\r\n").as_bytes()).await.ok()?;
        for b in &r.body {
            let text = String::from_utf8_lossy(b);
            let last = text.contains("finish_reason\":\"") || text.contains("[DONE]");
            if (r.cut || r.hang) && last {
                break;
            }
            let chunk = [format!("{:x}\r\n", b.len()).as_bytes(), b, b"\r\n"].concat();
            c.write_all(&chunk).await.ok()?;
            c.flush().await.ok()?;
        }
        if r.hang {
            let mut b = [0; 16];
            let _ = c.read(&mut b).await;
            *self.closed.lock().unwrap() += 1;
            return None;
        }
        if !r.cut {
            c.write_all(b"0\r\n\r\n").await.ok()?;
        }
        Some(())
    }
    pub fn add(&self, r: Reply) {
        self.script.lock().unwrap().push_back(r);
    }
    /// What was still to be answered is not.
    pub fn clear(&self) {
        self.script.lock().unwrap().clear();
    }
    pub fn requests(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }
    /// The credentials it was shown.
    pub fn keys(&self) -> Vec<String> {
        self.keys.lock().unwrap().clone()
    }
    pub async fn closed(&self) {
        let now = Instant::now();
        while *self.closed.lock().unwrap() == 0 {
            assert!(now.elapsed() < Duration::from_secs(20));
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }
}
