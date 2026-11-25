//! What an installed program is given of Axiom: the `agent` tool, or all the tools an agent of Axiom's
//! has, as a Model Context Protocol server on its standard streams. It is this program, started by that
//! one; it holds a key that opens these for one agent, and only while its attempt lives. Whatever is asked of it is judged by the
//! host, as a call of Axiom's own agents is.

use std::{collections::HashMap, path::Path, sync::Arc};

use axiom_proto::Call;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    sync::{Mutex, Semaphore},
    task::AbortHandle,
};

use crate::{
    client::{Client, ClientError},
    paths::Paths,
};

pub const ARG: &str = "--mcp";
/// One line, at most.
const LINE: u64 = 1024 * 1024;
/// How often who waits is told that it goes on.
const STILL: std::time::Duration = std::time::Duration::from_secs(60);
/// Calls at once.
const CALLS: usize = 4;

/// Where the host is, and the key. In a file only its owner reads: never an argument, never an
/// environment.
#[derive(Debug, Serialize, Deserialize)]
pub struct Way {
    pub data: std::path::PathBuf,
    pub grant: String,
}

/// Serves until its input ends.
pub async fn serve(file: &Path) -> std::io::Result<()> {
    let text = tokio::fs::read_to_string(file).await?;
    let way: Way = serde_json::from_str(&text).map_err(std::io::Error::other)?;
    speak(way, tokio::io::stdin(), tokio::io::stdout()).await
}

type Calls = Arc<std::sync::Mutex<HashMap<String, AbortHandle>>>;

async fn speak(
    way: Way,
    input: impl AsyncRead + Unpin,
    output: impl AsyncWrite + Unpin + Send + 'static,
) -> std::io::Result<()> {
    let way = Arc::new(way);
    let out = Arc::new(Mutex::new(output));
    let places = Arc::new(Semaphore::new(CALLS));
    let calls: Calls = Arc::default();
    let mut lines = BufReader::new(input);
    let mut tasks = tokio::task::JoinSet::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        let n = (&mut lines).take(LINE + 1).read_until(b'\n', &mut line).await?;
        // Its end, or a line longer than any of its calls is.
        if n == 0 || (n as u64 > LINE && !line.ends_with(b"\n")) {
            return Ok(());
        }
        while tasks.try_join_next().is_some() {}
        let Ok(v) = serde_json::from_slice::<Value>(&line) else {
            if !line.trim_ascii().is_empty() {
                write(&out, error(&Value::Null, -32700, "not JSON")).await?;
            }
            continue;
        };
        let method = v["method"].as_str().unwrap_or_default().to_owned();
        let id = v.get("id").cloned();
        match (method.as_str(), id) {
            ("notifications/cancelled", _) => {
                // Who asked waits no longer, and the host stops too.
                let key = v["params"]["requestId"].to_string();
                if let Some(h) = calls.lock().ok().and_then(|mut c| c.remove(&key)) {
                    h.abort();
                }
            }
            (_, None) => {}
            ("tools/call", Some(id)) => {
                let Ok(place) = places.clone().try_acquire_owned() else {
                    write(&out, error(&id, -32000, "too much is asked at once")).await?;
                    continue;
                };
                let (way, out, done, key) =
                    (way.clone(), out.clone(), calls.clone(), id.to_string());
                let (name, args) = (
                    v["params"]["name"].as_str().unwrap_or_default().to_owned(),
                    v["params"]["arguments"].clone(),
                );
                let token = v["params"]["_meta"]["progressToken"].clone();
                let begun = Arc::new(tokio::sync::Notify::new());
                let go = begun.clone();
                let task = tasks.spawn(async move {
                    go.notified().await;
                    let asked = call(&way, &name, args);
                    tokio::pin!(asked);
                    // A person may take long to answer: whoever waits is told that it goes on.
                    let mut n = 0u64;
                    let (text, ok) = loop {
                        tokio::select! {
                            r = &mut asked => break r,
                            () = tokio::time::sleep(STILL) => {
                                n += 1;
                                if !token.is_null() {
                                    let p = json!({"progressToken": token, "progress": n});
                                    let m = json!({"jsonrpc": "2.0", "method": "notifications/progress", "params": p});
                                    let _ = write(&out, m).await;
                                }
                            }
                        }
                    };
                    if let Ok(mut c) = done.lock() {
                        c.remove(&id.to_string());
                    }
                    let content = json!([{"type": "text", "text": text}]);
                    let result = json!({"content": content, "isError": !ok});
                    let _ =
                        write(&out, json!({"jsonrpc": "2.0", "id": id, "result": result})).await;
                    drop(place);
                });
                if let Ok(mut c) = calls.lock() {
                    c.insert(key, task);
                }
                begun.notify_one();
            }
            (m, Some(id)) => {
                let answer = match m {
                    "initialize" => Some(json!({
                        // What the other asks for: this server does nothing that versions differ in.
                        "protocolVersion": v["params"]["protocolVersion"]
                            .as_str()
                            .filter(|p| p.len() <= 32)
                            .unwrap_or("2025-06-18"),
                        "capabilities": {"tools": {}},
                        "serverInfo": {"name": "axiom", "version": env!("CARGO_PKG_VERSION")},
                    })),
                    "ping" => Some(json!({})),
                    "tools/list" => match listed(&way).await {
                        Ok(all) => Some(json!({ "tools": all })),
                        Err(e) => {
                            write(&out, error(&id, -32000, &e)).await?;
                            continue;
                        }
                    },
                    _ => None,
                };
                let v = match answer {
                    Some(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                    None => error(&id, -32601, "there is no such method"),
                };
                write(&out, v).await?;
            }
        }
    }
}

