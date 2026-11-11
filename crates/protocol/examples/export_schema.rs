//! Writes the generated schema files. Run from the repository root:
//! `cargo run -p axiom-protocol --example export_schema`
//! With `--check`, exits 1 if any committed file is stale (used in CI).
use axiom_protocol::registry;
use std::{fs, path::Path, process::ExitCode};

fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let files = [
        ("ipc/protocol.schema.json", registry::protocol_document()),
        (
            "provider/manifest.schema.json",
            registry::provider_manifest_document(),
        ),
        (
            "inspection/inspection.schema.json",
            registry::inspection_document(),
        ),
    ];
    let check = std::env::args().any(|a| a == "--check");
    let mut stale = false;
    for (rel, doc) in files {
        let path = root.join(rel);
        let mut text = serde_json::to_string_pretty(&doc).unwrap();
        text.push('\n');
        if check {
            if fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
                eprintln!("stale: schemas/{rel}");
                stale = true;
            }
        } else {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, text).unwrap();
            println!("wrote schemas/{rel}");
        }
    }
    if stale {
        eprintln!("regenerate with: cargo run -p axiom-protocol --example export_schema");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
