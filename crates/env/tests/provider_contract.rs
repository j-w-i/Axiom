//! The provider contract suite (`providers.md` §Contract tests, items 1–5), run over every
//! manifest under `providers/`. Item 6 (CI installs) arrives with the install tasks.

use axiom_env::manifest::{self, TEMPLATE_VARS};
use axiom_env::registry::{Builtin, Registry, Sources, BUILTIN};
use axiom_env::HostPaths;
use axiom_protocol::provider::{Checksum, InstallMethod};
use std::path::PathBuf;

fn providers_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../providers")
}

fn manifests() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = std::fs::read_dir(providers_dir())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().join("provider.toml").is_file())
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                std::fs::read_to_string(e.path().join("provider.toml")).unwrap(),
            )
        })
        .collect();
    out.sort();
    assert!(!out.is_empty());
    out
}

/// 1. Parses and validates; its id is its directory name.
#[test]
fn every_manifest_parses_and_validates() {
    for (dir, text) in manifests() {
        let m = manifest::parse(&text).unwrap_or_else(|e| panic!("providers/{dir}: {e:?}"));
        assert_eq!(
            m.id, dir,
            "providers/{dir}: id must equal the directory name"
        );
    }
}

/// 2. Every `version_regex` compiles with exactly one capture group.
#[test]
fn every_version_regex_has_one_group() {
    for (dir, text) in manifests() {
        let m = manifest::parse(&text).unwrap();
        for t in &m.tools {
            manifest::version_regex(&t.version_regex)
                .unwrap_or_else(|e| panic!("providers/{dir} tool {}: {e}", t.id));
        }
    }
}

/// 3. Templates use only the allowed variables (checked over the raw text, every field).
#[test]
fn every_template_uses_allowed_variables() {
    for (dir, text) in manifests() {
        for line in text.lines().filter(|l| !l.trim_start().starts_with('#')) {
            for v in manifest::template_vars(line).unwrap() {
                // `${linux}`: a method's own name for the host, only where it declares them.
                let linux = v == "linux" && text.contains("[[install.linux_names]]");
                assert!(
                    TEMPLATE_VARS.contains(&v) || linux,
                    "providers/{dir}: `${{{v}}}` in {line}"
                );
            }
        }
    }
}

/// 4. Every archive artifact has a checksum source.
#[test]
fn every_archive_artifact_has_a_checksum() {
    for (dir, text) in manifests() {
        let m = manifest::parse(&text).unwrap();
        for i in &m.install {
            if let InstallMethod::Archive { artifacts, .. } = i {
                for a in artifacts {
                    let ok = match &a.checksum {
                        Checksum::Sha256 { values } => !values.is_empty(),
                        Checksum::Sha256Url { url } => !url.is_empty(),
                        Checksum::Sha512 { values } => !values.is_empty(),
                    };
                    assert!(ok, "providers/{dir}: {} has no checksum source", a.url);
                }
            }
        }
    }
}

/// 4b. Nothing is fetched from a moving "latest" location: an artifact and its checksum file
/// name one release, so they cannot change between the two fetches or between machines
/// (AUD-107). A checksum file names the release too.
#[test]
fn no_artifact_or_checksum_tracks_latest() {
    for (dir, text) in manifests() {
        let m = manifest::parse(&text).unwrap();
        for i in &m.install {
            if let InstallMethod::Archive { artifacts, .. } = i {
                for a in artifacts {
                    let mut urls = vec![a.url.as_str()];
                    if let Checksum::Sha256Url { url } = &a.checksum {
                        urls.push(url);
                    }
                    for u in urls {
                        assert!(
                            !u.contains("/latest/") && !u.contains("/latest-"),
                            "providers/{dir}: {u} tracks the latest release"
                        );
                        assert!(
                            !u.starts_with("https://static.rust-lang.org/rustup/dist/"),
                            "providers/{dir}: {u} is rustup's moving copy"
                        );
                    }
                }
            }
        }
    }
}

/// 5. Detection runs on this host without error (found or not found), on this repository,
/// which is a Cargo workspace and so applies to the Rust provider at least.
#[tokio::test]
async fn detection_runs_on_this_host() {
    let r = Registry::load(&Sources {
        builtin: Builtin::Dir(providers_dir()),
        user_dir: None,
        workspace_dir: None,
        workspace_trusted: false,
    });
    assert_eq!(r.active().count(), manifests().len());
    let tc = tempfile::tempdir().unwrap();
    let host = HostPaths::from_env(tc.path().to_path_buf());
    let start = std::time::Instant::now();
    let d = axiom_env::detect(&r, &providers_dir().join(".."), &host).await;
    eprintln!("detection took {:?}: {d:#?}", start.elapsed());
    assert!(d.providers.iter().any(|p| p.provider_id == "rust"));
}

/// The compiled-in built-ins are exactly `providers/*`, byte for byte.
#[test]
fn embedded_builtins_match_the_providers_directory() {
    let embedded: Vec<(String, String)> = BUILTIN
        .iter()
        .map(|(n, t)| (n.to_string(), t.to_string()))
        .collect();
    assert_eq!(embedded, manifests());
    let r = Registry::load(&Sources {
        builtin: Builtin::Embedded,
        user_dir: None,
        workspace_dir: None,
        workspace_trusted: false,
    });
    assert!(r.infos().iter().all(|i| i.errors.is_empty()));
}