fn error(id: &Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

async fn write(out: &Mutex<impl AsyncWrite + Unpin>, v: Value) -> std::io::Result<()> {
    let mut out = out.lock().await;
    out.write_all(format!("{v}\n").as_bytes()).await?;
    out.flush().await
}

async fn hello(way: &Way) -> Result<(Client, Value), ClientError> {
    let mut c = Client::raw(&Paths::new(way.data.clone())).await?;
    let v = c.call(Call::PeerHello { grant: way.grant.clone() }).await?;
    Ok((c, v))
}

/// What the key opens. Their texts may be long: they are not to be put into a file instead, which its
/// model could not read without tools of its own.
async fn listed(way: &Way) -> Result<Vec<Value>, String> {
    let (_, v) = hello(way).await.map_err(|e| e.to_string())?;
    let all = v["tools"].as_array().cloned().unwrap_or_default();
    let shown = |t: &Value| {
        json!({"name": t["name"], "description": t["description"], "inputSchema": t["schema"],
            "_meta": {"anthropic/maxResultSizeChars": 200_000}})
    };
    Ok(all.iter().map(shown).collect())
}

/// One call, on a connection of its own: closed, the host stops what it does for it.
async fn call(way: &Way, name: &str, args: Value) -> (String, bool) {
    let asked = async {
        let (mut c, _) = hello(way).await?;
        c.call(Call::Peer { tool: name.to_owned(), args }).await
    };
    match asked.await {
        Ok(v) => (
            v["text"].as_str().unwrap_or_default().to_owned(),
            v["ok"].as_bool().unwrap_or_default(),
        ),
        Err(e) => (format!("Not done: {e}."), false),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    async fn said(input: &str) -> Vec<Value> {
        let way = Way { data: "/nonexistent".into(), grant: "x".into() };
        let (out, mut back) = tokio::io::duplex(1024 * 1024);
        let served = speak(way, input.as_bytes(), out);
        let mut all = String::new();
        let (r, n) = tokio::join!(served, back.read_to_string(&mut all));
        r.unwrap();
        n.unwrap();
        all.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    #[tokio::test]
    async fn it_says_what_it_is_and_nothing_to_what_it_is_only_told() {
        let all = said(concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}"#, "\n",
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#, "\n\n",
            r#"{"jsonrpc":"2.0","id":"two","method":"tools/list"}"#, "\n",
            r#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#, "\n",
            r#"{"jsonrpc":"2.0","id":4,"method":"resources/list"}"#, "\n",
            "nonsense\n",
        ))
        .await;
        assert_eq!(all.len(), 5, "{all:#?}");
        assert_eq!(all[0]["result"]["protocolVersion"], "2025-11-25");
        // What there is, the host says; there is none here.
        assert_eq!(
            (all[1]["id"].as_str(), all[1]["error"]["code"].as_i64()),
            (Some("two"), Some(-32000))
        );
        assert_eq!(all[2]["result"], json!({}));
        assert_eq!(all[3]["error"]["code"], -32601);
        assert_eq!(all[4]["error"]["code"], -32700);
    }

    #[tokio::test]
    async fn a_host_that_is_not_there_is_said_and_not_the_end() {
        let way = Way { data: "/nonexistent".into(), grant: "x".into() };
        let (words, ok) = super::call(&way, "agent", json!({})).await;
        assert!(!ok && words.starts_with("Not done"), "{words}");
    }

    #[tokio::test]
    async fn a_line_without_an_end_is_the_end() {
        let all = said(&"x".repeat(LINE as usize + 10)).await;
        assert!(all.is_empty());
    }
}
