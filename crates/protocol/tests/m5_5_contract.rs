//! M5.5-ARCH-001 (ADR-0021 §9): the additive contract for every language. Existing
//! expectations elsewhere are unchanged.

use axiom_protocol::{lsp, provider};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

fn round_trip<T: Serialize + DeserializeOwned>(v: Value) -> T {
    let t: T = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(serde_json::to_value(&t).unwrap(), v);
    t
}

#[test]
fn a_server_on_standby() {
    let s: lsp::ServersResult = round_trip(json!({"servers":[
        {"serverId":"pyright","providerId":"lsp-pyright","languages":["python"],"state":"standby"}
    ]}));
    assert_eq!(s.servers[0].state, lsp::ServerState::Standby);
}

#[test]
fn a_server_only_provider_with_requires() {
    let m: provider::ProviderManifest = toml::from_str(
        r#"
schema = "axiom.provider/1"
id = "lsp-jdtls"
name = "Eclipse JDT Language Server"
description = "Java language server"
version = "0.1.0"

[[tools]]
id = "jdtls"
program = "jdtls"
version_regex = '(\d+\.\d+\.\d+)'

[[lsp]]
id = "jdtls"
languages = ["java"]
command = ["jdtls"]
requires = ["java"]
"#,
    )
    .unwrap();
    assert_eq!(m.lsp[0].requires, vec!["java".to_string()]);
    // No `[project]` table: the provider detects nothing; its server applies by language.
    assert_eq!(m.project, provider::ProjectDetection::default());
    // A spec without `requires` serialises as before.
    let old = provider::LspServerSpec {
        id: "rust-analyzer".into(),
        languages: vec!["rust".into()],
        command: vec!["rust-analyzer".into()],
        initialization_options: None,
        install_hint: None,
        requires: vec![],
    };
    assert_eq!(
        serde_json::to_value(&old).unwrap(),
        json!({"id":"rust-analyzer","languages":["rust"],"command":["rust-analyzer"]})
    );
}
