//! A fake language server for tests (LSP base protocol on stdio, full document sync).
//! - `initialize`: completion, definition and hover; full sync.
//! - `didOpen` / `didChange`: publishes one error diagnostic per line containing `ERROR`.
//! - `textDocument/completion`: `alpha`, `beta`.
//! - `textDocument/definition`: line 0 of `$FAKE_LSP_DEFINITION` (a URI) or of the same file.
//! - `textDocument/hover` on a document containing `SLOW`: answers only once `$/cancelRequest`
//!   arrives (with RequestCancelled), else hover text with the document's version.
//! - `fake/config`: asks the client `workspace/configuration` for section `fake.answer` and
//!   returns what it got.
//! - `fake/crash`: exits with status 3 at once.
//! - `fake/echo`: returns its params.
//!
//! `FAKE_LSP_LOG` (optional): every received method is appended there, one per line.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, Write};

fn read(r: &mut impl BufRead) -> Option<Value> {
    let mut len = None;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let l = line.trim_end();
        if l.is_empty() {
            break;
        }
        if let Some((k, v)) = l.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                len = v.trim().parse::<usize>().ok();
            }
        }
    }
    let mut body = vec![0; len?];
    r.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

fn write(v: &Value) {
    let body = v.to_string();
    let mut out = std::io::stdout().lock();
    let _ = write!(out, "Content-Length: {}\r\n\r\n{body}", body.len());
    let _ = out.flush();
}

fn log(method: &str) {
    if let Ok(p) = std::env::var("FAKE_LSP_LOG") {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
        {
            let _ = writeln!(f, "{method}");
        }
    }
}

fn publish(uri: &str, text: &str, version: Option<i64>) {
    let diags: Vec<Value> = text
        .lines()
        .enumerate()
        .filter(|(_, l)| l.contains("ERROR"))
        .map(|(i, l)| {
            let c = l.find("ERROR").unwrap_or(0);
            json!({"range":{"start":{"line":i,"character":c},"end":{"line":i,"character":c+5}},
                   "severity":1,"source":"fake","message":"an ERROR here"})
        })
        .collect();
    write(
        &json!({"jsonrpc":"2.0","method":"textDocument/publishDiagnostics",
        "params":{"uri":uri,"version":version,"diagnostics":diags}}),
    );
}

fn main() {
    let stdin = std::io::stdin();
    let mut r = stdin.lock();
    let mut docs: HashMap<String, (String, i64)> = HashMap::new();
    let mut next = 1000;
    while let Some(m) = read(&mut r) {
        let method = m["method"].as_str().unwrap_or_default().to_string();
        let id = m.get("id").cloned();
        if !method.is_empty() {
            log(&method);
        }
        let p = &m["params"];
        let reply = |result: Value| {
            if let Some(id) = &id {
                write(&json!({"jsonrpc":"2.0","id":id,"result":result}));
            }
        };
        match method.as_str() {
            "initialize" => reply(json!({"capabilities":{
                "textDocumentSync":1,"completionProvider":{},"definitionProvider":true,"hoverProvider":true},
                "serverInfo":{"name":"fake-lsp","version":"0"}})),
            "textDocument/didOpen" => {
                let td = &p["textDocument"];
                let uri = td["uri"].as_str().unwrap_or_default().to_string();
                let text = td["text"].as_str().unwrap_or_default().to_string();
                let v = td["version"].as_i64().unwrap_or(0);
                publish(&uri, &text, Some(v));
                docs.insert(uri, (text, v));
            }
            "textDocument/didChange" => {
                let uri = p["textDocument"]["uri"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let v = p["textDocument"]["version"].as_i64().unwrap_or(0);
                if let Some(t) = p["contentChanges"]
                    .as_array()
                    .and_then(|a| a.last())
                    .and_then(|c| c["text"].as_str())
                {
                    publish(&uri, t, Some(v));
                    docs.insert(uri, (t.to_string(), v));
                }
            }
            "textDocument/didClose" => {
                if let Some(uri) = p["textDocument"]["uri"].as_str() {
                    docs.remove(uri);
                }
            }
            "textDocument/completion" => reply(json!([{"label":"alpha"},{"label":"beta"}])),
            "textDocument/definition" => {
                let uri = std::env::var("FAKE_LSP_DEFINITION").unwrap_or_else(|_| {
                    p["textDocument"]["uri"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string()
                });
                reply(
                    json!([{"uri":uri,"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}}}]),
                );
            }
            "textDocument/hover" => {
                let uri = p["textDocument"]["uri"].as_str().unwrap_or_default();
                let (text, v) = docs.get(uri).cloned().unwrap_or_default();
                if text.contains("SLOW") {
                    // Wait for the cancellation of this request.
                    while let Some(c) = read(&mut r) {
                        let cm = c["method"].as_str().unwrap_or_default();
                        log(cm);
                        if cm == "$/cancelRequest"
                            && c["params"]["id"] == id.clone().unwrap_or(Value::Null)
                        {
                            break;
                        }
                    }
                    if let Some(id) = &id {
                        write(
                            &json!({"jsonrpc":"2.0","id":id,"error":{"code":-32800,"message":"cancelled"}}),
                        );
                    }
                } else {
                    reply(json!({"contents":{"kind":"markdown","value":format!("version {v}")}}));
                }
            }
            "fake/config" => {
                next += 1;
                write(
                    &json!({"jsonrpc":"2.0","id":next,"method":"workspace/configuration",
                    "params":{"items":[{"section":"fake.answer"},{}]}}),
                );
                let mut got = Value::Null;
                while let Some(c) = read(&mut r) {
                    if c["id"] == next && c.get("method").is_none() {
                        got = c["result"].clone();
                        break;
                    }
                }
                reply(got);
            }
            "fake/apply" => {
                next += 1;
                write(
                    &json!({"jsonrpc":"2.0","id":next,"method":"workspace/applyEdit","params":{"edit":{"changes":{}}}}),
                );
                let mut got = Value::Null;
                while let Some(c) = read(&mut r) {
                    if c["id"] == next && c.get("method").is_none() {
                        got = c["result"].clone();
                        break;
                    }
                }
                reply(got);
            }
            "fake/crash" => std::process::exit(3),
            "fake/echo" => reply(p.clone()),
            "fake/fail" => {
                if let Some(id) = &id {
                    write(
                        &json!({"jsonrpc":"2.0","id":id,"error":{"code":-32001,"message":"it failed","data":{"why":"asked"}}}),
                    );
                }
            }
            "shutdown" => reply(Value::Null),
            "exit" => std::process::exit(0),
            _ => {
                if id.is_some() {
                    write(
                        &json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"unknown method"}}),
                    );
                }
            }
        }
    }
}
