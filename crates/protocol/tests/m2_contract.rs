//! Contract tests for the protocol 0.2 additions (ADR-0014, M2-ARCH-001). Same rule as
//! `contract.rs`: a failure means the wire changed; follow `governance.md`, do not edit the JSON.

use axiom_protocol::envelope::{ErrorKind, RpcError};
use axiom_protocol::method::{Method, Notification};
use axiom_protocol::{build, env, registry, PROTOCOL_VERSION};
use serde_json::{json, Value};

fn round_trip<T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(
    v: Value,
) -> T {
    let t: T = serde_json::from_value(v.clone()).expect("deserializes");
    assert_eq!(
        serde_json::to_value(&t).unwrap(),
        v,
        "re-serializes identically"
    );
    t
}

#[test]
fn protocol_version_is_0_2() {
    assert_eq!(PROTOCOL_VERSION, "0.2");
}

#[test]
fn m2_methods_and_notifications_are_registered() {
    let names = registry::method_names();
    for m in [
        env::Providers::NAME,
        env::Detect::NAME,
        env::Versions::NAME,
        env::PlanInstall::NAME,
        env::Install::NAME,
        env::Uninstall::NAME,
        env::Resolve::NAME,
        env::Trust::NAME,
        build::Tasks::NAME,
        build::Run::NAME,
    ] {
        assert!(names.contains(&m), "{m} not registered");
    }
    assert_eq!(
        [
            env::Providers::NAME,
            env::Detect::NAME,
            env::Versions::NAME,
            env::PlanInstall::NAME,
            env::Install::NAME,
            env::Uninstall::NAME,
            env::Resolve::NAME,
            env::Trust::NAME,
            build::Tasks::NAME,
            build::Run::NAME,
        ],
        [
            "env/providers",
            "env/detect",
            "env/versions",
            "env/planInstall",
            "env/install",
            "env/uninstall",
            "env/resolve",
            "env/trust",
            "build/tasks",
            "build/run",
        ]
    );
    let mut g = schemars::SchemaGenerator::default();
    let notes: Vec<_> = registry::notifications(&mut g)
        .into_iter()
        .map(|n| n.name)
        .collect();
    for n in [
        env::Progress::NAME,
        env::Installed::NAME,
        build::Diagnostics::NAME,
    ] {
        assert!(notes.contains(&n), "{n} not registered");
    }
    assert_eq!(env::Progress::NAME, "env/installProgress");
    assert_eq!(env::Installed::NAME, "env/installed");
    assert_eq!(build::Diagnostics::NAME, "build/diagnostics");
}

#[test]
fn m2_error_codes() {
    let cases = [
        (ErrorKind::ChecksumMismatch, -32019, "checksum_mismatch"),
        (
            ErrorKind::UnsupportedPlatform,
            -32020,
            "unsupported_platform",
        ),
        (ErrorKind::ApprovalDenied, -32021, "approval_denied"),
        (
            ErrorKind::PrerequisiteMissing,
            -32022,
            "prerequisite_missing",
        ),
    ];
    let all_old = [
        ErrorKind::ParseError,
        ErrorKind::InvalidRequest,
        ErrorKind::MethodNotFound,
        ErrorKind::InvalidParams,
        ErrorKind::Internal,
        ErrorKind::NotInitialized,
        ErrorKind::Cancelled,
        ErrorKind::Timeout,
        ErrorKind::Unsupported,
        ErrorKind::NotFound,
        ErrorKind::AlreadyExists,
        ErrorKind::PermissionDenied,
        ErrorKind::Conflict,
        ErrorKind::Io,
        ErrorKind::UnsupportedEncoding,
        ErrorKind::TooLarge,
        ErrorKind::WorkspaceNotOpen,
        ErrorKind::SpawnFailed,
        ErrorKind::Unknown,
    ];
    for (kind, code, name) in cases {
        assert_eq!(kind.code(), code);
        assert!(
            all_old.iter().all(|k| k.code() != code),
            "{name} code reused"
        );
        let e = RpcError::new(kind, "x").with_details(json!({"missing": ["cargo"]}));
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["code"], code);
        assert_eq!(v["data"]["kind"], name);
        let back: RpcError = serde_json::from_value(v).unwrap();
        assert_eq!(back.data.kind, kind);
    }
}

