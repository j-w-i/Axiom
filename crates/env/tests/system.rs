//! System installs through the host package manager (M2-ENV-004), with fake managers and a
//! fake pkexec on a private PATH. Nothing here runs a real package manager.

use axiom_env::system::{detect_manager, SystemHost};
use axiom_env::{EnvError, Http, HttpConfig, Installer};
use axiom_protocol::env::{Effect, Step};
use axiom_protocol::lifecycle::HostOs;
use axiom_protocol::provider::{InstallScope, ProviderManifest};
use std::path::{Path, PathBuf};

/// Creates empty "executables" named like `program` on this OS; returns the first path.
fn fake(dir: &Path, programs: &[&str]) -> Vec<PathBuf> {
    std::fs::create_dir_all(dir).unwrap();
    programs
        .iter()
        .map(|p| {
            let name = axiom_env::detect::executable_names(p).remove(0);
            let path = dir.join(name);
            std::fs::write(&path, "").unwrap();
            path
        })
        .collect()
}

fn host(os: HostOs, dir: &Path, os_release: Option<&str>) -> SystemHost {
    SystemHost {
        os,
        path: Some(std::env::join_paths([dir]).unwrap()),
        os_release: os_release.map(String::from),
        sudo_askpass: None,
    }
}

#[test]
fn detects_manager() {
    let t = tempfile::tempdir().unwrap();
    let bin = t.path().join("bin");
    fake(&bin, &["apt-get", "dnf", "brew", "winget"]);
    let name = |h: &SystemHost| detect_manager(h).map(|m| m.name);
    // The distribution's own manager wins when several are installed.
    assert_eq!(
        name(&host(
            HostOs::Linux,
            &bin,
            Some("ID=fedora\nVERSION_ID=44\n")
        )),
        Some("dnf")
    );
    assert_eq!(
        name(&host(
            HostOs::Linux,
            &bin,
            Some("ID=linuxmint\nID_LIKE=\"ubuntu debian\"\n")
        )),
        Some("apt")
    );
    // Unknown distribution: first of apt, dnf, pacman present.
    assert_eq!(
        name(&host(HostOs::Linux, &bin, Some("ID=gentoo\n"))),
        Some("apt")
    );
    assert_eq!(name(&host(HostOs::Linux, &bin, None)), Some("apt"));
    assert_eq!(name(&host(HostOs::Macos, &bin, None)), Some("brew"));
    assert_eq!(name(&host(HostOs::Windows, &bin, None)), Some("winget"));
    // Native manager absent: fall back to what is present.
    let only_pacman = t.path().join("p");
    fake(&only_pacman, &["pacman"]);
    assert_eq!(
        name(&host(HostOs::Linux, &only_pacman, Some("ID=fedora\n"))),
        Some("pacman")
    );
    let empty = t.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    assert_eq!(
        name(&host(HostOs::Linux, &empty, Some("ID=fedora\n"))),
        None
    );
}

fn manifest() -> ProviderManifest {
    axiom_env::manifest::parse(
        r#"schema = "axiom.provider/1"
id = "hello"
name = "Hello"
description = "d"
version = "0.1.0"

[[tools]]
id = "hello"
program = "hello"
version_regex = 'hello \(GNU Hello\) (\d+\.\d+)'

[[install]]
kind = "system_package"
packages = { dnf = ["hello"], apt = ["hello"], brew = ["hello"], winget = ["GNU.Hello", "Other.Thing"] }
"#,
    )
    .unwrap()
}

fn installer(data: &Path, h: SystemHost) -> Installer {
    Installer::new(
        data.to_path_buf(),
        Http::new(HttpConfig::default()).unwrap(),
    )
    .with_system_host(h)
}

