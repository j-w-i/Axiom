use std::{process::ExitCode, time::Duration};

use axiom_host::{Config, Host, HostError, paths::Paths};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(paths) = Paths::discover() else {
        eprintln!("axiom-host: no data directory: set AXIOM_DATA");
        return ExitCode::FAILURE;
    };
    let mut cfg = Config::new(paths);
    while let Some(a) = args.next() {
        match (a.as_str(), args.next()) {
            ("--idle-exit", Some(v)) => match v.parse() {
                Ok(s) => cfg.idle_exit = Some(Duration::from_secs(s)),
                Err(e) => {
                    eprintln!("axiom-host: --idle-exit {v}: {e}");
                    return ExitCode::FAILURE;
                }
            },
            (other, _) => {
                eprintln!("axiom-host: {other}? usage: axiom-host [--idle-exit SECONDS]");
                return ExitCode::FAILURE;
            }
        }
    }
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("axiom-host: {e}");
            return ExitCode::FAILURE;
        }
    };
    let r = rt.block_on(async {
        let h = Host::start(cfg)?;
        println!("ready");
        h.serve().await
    });
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(HostError::AlreadyRunning) => {
            eprintln!("axiom-host: {}", HostError::AlreadyRunning);
            ExitCode::from(3)
        }
        Err(e) => {
            eprintln!("axiom-host: {e}");
            ExitCode::FAILURE
        }
    }
}