#[test]
fn env_wire_shapes() {
    let p: <env::Providers as Method>::Result = round_trip(json!({"providers": [
        {"id":"rust","name":"Rust","description":"Rust via rustup","version":"0.1.0",
         "source":"builtin","trusted":true},
        {"id":"mine","name":"mine","description":"","version":"",
         "source":"workspace","trusted":false,"errors":["line 3: unknown field `x`"]}
    ]}));
    assert_eq!(p.providers[1].source, env::ProviderSource::Workspace);
    round_trip::<<env::Providers as Method>::Params>(json!({}));
    round_trip::<<env::Providers as Method>::Params>(json!({"workspaceId":"w1"}));

    round_trip::<<env::Detect as Method>::Params>(json!({"workspaceId":"w1"}));
    let d: <env::Detect as Method>::Result = round_trip(json!({"providers": [
        {"providerId":"rust","selectedVersion":"1.84.0","tools":[
            {"toolId":"cargo","required":true,"found":{
                "providerId":"rust","toolId":"cargo","version":"1.84.0",
                "location":{"kind":"managed","root":"/data/toolchains/rust/1.84.0"},
                "verifiedMs":1763400000000u64}},
            {"toolId":"rustfmt","required":false}
        ]}
    ]}));
    assert!(d.providers[0].tools[1].found.is_none());
    let sys: env::ToolLocation = round_trip(json!({"kind":"system","path":"/usr/bin/cargo"}));
    assert_eq!(
        sys,
        env::ToolLocation::System {
            path: "/usr/bin/cargo".into()
        }
    );

    round_trip::<<env::Versions as Method>::Params>(json!({"providerId":"rust"}));
    round_trip::<<env::Versions as Method>::Result>(
        json!({"available":["1.84.0","1.83.0"],"installed":["1.83.0"]}),
    );
    round_trip::<<env::PlanInstall as Method>::Params>(
        json!({"providerId":"rust","version":"1.84.0"}),
    );
    round_trip::<<env::PlanInstall as Method>::Params>(
        json!({"providerId":"rust","version":"1.84.0","method":1}),
    );
    round_trip::<<env::Install as Method>::Params>(json!({"planId":"p-1"}));
    let r = json!({"planId":"p-1","providerId":"rust","version":"1.84.0",
        "root":"/data/toolchains/rust/1.84.0","verified":[]});
    round_trip::<<env::Install as Method>::Result>(r.clone());
    round_trip::<<env::Installed as Notification>::Params>(r);
    round_trip::<<env::Progress as Notification>::Params>(json!({
        "planId":"p-1","step":0,"steps":3,"label":"Downloading","bytesDone":10,"bytesTotal":100
    }));
    round_trip::<<env::Progress as Notification>::Params>(
        json!({"planId":"p-1","step":2,"steps":3,"label":"Verifying cargo"}),
    );
    round_trip::<<env::Uninstall as Method>::Params>(
        json!({"providerId":"rust","version":"1.84.0"}),
    );
    round_trip::<<env::Uninstall as Method>::Result>(json!({}));

    round_trip::<<env::Resolve as Method>::Params>(json!({"workspaceId":"w1"}));
    let e: <env::Resolve as Method>::Result = round_trip(json!({
        "toolchains":[{"providerId":"rust","toolId":"cargo","version":"1.84.0",
            "location":{"kind":"managed","root":"/r"},"verifiedMs":1}],
        "pathPrepend":["/r/bin"],
        "vars":{"CARGO_HOME":"/r/cargo","RUSTUP_HOME":"/r/rustup"}
    }));
    assert_eq!(e.path_prepend, ["/r/bin"]);
    round_trip::<<env::Trust as Method>::Params>(json!({"workspaceId":"w1","trusted":true}));
}

#[test]
fn install_plan_roundtrip() {
    let plan = json!({
        "planId":"p-7f3a","providerId":"node","version":"22.11.0","scope":"managed",
        "steps":[
            {"kind":"download","url":"https://nodejs.org/dist/v22.11.0/node-v22.11.0-linux-x64.tar.xz",
             "bytes":29000000,
             "sha256":{"kind":"url","url":"https://nodejs.org/dist/v22.11.0/SHASUMS256.txt"}},
            {"kind":"download","url":"https://example.org/a.zip",
             "sha256":{"kind":"digest","value":"ab12"}},
            {"kind":"extract","format":"tar.xz","stripComponents":1},
            {"kind":"run","argv":["bin/node","--version"],"env":{}},
            {"kind":"systemPackages","manager":"apt","packages":["build-essential"],
             "argv":["apt-get","install","-y","build-essential"]},
            {"kind":"verify","toolId":"node"}
        ],
        "downloadBytes":29000000,
        "effects":["network","writeManaged","runPrograms","systemChange","elevation"],
        "expiresMs":1763400600000u64
    });
    let p: env::InstallPlan = round_trip(plan);
    assert_eq!(p.steps.len(), 6);
    assert!(matches!(
        &p.steps[0],
        env::Step::Download {
            sha256: env::Sha256Source::Url { .. },
            ..
        }
    ));
    assert!(matches!(
        p.steps[2],
        env::Step::Extract {
            strip_components: 1,
            ..
        }
    ));
    // A plan without the optional total still parses (unknown sizes).
    let mut v = serde_json::to_value(&p).unwrap();
    v.as_object_mut().unwrap().remove("downloadBytes");
    let q: env::InstallPlan = round_trip(v);
    assert_eq!(q.download_bytes, None);
    // Unknown step kinds are rejected: a client must never approve a step it cannot show.
    assert!(serde_json::from_value::<env::Step>(json!({"kind":"curlPipeSh"})).is_err());
}

#[test]
fn build_wire_shapes() {
    round_trip::<<build::Tasks as Method>::Params>(json!({"workspaceId":"w1"}));
    round_trip::<<build::Tasks as Method>::Result>(json!({"tasks":[
        {"providerId":"rust","id":"build","kind":"build","label":"cargo build"},
        {"providerId":"rust","id":"test","kind":"test","label":"cargo test"}
    ]}));
    round_trip::<<build::Run as Method>::Params>(
        json!({"workspaceId":"w1","providerId":"rust","task":"build"}),
    );
    round_trip::<<build::Run as Method>::Result>(json!({"taskId":"t-3"}));
    let d: <build::Diagnostics as Notification>::Params = round_trip(json!({
        "taskId":"t-3",
        "diagnostics":[{
            "severity":"error","message":"mismatched types","source":"rustc","code":"E0308",
            "location":{"file":"src/main.rs","line":4,"column":18},"related":[]
        }],
        "complete":false
    }));
    assert!(!d.complete);
    round_trip::<<build::Diagnostics as Notification>::Params>(
        json!({"taskId":"t-3","diagnostics":[],"complete":true}),
    );
}
