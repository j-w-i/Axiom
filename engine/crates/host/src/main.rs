use std::{error::Error, io::Write, process::ExitCode, time::Duration};

use axiom_host::{Config, Host, HostError, paths::Paths, signal};

fn main() -> ExitCode {
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

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let mut cfg = Config::new(Paths::discover().ok_or("no data directory: set AXIOM_DATA")?);
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
