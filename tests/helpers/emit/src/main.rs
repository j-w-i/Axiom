//! `emit` — a tiny cross-platform stand-in for `echo`, `sleep`, `seq`, `printf`
//! and `exit`, so shell and daemon tests do not depend on a POSIX userland.
//!
//! Actions run left to right:
//!   out <text>        write text to stdout, then a newline
//!   err <text>        write text to stderr, then a newline
//!   raw-out <text>    write text to stdout with no newline (escapes \e \r \n \t are decoded)
//!   raw-err <text>    same, to stderr
//!   seq <from> <to>   write each number on its own line to stdout
//!   repeat <n> <text> write <text> + newline n times to stdout
//!   sleep <ms>        sleep
//!   cwd               print the current directory to stdout
//!   env <NAME>        print the value of an environment variable (empty if unset)
//!   spawn-sleep <ms>  start a detached `emit sleep <ms>` child and print its pid
//!   exit <code>       exit immediately with that code
//! Unknown actions are an error (exit 64). Without `exit`, the exit code is 0.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

fn decode(s: &str) -> String {
    s.replace("\\e", "\u{1b}")
        .replace("\\r", "\r")
        .replace("\\n", "\n")
        .replace("\\t", "\t")
}

fn usage(msg: &str) -> ! {
    eprintln!("emit: {msg}");
    std::process::exit(64);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut i = 0;
    let next = |i: &mut usize, what: &str| -> String {
        *i += 1;
        args.get(*i)
            .cloned()
            .unwrap_or_else(|| usage(&format!("missing argument for {what}")))
    };
    while i < args.len() {
        let action = args[i].clone();
        match action.as_str() {
            "out" => {
                let t = next(&mut i, "out");
                let mut o = stdout.lock();
                let _ = writeln!(o, "{t}");
                let _ = o.flush();
            }
            "err" => {
                let t = next(&mut i, "err");
                let mut e = stderr.lock();
                let _ = writeln!(e, "{t}");
                let _ = e.flush();
            }
            "raw-out" => {
                let t = decode(&next(&mut i, "raw-out"));
                let mut o = stdout.lock();
                let _ = o.write_all(t.as_bytes());
                let _ = o.flush();
            }
            "raw-err" => {
                let t = decode(&next(&mut i, "raw-err"));
                let mut e = stderr.lock();
                let _ = e.write_all(t.as_bytes());
                let _ = e.flush();
            }
            "seq" => {
                let from: i64 = next(&mut i, "seq")
                    .parse()
                    .unwrap_or_else(|_| usage("seq: bad from"));
                let to: i64 = next(&mut i, "seq")
                    .parse()
                    .unwrap_or_else(|_| usage("seq: bad to"));
                let mut o = std::io::BufWriter::new(stdout.lock());
                for n in from..=to {
                    let _ = writeln!(o, "{n}");
                }
                let _ = o.flush();
            }
            "repeat" => {
                let n: u64 = next(&mut i, "repeat")
                    .parse()
                    .unwrap_or_else(|_| usage("repeat: bad count"));
                let t = next(&mut i, "repeat");
                let mut o = std::io::BufWriter::new(stdout.lock());
                for _ in 0..n {
                    let _ = writeln!(o, "{t}");
                }
                let _ = o.flush();
            }
            "sleep" => {
                let ms: u64 = next(&mut i, "sleep")
                    .parse()
                    .unwrap_or_else(|_| usage("sleep: bad ms"));
                std::thread::sleep(Duration::from_millis(ms));
            }
            "cwd" => {
                let d = std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                let mut o = stdout.lock();
                let _ = writeln!(o, "{d}");
                let _ = o.flush();
            }
            "env" => {
                let name = next(&mut i, "env");
                let mut o = stdout.lock();
                let _ = writeln!(o, "{}", std::env::var(&name).unwrap_or_default());
                let _ = o.flush();
            }
            "spawn-sleep" => {
                let ms = next(&mut i, "spawn-sleep");
                let exe = std::env::current_exe().unwrap_or_else(|_| usage("cannot find own path"));
                match Command::new(exe)
                    .args(["sleep", &ms])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                {
                    Ok(child) => {
                        let mut o = stdout.lock();
                        let _ = writeln!(o, "{}", child.id());
                        let _ = o.flush();
                    }
                    Err(e) => usage(&format!("spawn-sleep: {e}")),
                }
            }
            "exit" => {
                let code: i32 = next(&mut i, "exit")
                    .parse()
                    .unwrap_or_else(|_| usage("exit: bad code"));
                std::process::exit(code);
            }
            other => usage(&format!("unknown action `{other}`")),
        }
        i += 1;
    }
}
