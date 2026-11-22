//! A fake ACP agent for tests. Newline-delimited JSON-RPC on stdio. The first word of the
//! prompt picks the scenario:
//! - `read`: reads `README.md` through `fs/read_text_file` and quotes its first line.
//! - `write`: announces an edit, asks permission, writes `notes.md` through `fs/write_text_file`.
//! - `run`: runs `sh -c 'echo term-out'` through `terminal/*`.
//! - `sneak`: writes `sneaky.txt` without asking permission first.
//! - `outside`: reads `/etc/hostname` (outside the workspace).
//! - `wait`: asks permission and waits (for cancel tests).
//! - `peek`: asks a `read` permission for `notes.md`, then writes it (AUD-040: a read approval
//!   must not cover a write).
//! - `envrun`: runs `sh -c 'echo term-out'` with `LD_PRELOAD` set (AUD-039).
//! - `crash`: exits at once, mid-turn (AUD-032: the next turn starts the agent again).
//!
//! `FAKE_ACP_LOG` (optional): `session/new` params are appended there as JSON lines.

use serde_json::{json, Value};
use std::io::{BufRead, Write};

struct Peer {
    lines: std::io::Lines<std::io::StdinLock<'static>>,
    next: i64,
    cancelled: bool,
}

fn send(v: Value) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

impl Peer {
    /// A request to the client; reads until its response (noting a `session/cancel`).
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next += 1;
        let id = self.next;
        send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        loop {
            let Some(Ok(l)) = self.lines.next() else {
                std::process::exit(0)
            };
            let Ok(v) = serde_json::from_str::<Value>(&l) else {
                continue;
            };
            if v["method"] == "session/cancel" {
                self.cancelled = true;
                continue;
            }
            if v["id"] == id && v.get("method").is_none() {
                return match v.get("error") {
                    Some(e) => Err(e["message"].as_str().unwrap_or("error").to_string()),
                    None => Ok(v["result"].clone()),
                };
            }
        }
    }
}

fn say(sid: &str, text: &str) {
    send(
        json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":sid,
        "update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":text}}}}),
    );
}

fn update(sid: &str, u: Value) {
    send(json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":sid,"update":u}}));
}

fn main() {
    let stdin: &'static std::io::Stdin = Box::leak(Box::new(std::io::stdin()));
    let mut peer = Peer {
        lines: stdin.lock().lines(),
        next: 0,
        cancelled: false,
    };
    let mut cwd = String::new();
    loop {
        let Some(Ok(line)) = peer.lines.next() else {
            return;
        };
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(id) = msg.get("id").cloned() else {
            continue;
        };
        let p = msg["params"].clone();
        match msg["method"].as_str().unwrap_or("") {
            "initialize" => send(json!({"jsonrpc":"2.0","id":id,"result":{
                "protocolVersion":1,
                "agentCapabilities":{"loadSession":false,"promptCapabilities":{"image":false,"embeddedContext":false}},
                "authMethods":[]}})),
            "session/new" => {
                cwd = p["cwd"].as_str().unwrap_or("").to_string();
                if let Ok(log) = std::env::var("FAKE_ACP_LOG") {
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(log)
                    {
                        let _ = writeln!(f, "{p}");
                    }
                }
                send(json!({"jsonrpc":"2.0","id":id,"result":{"sessionId":"fa-1"}}));
            }
            "session/prompt" => {
                let sid = p["sessionId"].as_str().unwrap_or("").to_string();
                let text = p["prompt"][0]["text"].as_str().unwrap_or("").to_string();
                peer.cancelled = false;
                let stop = scenario(
                    &mut peer,
                    &sid,
                    &cwd,
                    text.split_whitespace().next().unwrap_or(""),
                );
                send(json!({"jsonrpc":"2.0","id":id,"result":{"stopReason":stop}}));
            }
            _ => send(json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"no"}})),
        }
    }
}

