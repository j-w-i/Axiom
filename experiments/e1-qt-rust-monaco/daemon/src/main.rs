//! E1 experiment daemon: JSON-RPC 2.0, newline-delimited JSON over stdio.
//! Deliberately std-only; the production daemon uses tokio (see ADR-0003).
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

type Out = Arc<Mutex<std::io::Stdout>>;

fn send(out: &Out, v: Value) {
    let mut o = out.lock().unwrap();
    let _ = writeln!(o, "{}", v);
    let _ = o.flush();
}

fn err(id: Value, code: i64, msg: String, kind: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":msg,"data":{"kind":kind}}})
}

fn pump<R: Read + Send + 'static>(r: R, stream: &'static str, run_id: String, out: Out) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for line in BufReader::new(r).lines().map_while(Result::ok) {
            send(&out, json!({"jsonrpc":"2.0","method":"process/output","params":{"runId":run_id,"stream":stream,"text":line}}));
        }
    })
}

fn main() {
    let out: Out = Arc::new(Mutex::new(std::io::stdout()));
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => { send(&out, err(Value::Null, -32700, e.to_string(), "parse_error")); continue; }
        };
        let id = msg.get("id").cloned().unwrap_or(Value::Null);
        let p = msg.get("params").cloned().unwrap_or(json!({}));
        match msg.get("method").and_then(Value::as_str).unwrap_or("") {
            "initialize" => send(&out, json!({"jsonrpc":"2.0","id":id,"result":{"protocolVersion":"0.1","server":"e1-daemon","pid":std::process::id()}})),
            "fs/readFile" => {
                let path = p["path"].as_str().unwrap_or("");
                match std::fs::read_to_string(path) {
                    Ok(text) => send(&out, json!({"jsonrpc":"2.0","id":id,"result":{"text":text}})),
                    Err(e) => send(&out, err(id, -32001, e.to_string(), "io_error")),
                }
            }
            "fs/writeFile" => {
                let (path, text) = (p["path"].as_str().unwrap_or(""), p["text"].as_str().unwrap_or(""));
                match std::fs::write(path, text) {
                    Ok(()) => send(&out, json!({"jsonrpc":"2.0","id":id,"result":{"bytes":text.len()}})),
                    Err(e) => send(&out, err(id, -32001, e.to_string(), "io_error")),
                }
            }
            "process/run" => {
                let argv: Vec<String> = p["argv"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
                let run_id = p["runId"].as_str().unwrap_or("run").to_string();
                let out2 = out.clone();
                // Runs off the request loop so further requests are served while the process runs.
                thread::spawn(move || {
                    if argv.is_empty() { send(&out2, err(id, -32602, "argv is empty".into(), "invalid_params")); return; }
                    match Command::new(&argv[0]).args(&argv[1..]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
                        Err(e) => send(&out2, err(id, -32002, format!("{}: {}", argv[0], e), "spawn_failed")),
                        Ok(mut child) => {
                            let a = pump(child.stdout.take().unwrap(), "stdout", run_id.clone(), out2.clone());
                            let b = pump(child.stderr.take().unwrap(), "stderr", run_id.clone(), out2.clone());
                            let status = child.wait();
                            let _ = a.join(); let _ = b.join();
                            let code = status.ok().and_then(|s| s.code());
                            send(&out2, json!({"jsonrpc":"2.0","id":id,"result":{"runId":run_id,"exitCode":code}}));
                        }
                    }
                });
            }
            "shutdown" => { send(&out, json!({"jsonrpc":"2.0","id":id,"result":null})); break; }
            other => send(&out, err(id, -32601, format!("method not found: {other}"), "method_not_found")),
        }
    }
}
