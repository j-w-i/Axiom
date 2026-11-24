//! Host benchmark: N agents streaming at R tokens/s each through the real process and socket.
//! `cargo run --release -p axiom-host --example bench`. Prints one JSON line per scenario.

// What it prints is what it is for.
#![allow(clippy::print_stdout)]

use std::{
    io::{BufRead, BufReader},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use axiom_host::{client::Client, paths::Paths};
use axiom_proto::{Body, Call, Message, SessionId, now_ms};

type R<T> = Result<T, Box<dyn std::error::Error>>;

fn main() -> R<()> {
    let bin = std::env::current_exe()?
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join(format!("axiom-host{}", std::env::consts::EXE_SUFFIX)))
        .ok_or("no target directory")?;
    if !bin.exists() {
        return Err(format!("build the host first: {}", bin.display()).into());
    }
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    rt.block_on(async {
        start(&bin)?;
        for (agents, rate, secs) in [(1, 100, 5), (10, 100, 8), (100, 100, 8)] {
            stream(&bin, agents, rate, secs).await?;
        }
        Ok(())
    })
}

fn dir() -> R<tempdir::Dir> {
    tempdir::Dir::new()
}

fn spawn(bin: &std::path::Path, p: &Paths) -> R<(std::process::Child, f64)> {
    let t = Instant::now();
    let mut c = Command::new(bin).env("AXIOM_DATA", &p.data).stdout(Stdio::piped()).spawn()?;
    let mut l = String::new();
    BufReader::new(c.stdout.take().ok_or("no stdout")?).read_line(&mut l)?;
    Ok((c, t.elapsed().as_secs_f64() * 1e3))
}

fn pct(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(f64::total_cmp);
    v.get(((v.len() as f64 * p) as usize).min(v.len().saturating_sub(1))).copied().unwrap_or(0.0)
}

/// CPU seconds and resident MB, as `ps` reports them.
fn usage(pid: u32) -> R<(f64, f64)> {
    let o = Command::new("ps").args(["-o", "time=,rss=", "-p", &pid.to_string()]).output()?;
    let s = String::from_utf8_lossy(&o.stdout).into_owned();
    let mut w = s.split_whitespace();
    let cpu = w
        .next()
        .unwrap_or("0")
        .split(':')
        .try_fold(0.0, |a, p| p.parse::<f64>().map(|p| a * 60.0 + p))?;
    Ok((cpu, w.next().unwrap_or("0").parse::<f64>()? / 1024.0))
}

fn start(bin: &std::path::Path) -> R<()> {
    let mut t = Vec::new();
    for _ in 0..20 {
        let d = dir()?;
        let p = Paths::new(d.0.join("d"));
        let (mut c, ms) = spawn(bin, &p)?;
        t.push(ms);
        c.kill()?;
        c.wait()?;
    }
    println!(
        r#"{{"bench":"start","p50_ms":{:.1},"p99_ms":{:.1}}}"#,
        pct(&mut t, 0.5),
        pct(&mut t, 0.99)
    );
    Ok(())
}

async fn stream(bin: &std::path::Path, agents: usize, rate: usize, secs: usize) -> R<()> {
    let d = dir()?;
    let p = Paths::new(d.0.join("d"));
    let (mut host, _) = spawn(bin, &p)?;
    let script = d.0.join("s.jsonl");
    let mut s = String::from("{\"do\":\"agent\",\"name\":\"B\"}\n");
    let n = rate * secs;
    for i in 0..n {
        if i % 400 == 0 {
            if i > 0 {
                s += &format!(
                    "{{\"do\":\"end\",\"item\":\"{}\",\"outcome\":\"completed\"}}\n",
                    i - 400
                );
            }
            s += &format!("{{\"do\":\"start\",\"item\":\"{i}\",\"kind\":\"prose\"}}\n");
        }
        let after = (i + 1) * 1000 / rate - i * 1000 / rate;
        s += &format!(
            "{{\"after\":{after},\"do\":\"text\",\"item\":\"{}\",\"text\":\"token \"}}\n",
            i / 400 * 400
        );
    }
    std::fs::write(&script, s)?;
    let mut c = Client::connect(&p, "bench").await?;
    let mut sessions = Vec::new();
    for i in 0..agents {
        let v = c.call(Call::SessionCreate { title: format!("B{i}"), workspace: None }).await?;
        let id = SessionId(v["session"].as_str().unwrap_or_default().into());
        c.call(Call::Follow { session: id.clone(), after: 0 }).await?;
        sessions.push(id);
    }
    let (cpu0, _) = usage(host.id())?;
    let t = Instant::now();
    // From another connection: a start is answered when it is on the drive, and the events of those
    // started before are to be read as they come.
    let mut starter = Client::connect(&p, "bench").await?;
    let path = script.display().to_string();
    let starting = tokio::spawn(async move {
        for session in sessions {
            let call = Call::ReplayStart { session, path: path.clone(), speed: 1.0 };
            starter.call(call).await.map_err(|e| e.to_string())?;
        }
        Ok::<_, String>(())
    });
    let (mut ended, mut text, mut events, mut msgs, mut resync) = (0, 0usize, 0usize, 0usize, 0);
    let mut lag = Vec::new();
    while ended < agents {
        let Some(m) = c.next_within(Duration::from_secs(10)).await? else {
            break;
        };
        msgs += 1;
        match m {
            Message::Events { events: ev, .. } => {
                let now = now_ms();
                for e in ev {
                    events += 1;
                    match e.body {
                        Body::ItemDelta { text: x, .. } => {
                            text += x.len();
                            lag.push(now.saturating_sub(e.at) as f64);
                        }
                        Body::RunEnded { .. } => ended += 1,
                        _ => {}
                    }
                }
            }
            Message::Resync { .. } => resync += 1,
            _ => {}
        }
    }
    starting.await??;
    let wall = t.elapsed().as_secs_f64();
    let (cpu1, rss) = usage(host.id())?;
    let db: u64 = std::fs::read_dir(&p.data)?
        .flatten()
        .filter_map(|e| e.metadata().ok())
        .filter(std::fs::Metadata::is_file)
        .map(|m| m.len())
        .sum();
    println!(
        r#"{{"bench":"stream","agents":{agents},"tokens_per_s":{rate},"cpu_percent":{:.1},"rss_mb":{rss:.1},"tokens":{},"delta_bytes":{text},"events":{events},"messages":{msgs},"resyncs":{resync},"commit_to_client_ms_p50":{:.0},"p99":{:.0},"max":{:.0},"journal_mb":{:.1},"wall_s":{wall:.1}}}"#,
        (cpu1 - cpu0) / wall * 100.0,
        n * agents,
        pct(&mut lag, 0.5),
        pct(&mut lag, 0.99),
        pct(&mut lag, 1.0),
        db as f64 / 1e6,
    );
    host.kill()?;
    host.wait()?;
    Ok(())
}

mod tempdir {
    pub struct Dir(pub std::path::PathBuf);
    impl Dir {
        pub fn new() -> super::R<Self> {
            let p =
                std::env::temp_dir().join(format!("axiom-bench-{}", axiom_proto::ItemId::new()));
            std::fs::create_dir_all(&p)?;
            Ok(Self(p))
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