#[tokio::test]
async fn plan_lists_exact_argv() {
    let t = tempfile::tempdir().unwrap();
    let bin = t.path().join("bin");
    let paths = fake(&bin, &["dnf", "pkexec"]);
    let (dnf, pkexec) = (
        paths[0].to_string_lossy().into_owned(),
        paths[1].to_string_lossy().into_owned(),
    );
    let m = manifest();
    let inst = installer(t.path(), host(HostOs::Linux, &bin, Some("ID=fedora\n")));

    // Never chosen by default: system changes are only ever planned when asked for.
    assert!(matches!(
        inst.plan(&m, "system", None).await,
        Err(EnvError::UnsupportedPlatform(_))
    ));

    let plan = inst.plan(&m, "system", Some(0)).await.unwrap();
    assert_eq!(plan.scope, InstallScope::System);
    assert_eq!(
        plan.effects,
        [
            Effect::Network,
            Effect::SystemChange,
            Effect::Elevation,
            Effect::RunPrograms
        ]
    );
    let Step::SystemPackages {
        manager,
        packages,
        argv,
    } = &plan.steps[0]
    else {
        panic!("{:?}", plan.steps)
    };
    assert_eq!(
        (manager.as_str(), packages.as_slice()),
        ("dnf", &["hello".to_string()][..])
    );
    assert_eq!(
        argv,
        &[pkexec, dnf, "install".into(), "-y".into(), "hello".into()]
    );
    assert!(matches!(&plan.steps[1], Step::Verify { tool_id } if tool_id == "hello"));
    assert_eq!(plan.steps.len(), 2);

    // Without pkexec: sudo -A only when an askpass helper is configured; else refused.
    let no_pk = t.path().join("nopk");
    let p = fake(&no_pk, &["dnf", "sudo"]);
    let mut h = host(HostOs::Linux, &no_pk, Some("ID=fedora\n"));
    match installer(t.path(), h.clone())
        .plan(&m, "system", Some(0))
        .await
    {
        Err(EnvError::PrerequisiteMissing(_, missing)) => assert_eq!(missing, ["pkexec"]),
        other => panic!("{other:?}"),
    }
    h.sudo_askpass = Some("/usr/libexec/askpass".into());
    let plan = installer(t.path(), h)
        .plan(&m, "system", Some(0))
        .await
        .unwrap();
    let Step::SystemPackages { argv, .. } = &plan.steps[0] else {
        panic!()
    };
    assert_eq!(
        argv[..3],
        [
            p[1].to_string_lossy().into_owned(),
            "-A".into(),
            p[0].to_string_lossy().into_owned()
        ]
    );

    // macOS: Homebrew runs unelevated (it refuses root); Windows: one winget call per id.
    let mac = t.path().join("mac");
    let brew = fake(&mac, &["brew"]).remove(0);
    let plan = installer(t.path(), host(HostOs::Macos, &mac, None))
        .plan(&m, "system", Some(0))
        .await
        .unwrap();
    assert!(!plan.effects.contains(&Effect::Elevation));
    let Step::SystemPackages { argv, .. } = &plan.steps[0] else {
        panic!()
    };
    assert_eq!(
        argv,
        &[
            brew.to_string_lossy().into_owned(),
            "install".into(),
            "hello".into()
        ]
    );
    let win = t.path().join("win");
    let winget = fake(&win, &["winget"])
        .remove(0)
        .to_string_lossy()
        .into_owned();
    let plan = installer(t.path(), host(HostOs::Windows, &win, None))
        .plan(&m, "system", Some(0))
        .await
        .unwrap();
    let calls: Vec<_> = plan
        .steps
        .iter()
        .filter_map(|s| match s {
            Step::SystemPackages { argv, .. } => Some(argv.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[0][..5],
        [
            winget,
            "install".into(),
            "--exact".into(),
            "--id".into(),
            "GNU.Hello".into()
        ]
    );

    // A manager the manifest lists nothing for.
    let arch = t.path().join("arch");
    fake(&arch, &["pacman", "pkexec"]);
    assert!(matches!(
        installer(t.path(), host(HostOs::Linux, &arch, Some("ID=arch\n")))
            .plan(&m, "system", Some(0))
            .await,
        Err(EnvError::UnsupportedPlatform(_))
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn runs_only_approved_plan() {
    use std::os::unix::fs::PermissionsExt;
    let t = tempfile::tempdir().unwrap();
    let bin = t.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let log = t.path().join("log");
    let script = |name: &str, body: String| {
        let p = bin.join(name);
        std::fs::write(&p, body).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    };
    // pkexec shim: records its argv, then runs it (as polkit would after the prompt).
    script(
        "pkexec",
        format!(
            "#!/bin/sh\necho \"pkexec $*\" >> '{}'\nexec \"$@\"\n",
            log.display()
        ),
    );
    // dnf shim: records its argv and "installs" hello into the same bin dir.
    script(
        "dnf",
        format!(
            "#!/bin/sh\necho \"dnf $*\" >> '{log}'\necho 'Installing: hello'\nprintf '#!/bin/sh\\necho \"hello (GNU Hello) 2.12\"\\n' > '{bin}/hello'\nchmod +x '{bin}/hello'\n",
            log = log.display(),
            bin = bin.display()
        ),
    );
    let m = manifest();
    let inst = installer(t.path(), host(HostOs::Linux, &bin, Some("ID=fedora\n")));
    let tok = tokio_util::sync::CancellationToken::new();

    let plan = inst.plan(&m, "system", Some(0)).await.unwrap();
    assert!(!log.exists(), "planning runs nothing");
    assert!(matches!(
        inst.install("not-a-plan", Some(&m), &tok, &|_| {}).await,
        Err(EnvError::ApprovalDenied(_))
    ));
    assert!(!log.exists(), "an unapproved id runs nothing");

    let labels = std::sync::Mutex::new(Vec::new());
    let r = inst
        .install(&plan.plan_id, Some(&m), &tok, &|p| {
            labels.lock().unwrap().push(p.label)
        })
        .await
        .unwrap();
    let Step::SystemPackages { argv, .. } = &plan.steps[0] else {
        panic!()
    };
    let logged = std::fs::read_to_string(&log).unwrap();
    // Exactly the command the plan showed: pkexec <dnf> install -y hello, then dnf itself.
    assert_eq!(
        logged.lines().collect::<Vec<_>>(),
        [
            format!("pkexec {}", argv[1..].join(" ")),
            "dnf install -y hello".to_string()
        ]
    );
    assert_eq!(r.root, None);
    assert_eq!(r.verified[0].version, "2.12");
    assert!(
        matches!(&r.verified[0].location, axiom_protocol::env::ToolLocation::System { path } if path.ends_with("/hello"))
    );
    let labels = labels.into_inner().unwrap();
    assert!(
        labels.iter().any(|l| l == "Installing: hello"),
        "output streamed as progress: {labels:?}"
    );
    assert!(
        !t.path().join("toolchains").exists(),
        "nothing written to managed storage"
    );
    // Spent.
    assert!(matches!(
        inst.install(&plan.plan_id, Some(&m), &tok, &|_| {}).await,
        Err(EnvError::ApprovalDenied(_))
    ));
}
