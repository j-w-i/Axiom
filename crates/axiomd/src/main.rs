use axiom_rpc::{serve, ServerInfo};
use axiom_store::{default_db_path, Store, StoreError};
use axiomd::{build_router, App};
use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Copy, ValueEnum)]
enum Level {
    Error,
    Warn,
    Info,
    Debug,
}

/// The Axiom daemon. Speaks protocol 0.1 on stdin/stdout; logs to stderr.
#[derive(Parser)]
#[command(name = "axiomd", version)]
struct Cli {
    /// Directory for the database (overrides AXIOM_DATA_DIR).
    #[arg(long)]
    data_dir: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "warn")]
    log_level: Level,
}

fn main() {
    let cli = Cli::parse();
    // stdout is the protocol channel: logs go to stderr only.
    let level = match cli.log_level {
        Level::Error => tracing::Level::ERROR,
        Level::Warn => tracing::Level::WARN,
        Level::Info => tracing::Level::INFO,
        Level::Debug => tracing::Level::DEBUG,
    };
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(level)
        .with_ansi(false)
        .init();

    let db_path = match &cli.data_dir {
        Some(d) => d.join("axiom.db"),
        None => default_db_path(),
    };
    let store = match Store::open(&db_path) {
        Ok(s) => s,
        Err(e @ StoreError::SchemaTooNew { .. }) => {
            eprintln!("axiomd: {e}");
            std::process::exit(2);
        }
        Err(e) => {
            eprintln!("axiomd: cannot open the database: {e}");
            std::process::exit(1);
        }
    };
    if let Some(moved) = store.recovered_from_corruption() {
        tracing::error!(
            "the database was corrupt and was moved to {}; starting with a fresh one",
            moved.display()
        );
    }

    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("axiomd: cannot start the async runtime: {e}");
            std::process::exit(1);
        }
    };
    let code = rt.block_on(async {
        let app = App::new(store);
        let router = Arc::new(build_router(app));
        let info = ServerInfo {
            name: "axiomd".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        };
        match serve(tokio::io::stdin(), tokio::io::stdout(), router, info).await {
            Ok(()) => 0,
            Err(e) => {
                tracing::error!("connection ended with an error: {e}");
                1
            }
        }
    });
    // A blocked stdin read must not keep the process alive.
    rt.shutdown_background();
    std::process::exit(code);
}
