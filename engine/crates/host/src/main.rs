use std::{error::Error, io::Write, process::ExitCode, time::Duration};

use axiom_host::{Config, Host, HostError, mcp, paths::Paths, signal, vault};

fn main() -> ExitCode {
    #[cfg(unix)]
    if std::env::args().nth(1).as_deref() == Some(axiom_spawn::reaper::ARG) {
        axiom_spawn::reaper::run(std::io::stdin().lock());
        return ExitCode::SUCCESS;
    }
    if std::env::args().nth(1).as_deref() == Some(mcp::ARG) {
        return bridge();
    }
    let Err(e) = run() else {
        return ExitCode::SUCCESS;
    };
    // Nobody may be there to read it; so with "ready".
    let _ = writeln!(std::io::stderr(), "axiom-host: {e}");
    match e.downcast_ref() {
        Some(HostError::AlreadyRunning) => ExitCode::from(3),
        _ => ExitCode::FAILURE,
    }
}

/// For an installed program: see [`mcp`].
fn bridge() -> ExitCode {
    let Some(file) = std::env::args_os().nth(2) else { return ExitCode::FAILURE };
    let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
        return ExitCode::FAILURE;
    };
    let r = rt.block_on(mcp::serve(std::path::Path::new(&file)));
    // What still reads its input does not keep it.
    rt.shutdown_background();
    if r.is_ok() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let mut cfg = Config::new(Paths::discover().ok_or("no data directory: set AXIOM_DATA")?);
    cfg.vault = vault::of(&cfg.paths);
    cfg.bridge = std::env::current_exe().ok();
    while let Some(a) = args.next() {
        match (a.as_str(), args.next()) {
            ("--idle-exit", Some(v)) => {
                let secs = v.parse().map_err(|e| format!("--idle-exit {v}: {e}"))?;
                cfg.idle_exit = Some(Duration::from_secs(secs));
            }
            _ => return Err(format!("{a}? usage: axiom-host [--idle-exit SECONDS]").into()),
        }
    }
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    let r = rt.block_on(async {
        let stop = signal::stop()?;
        let h = Host::start(cfg)?;
        // Before anything is started by it.
        #[cfg(unix)]
        axiom_spawn::reaper::start(&std::env::current_exe()?)?;
        let _ = writeln!(std::io::stdout(), "ready");
        h.serve_until(async {
            stop.await;
        })
        .await
    });
    // Everything that matters has ended. What still reads a file that does not answer may not.
    rt.shutdown_timeout(Duration::from_secs(1));
    Ok(r?)
}
