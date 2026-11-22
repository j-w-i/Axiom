//! A fake language server for tests (LSP base protocol on stdio, full document sync).
//! - `initialize`: completion, definition and hover; full sync.
//! - `didOpen` / `didChange`: publishes one error diagnostic per line containing `ERROR`.
//! - `textDocument/completion`: `alpha`, `beta`, a `printLine` snippet (documented on
//!   `completionItem/resolve`) and `HashMap` (with an import as an additional edit).
//! - `textDocument/definition`: line 0 of `$FAKE_LSP_DEFINITION` (a URI) or of the same file.
//! - `textDocument/hover` on a document containing `SLOW`: answers only once `$/cancelRequest`
//!   arrives (with RequestCancelled), else hover text with the document's version.
//! - `fake/config`: asks the client `workspace/configuration` for section `fake.answer` and
//!   returns what it got.
//! - `fake/crash`: exits with status 3 at once.
//! - `fake/echo`: returns its params.
//! - `textDocument/rename`: every whole-word occurrence of the word at the position, in every
//!   open document, becomes the new name.
//! - `textDocument/formatting`: removes trailing spaces.
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
                "textDocumentSync":1,"completionProvider":{"triggerCharacters":["."],"resolveProvider":true},"definitionProvider":true,"hoverProvider":true,
                "renameProvider":true,"documentFormattingProvider":true,
                "referencesProvider":true,"documentSymbolProvider":true,"documentHighlightProvider":true,
                "codeActionProvider":true,"signatureHelpProvider":{"triggerCharacters":["(",","]}},
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
            "textDocument/completion" => reply(json!([
                {"label":"alpha"},{"label":"beta"},
                {"label":"printLine","kind":3,"insertTextFormat":2,
                 "insertText":"printLine(${1:message}, ${2:level})$0"},
                {"label":"HashMap","kind":7,"insertText":"HashMap",
                 "additionalTextEdits":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},
                   "newText":"use std::collections::HashMap;\n"}]}
            ])),
            // M5.5-EDIT-001: documentation only on resolve.
            "completionItem/resolve" => {
                let mut item = p.clone();
                if item["label"] == "printLine" {
                    item["detail"] = json!("fn printLine(message, level)");
                    item["documentation"] = json!({"kind":"markdown","value":"Prints a **line** at a level."});
                }
                reply(item)
            }
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
            "textDocument/rename" => {
                let uri = p["textDocument"]["uri"].as_str().unwrap_or_default();
                let (line, ch) = (
                    p["position"]["line"].as_u64().unwrap_or(0) as usize,
                    p["position"]["character"].as_u64().unwrap_or(0) as usize,
                );
                let new = p["newName"].as_str().unwrap_or_default();
                let word = docs
                    .get(uri)
                    .and_then(|(t, _)| word_at(t, line, ch))
                    .unwrap_or_default();
                let mut changes = serde_json::Map::new();
                if !word.is_empty() {
                    for (u, (t, _)) in &docs {
                        let edits: Vec<Value> = occurrences(t, &word)
                            .into_iter()
                            .map(|(l, c)| json!({"range":{"start":{"line":l,"character":c},"end":{"line":l,"character":c + word.len()}},"newText":new}))
                            .collect();
                        if !edits.is_empty() {
                            changes.insert(u.clone(), Value::Array(edits));
                        }
                    }
                }
                reply(json!({"changes": changes}));
            }
            // Every occurrence of the word at the position, in every open document.
            "textDocument/references" | "textDocument/documentHighlight" => {
                let uri = p["textDocument"]["uri"].as_str().unwrap_or_default();
                let (line, ch) = (
                    p["position"]["line"].as_u64().unwrap_or(0) as usize,
                    p["position"]["character"].as_u64().unwrap_or(0) as usize,
                );
                let word = docs
                    .get(uri)
                    .and_then(|(t, _)| word_at(t, line, ch))
                    .unwrap_or_default();
                let range = |l: usize, c: usize| json!({"start":{"line":l,"character":c},"end":{"line":l,"character":c + word.len()}});
                let mut out = Vec::new();
                if !word.is_empty() {
                    if method == "textDocument/references" {
                        let mut uris: Vec<&String> = docs.keys().collect();
                        uris.sort();
                        for u in uris {
                            for (l, c) in occurrences(&docs[u].0, &word) {
                                out.push(json!({"uri":u,"range":range(l, c)}));
                            }
                        }
                    } else if let Some((t, _)) = docs.get(uri) {
                        for (l, c) in occurrences(t, &word) {
                            out.push(json!({"range":range(l, c),"kind":1}));
                        }
                    }
                }
                reply(Value::Array(out));
            }
            // `fn name` lines as functions, `let name` inside them as their children.
            "textDocument/documentSymbol" => {
                let uri = p["textDocument"]["uri"].as_str().unwrap_or_default();
                let text = docs.get(uri).map(|(t, _)| t.clone()).unwrap_or_default();
                let mut out: Vec<Value> = Vec::new();
                for (i, l) in text.lines().enumerate() {
                    let t = l.trim_start();
                    let indent = l.len() - t.len();
                    for (kw, kind) in [("fn ", 12), ("let ", 13)] {
                        if let Some(rest) = t.strip_prefix(kw) {
                            let name: String = rest
                                .chars()
                                .take_while(|c| c.is_alphanumeric() || *c == '_')
                                .collect();
                            if name.is_empty() {
                                continue;
                            }
                            let c = indent + kw.len();
                            let r = json!({"start":{"line":i,"character":c},"end":{"line":i,"character":c + name.len()}});
                            let sym = json!({"name":name,"kind":kind,"range":r,"selectionRange":r,"children":[]});
                            if kind == 13 {
                                if let Some(f) = out.last_mut() {
                                    f["children"].as_array_mut().unwrap().push(sym);
                                    continue;
                                }
                            }
                            out.push(sym);
                        }
                    }
                }
                reply(Value::Array(out));
            }
            // One quick fix: a line without a trailing `;` gets one.
            "textDocument/codeAction" => {
                let uri = p["textDocument"]["uri"].as_str().unwrap_or_default();
                let line = p["range"]["start"]["line"].as_u64().unwrap_or(0) as usize;
                let text = docs.get(uri).map(|(t, _)| t.clone()).unwrap_or_default();
                let l = text.lines().nth(line).unwrap_or_default();
                let t = l.trim_end();
                if t.is_empty() || t.ends_with(';') || t.ends_with('{') || t.ends_with('}') {
                    reply(json!([]));
                } else {
                    reply(
                        json!([{"title":"Add a semicolon","kind":"quickfix","isPreferred":true,
                        "edit":{"changes":{uri:[{"range":{"start":{"line":line,"character":t.len()},"end":{"line":line,"character":t.len()}},"newText":";"}]}}}]),
                    );
                }
            }
            // `value(a: i32, b: i32)`, the active parameter counted from the commas before the
            // position on its line.
            "textDocument/signatureHelp" => {
                let uri = p["textDocument"]["uri"].as_str().unwrap_or_default();
                let (line, ch) = (
                    p["position"]["line"].as_u64().unwrap_or(0) as usize,
                    p["position"]["character"].as_u64().unwrap_or(0) as usize,
                );
                let text = docs.get(uri).map(|(t, _)| t.clone()).unwrap_or_default();
                let before: String = text
                    .lines()
                    .nth(line)
                    .unwrap_or_default()
                    .chars()
                    .take(ch)
                    .collect();
                let open = before.rfind('(');
                match open {
                    None => reply(Value::Null),
                    Some(o) => {
                        let active = before[o..].matches(',').count();
                        reply(
                            json!({"signatures":[{"label":"value(a: i32, b: i32) -> i32",
                            "parameters":[{"label":[6,12]},{"label":[14,20]}]}],
                            "activeSignature":0,"activeParameter":active}),
                        );
                    }
                }
            }
            "textDocument/formatting" => {
                let uri = p["textDocument"]["uri"].as_str().unwrap_or_default();
                let text = docs.get(uri).map(|(t, _)| t.clone()).unwrap_or_default();
                let edits: Vec<Value> = text
                    .lines()
                    .enumerate()
                    .filter_map(|(i, l)| {
                        let t = l.trim_end_matches(' ');
                        (t.len() < l.len()).then(|| json!({"range":{"start":{"line":i,"character":t.len()},"end":{"line":i,"character":l.len()}},"newText":""}))
                    })
                    .collect();
                reply(Value::Array(edits));
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

/// The identifier at a 0-based line and (ASCII) character.
fn word_at(text: &str, line: usize, ch: usize) -> Option<String> {
    let l = text.lines().nth(line)?;
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let bytes: Vec<char> = l.chars().collect();
    if ch > bytes.len() {
        return None;
    }
    let mut a = ch;
    while a > 0 && is_word(bytes[a - 1]) {
        a -= 1;
    }
    let mut b = ch;
    while b < bytes.len() && is_word(bytes[b]) {
        b += 1;
    }
    (a < b).then(|| bytes[a..b].iter().collect())
}

/// (line, character) of every whole-word occurrence (ASCII text).
fn occurrences(text: &str, word: &str) -> Vec<(usize, usize)> {
    let is_word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    let mut out = Vec::new();
    for (i, l) in text.lines().enumerate() {
        let mut from = 0;
        while let Some(k) = l[from..].find(word) {
            let at = from + k;
            let before = l[..at].chars().last();
            let after = l[at + word.len()..].chars().next();
            if !is_word(before) && !is_word(after) {
                out.push((i, at));
            }
            from = at + word.len();
        }
    }
    out
}
