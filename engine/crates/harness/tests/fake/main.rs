//! An agent that plays a script, for the tests. It is started under another name in a folder of its
//! own, and finds there what to do: `script.jsonl`, a step on each line. What it was started with goes
//! to `seen.json`, what it was told to `heard.jsonl`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::print_stderr)]

use std::{
    collections::BTreeMap,
    io::{BufRead, Write},
    time::Duration,
};

use serde_json::{Value, json};

fn main() {
    let dir = std::env::current_exe().unwrap().parent().unwrap().to_owned();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env: BTreeMap<String, String> = std::env::vars().collect();
    let cwd = std::env::current_dir().unwrap();
    let seen = json!({"args": args, "env": env, "cwd": cwd.to_str().unwrap()});
    std::fs::write(dir.join("seen.json"), seen.to_string()).unwrap();
    if args == ["--version"] {
        let v = std::fs::read(dir.join("version")).unwrap_or_default();
        std::io::stdout().write_all(&v).unwrap();
        std::io::stdout().flush().unwrap();
        if v.is_empty() {
            std::process::exit(3);
        }
        return;
    }
    let script = std::fs::read_to_string(dir.join("script.jsonl")).unwrap();
    let mut heard = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("heard.jsonl"))
        .unwrap();
    let mut known: BTreeMap<String, Value> = BTreeMap::new();
    let mut stdin = std::io::stdin().lock();
    let mut hear = move || -> Option<Value> {
        let mut l = String::new();
        if stdin.read_line(&mut l).unwrap_or(0) == 0 {
            return None;
        }
        heard.write_all(l.as_bytes()).unwrap();
        Some(serde_json::from_str(&l).expect("it is told JSON"))
    };
    let out = |b: &[u8]| {
        let mut o = std::io::stdout().lock();
        // Nobody reads it any more: an agent dies of that.
        if o.write_all(b).and_then(|()| o.flush()).is_err() {
            std::process::exit(98);
        }
    };
    for line in script.lines().filter(|l| !l.trim().is_empty() && !l.starts_with("//")) {
        let s: Value = serde_json::from_str(line).unwrap_or_else(|e| panic!("{e}: {line}"));
        let name = s["as"].as_str().unwrap_or("it").to_owned();
        if let Some(p) = s.get("hear").or(s.get("until")) {
            loop {
                let Some(v) = hear() else { std::process::exit(96) };
                if fits(p, &v) {
                    known.insert(name, v);
                    break;
                }
                if s.get("hear").is_some() {
                    eprintln!("expected {p}, heard {v}");
                    std::process::exit(97);
                }
            }
        }
        let times = s["times"].as_u64().unwrap_or(1);
        for n in 0..times {
            if let Some(v) = s.get("say") {
                let mut v = v.clone();
                fill(&mut v, &known, n);
                out(format!("{v}\n").as_bytes());
            }
            if let Some(t) = s["err"].as_str() {
                eprintln!("{t}");
            }
            if let Some(t) = s["raw"].as_str() {
                out(t.as_bytes());
            }
        }
        if let Some(n) = s["long"].as_u64() {
            out(&vec![b'x'; usize::try_from(n).unwrap()]);
        }
        if let Some(b) = s["bytes"].as_array() {
            let b: Vec<u8> = b.iter().map(|x| u8::try_from(x.as_u64().unwrap()).unwrap()).collect();
            out(&b);
        }
        if let Some(f) = s["touch"].as_str() {
            std::fs::write(dir.join(f), "").unwrap();
        }
        if let Some(ms) = s["sleep"].as_u64() {
            std::thread::sleep(Duration::from_millis(ms));
        }
        if let Some(mark) = s["child"].as_str() {
            let c = std::process::Command::new("/bin/sleep").arg(mark).spawn().unwrap();
            std::mem::forget(c);
        }
        if let Some(c) = s["exit"].as_i64() {
            std::process::exit(i32::try_from(c).unwrap());
        }
        if s["abort"] == true {
            std::process::abort();
        }
        if s["hang"] == true {
            std::thread::sleep(Duration::from_secs(600));
        }
    }
    while hear().is_some() {}
}

/// Whether everything in `p` is in `v`.
fn fits(p: &Value, v: &Value) -> bool {
    match (p, v) {
        (Value::Object(p), Value::Object(v)) => {
            p.iter().all(|(k, p)| v.get(k).is_some_and(|v| fits(p, v)))
        }
        (Value::Array(p), Value::Array(v)) => {
            p.len() == v.len() && p.iter().zip(v).all(|(p, v)| fits(p, v))
        }
        _ => p == v,
    }
}

/// `"$a/b"` is what was heard as `a`, there at `/b`; `"$n"` counts.
fn fill(v: &mut Value, known: &BTreeMap<String, Value>, n: u64) {
    match v {
        Value::Object(m) => m.values_mut().for_each(|v| fill(v, known, n)),
        Value::Array(a) => a.iter_mut().for_each(|v| fill(v, known, n)),
        Value::String(s) => {
            if s == "$n" {
                *v = json!(format!("{n}"));
            } else if let Some(r) = s.strip_prefix('$') {
                let (name, at) = r.split_once('/').unwrap();
                *v = known[name].pointer(&format!("/{at}")).cloned().unwrap_or_default();
            }
        }
        _ => {}
    }
}