fn scenario(peer: &mut Peer, sid: &str, cwd: &str, word: &str) -> &'static str {
    match word {
        "read" => match peer.call(
            "fs/read_text_file",
            json!({"sessionId":sid,"path":format!("{cwd}/README.md"),"line":1,"limit":1}),
        ) {
            Ok(r) => say(
                sid,
                &format!("README: {}", r["content"].as_str().unwrap_or("").trim()),
            ),
            Err(e) => say(sid, &format!("read failed: {e}")),
        },
        "write" | "wait" => {
            let path = format!("{cwd}/notes.md");
            let call = json!({"toolCallId":"t1","title":"Write notes.md","kind":"edit","status":"pending",
                "locations":[{"path":path}],
                "content":[{"type":"diff","path":path,"oldText":null,"newText":"hello\n"}]});
            let mut tc = call.clone();
            tc["sessionUpdate"] = json!("tool_call");
            update(sid, tc);
            let r = peer.call(
                "session/request_permission",
                json!({"sessionId":sid,"toolCall":call,
                "options":[{"optionId":"allow","name":"Allow","kind":"allow_once"},
                           {"optionId":"reject","name":"Reject","kind":"reject_once"}]}),
            );
            let outcome = r
                .as_ref()
                .map(|v| v["outcome"].clone())
                .unwrap_or(Value::Null);
            if peer.cancelled || outcome["outcome"] == "cancelled" {
                update(
                    sid,
                    json!({"sessionUpdate":"tool_call_update","toolCallId":"t1","status":"failed"}),
                );
                return "cancelled";
            }
            if outcome["optionId"] != "allow" {
                update(
                    sid,
                    json!({"sessionUpdate":"tool_call_update","toolCallId":"t1","status":"failed"}),
                );
                say(sid, "not allowed");
                return "end_turn";
            }
            match peer.call(
                "fs/write_text_file",
                json!({"sessionId":sid,"path":path,"content":"hello\n"}),
            ) {
                Ok(_) => {
                    update(
                        sid,
                        json!({"sessionUpdate":"tool_call_update","toolCallId":"t1","status":"completed"}),
                    );
                    say(sid, "wrote notes.md");
                }
                Err(e) => {
                    update(
                        sid,
                        json!({"sessionUpdate":"tool_call_update","toolCallId":"t1","status":"failed"}),
                    );
                    say(sid, &format!("write failed: {e}"));
                }
            }
        }
        "crash" => std::process::exit(3),
        "peek" => {
            let path = format!("{cwd}/notes.md");
            let call = json!({"toolCallId":"t2","title":"Read notes.md","kind":"read",
                "locations":[{"path":path}]});
            let _ = peer.call(
                "session/request_permission",
                json!({"sessionId":sid,"toolCall":call,
                "options":[{"optionId":"allow","name":"Allow","kind":"allow_once"},
                           {"optionId":"reject","name":"Reject","kind":"reject_once"}]}),
            );
            match peer.call(
                "fs/write_text_file",
                json!({"sessionId":sid,"path":path,"content":"peeked\n"}),
            ) {
                Ok(_) => say(sid, "wrote notes.md"),
                Err(e) => say(sid, &format!("write refused: {e}")),
            }
        }
        "run" | "envrun" => {
            let mut create = json!({"sessionId":sid,"command":"sh","args":["-c","echo term-out"],"outputByteLimit":4096});
            if word == "envrun" {
                create["env"] = json!([{"name":"LD_PRELOAD","value":"/tmp/evil.so"}]);
            }
            let r = peer.call("terminal/create", create);
            let Ok(r) = r else {
                say(
                    sid,
                    &format!("terminal refused: {}", r.err().unwrap_or_default()),
                );
                return "end_turn";
            };
            let tid = r["terminalId"].as_str().unwrap_or("").to_string();
            let exit = peer
                .call(
                    "terminal/wait_for_exit",
                    json!({"sessionId":sid,"terminalId":tid}),
                )
                .unwrap_or_default();
            let out = peer
                .call("terminal/output", json!({"sessionId":sid,"terminalId":tid}))
                .unwrap_or_default();
            let _ = peer.call(
                "terminal/release",
                json!({"sessionId":sid,"terminalId":tid}),
            );
            say(
                sid,
                &format!(
                    "ran: {} exit {}",
                    out["output"].as_str().unwrap_or("").trim(),
                    exit["exitCode"]
                ),
            );
        }
        "sneak" => match peer.call(
            "fs/write_text_file",
            json!({"sessionId":sid,"path":format!("{cwd}/sneaky.txt"),"content":"x"}),
        ) {
            Ok(_) => say(sid, "sneak worked"),
            Err(e) => say(sid, &format!("sneak refused: {e}")),
        },
        "outside" => match peer.call(
            "fs/read_text_file",
            json!({"sessionId":sid,"path":"/etc/hostname"}),
        ) {
            Ok(_) => say(sid, "outside worked"),
            Err(e) => say(sid, &format!("outside refused: {e}")),
        },
        _ => {
            send(
                json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":sid,
                "update":{"sessionUpdate":"plan","entries":[{"content":"say hello","priority":"high","status":"completed"}]}}}),
            );
            say(sid, "hello from the fake agent");
        }
    }
    "end_turn"
}
