//! Language servers in a folder the person has not trusted (AUD-008;
//! `docs/architecture/security.md` §Terminals and language servers).
//!
//! Opening a folder must not run its code. Several servers do by default: rust-analyzer builds
//! `build.rs` scripts and proc macros and runs `cargo check`, jdtls imports Gradle and Maven
//! builds, the TypeScript server loads the workspace's own `typescript`, SwiftPM evaluates
//! `Package.swift`, MSBuild evaluates project files. Until the folder is trusted a built-in
//! server is therefore one of:
//! - **as configured**: it reads and parses files and runs nothing of the project's;
//! - **restricted**: started with the features that would run project code turned off;
//! - **refused**: not started, because no setting reliably keeps it from running project code
//!   (rust-analyzer: a `rust-analyzer.toml`, `.cargo/config.toml` or `rust-toolchain.toml` in
//!   the repository can turn its build scripts back on or name the binaries it runs). It is
//!   `stopped` with a detail asking to trust the folder.
//!
//! Servers the person added (user and workspace providers) are their choice and run as
//! configured. Trusting the folder restarts every server as configured.
//!
//! Residual (review N8): restricted jdtls does not import Gradle or Maven builds, but jdtls has
//! no setting that turns annotation processing off, so an Eclipse-format project whose
//! `.settings/org.eclipse.jdt.apt.core.prefs` enables APT can still run the processor JARs its
//! `.factorypath` names.

use crate::server::{which, Definition, Launch};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// How a server runs in this folder.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Restriction {
    /// As configured: a trusted folder, or a server the person added.
    #[default]
    None,
    /// An untrusted folder, and a server that runs none of the project's code: as configured.
    Safe,
    /// Started with code-running features off; `note` says so (the server's detail).
    Restricted { note: String },
    /// Not started until the folder is trusted; `detail` says why (the server's detail).
    Refused { detail: String },
}

/// The start of a refused server's detail: what the shell can offer ("trust this folder").
pub const TRUST_TO_ENABLE: &str = "Trust this folder to enable";

/// How the built-in server `id` may run untrusted.
enum Policy {
    AsIs,
    Restricted,
    Refused(&'static str),
}

fn policy(id: &str) -> Policy {
    match id {
        // Read and parse only.
        "bash-language-server"
        | "clangd"
        | "docker-langserver"
        | "marksman"
        | "taplo"
        | "vscode-html"
        | "vscode-css"
        | "vscode-json"
        | "yaml-language-server" => Policy::AsIs,
        "jdtls" | "typescript-language-server" | "gopls" => Policy::Restricted,
        "rust-analyzer" => Policy::Refused("it builds the project's build scripts and macros"),
        "basedpyright" => Policy::Refused("it may run the project's Python interpreter"),
        "csharp-ls" => Policy::Refused("it evaluates the project's MSBuild files"),
        "dart" => Policy::Refused("it runs the project's analyzer plugins"),
        "kotlin-lsp" => Policy::Refused("it imports the project's Gradle build"),
        "sourcekit-lsp" => Policy::Refused("it runs the project's Package.swift"),
        // A built-in server not classified yet: the safe choice.
        _ => Policy::Refused("it may run the project's code"),
    }
}

/// Merges `b` over `a` (objects key by key).
fn merge(a: Value, b: Value) -> Value {
    match (a, b) {
        (Value::Object(mut a), Value::Object(b)) => {
            for (k, v) in b {
                let cur = a.remove(&k).unwrap_or(Value::Null);
                a.insert(k, merge(cur, v));
            }
            Value::Object(a)
        }
        (_, b) => b,
    }
}

/// The TypeScript the server was installed with (`<prefix>/node_modules/typescript`), never the
/// workspace's: found from the server's own program, outside `root`.
fn bundled_tsserver(exe: &Path, root: &Path) -> Option<PathBuf> {
    let exe = std::fs::canonicalize(exe).unwrap_or_else(|_| exe.to_path_buf());
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut dir = exe.parent();
    while let Some(d) = dir {
        if d.starts_with(&root) {
            return None;
        }
        for candidate in [
            d.join("node_modules/typescript/lib/tsserver.js"),
            d.join("lib/node_modules/typescript/lib/tsserver.js"),
        ] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        dir = d.parent();
    }
    None
}

/// `launch` for the built-in server `def` in an untrusted folder: its options and settings with
/// the code-running features off, or a refusal.
pub fn restrict(def: &Definition, mut launch: Launch) -> Launch {
    let refuse = |why: &str| {
        format!(
            "{TRUST_TO_ENABLE} {}: {why}, which could run code from this folder.",
            def.id
        )
    };
    match policy(&def.id) {
        Policy::AsIs => launch.restriction = Restriction::Safe,
        Policy::Refused(why) => {
            launch.restriction = Restriction::Refused {
                detail: refuse(why),
            };
        }
        Policy::Restricted => {
            let (options, settings, env, note): (Value, Value, Vec<(&str, &str)>, &str) =
                match def.id.as_str() {
                    "jdtls" => {
                        let java = json!({"java": {
                            "import": {
                                "gradle": {"enabled": false, "wrapper": {"enabled": false}},
                                "maven": {"enabled": false},
                            },
                            "configuration": {"updateBuildConfiguration": "disabled"},
                        }});
                        (
                            json!({"settings": java.clone()}),
                            java,
                            vec![],
                            "Restricted until you trust this folder: Gradle and Maven builds are not imported.",
                        )
                    }
                    "gopls" => (
                        Value::Null,
                        Value::Null,
                        // The go.mod `toolchain` line cannot fetch and run another Go.
                        vec![("GOTOOLCHAIN", "local")],
                        "Restricted until you trust this folder: the go command uses only the installed Go.",
                    ),
                    _ => {
                        // typescript-language-server: the TypeScript it was installed with.
                        let exe = def
                            .command
                            .first()
                            .and_then(|p| which(p, &launch.env));
                        let Some(tsserver) =
                            exe.and_then(|e| bundled_tsserver(&e, &launch.root))
                        else {
                            launch.restriction = Restriction::Refused {
                                detail: refuse("its own TypeScript was not found, and the folder's would be loaded"),
                            };
                            return launch;
                        };
                        (
                            json!({"tsserver": {"path": tsserver.to_string_lossy()}, "plugins": []}),
                            Value::Null,
                            vec![],
                            "Restricted until you trust this folder: the server's own TypeScript, not the folder's.",
                        )
                    }
                };
            if !options.is_null() {
                launch.initialization_options = Some(merge(
                    launch.initialization_options.take().unwrap_or(Value::Null),
                    options,
                ));
            }
            if !settings.is_null() {
                launch.settings = merge(std::mem::take(&mut launch.settings), settings);
            }
            for (k, v) in env {
                launch.env.insert(k.into(), v.into());
            }
            launch.restriction = Restriction::Restricted { note: note.into() };
        }
    }
    launch
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn def(id: &str, cmd: &str) -> Definition {
        Definition {
            id: id.into(),
            provider_id: "p".into(),
            languages: vec![],
            command: vec![cmd.into()],
            initialization_options: None,
            install_hint: None,
            requires: vec![],
        }
    }

    fn launch(root: &Path, path: &Path) -> Launch {
        Launch {
            root: root.to_path_buf(),
            env: BTreeMap::from([("PATH".to_string(), path.to_string_lossy().into_owned())]),
            settings: json!({"java": {"home": "/jdk"}}),
            initialization_options: Some(json!({"keep": 1})),
            blocked: None,
            install_provider: None,
            restriction: Restriction::None,
        }
    }

    #[test]
    fn untrusted_servers_are_restricted_or_refused() {
        let d = tempfile::tempdir().unwrap();
        let l = launch(d.path(), d.path());
        let ra = restrict(&def("rust-analyzer", "rust-analyzer"), l.clone());
        match &ra.restriction {
            Restriction::Refused { detail } => assert!(detail.starts_with(TRUST_TO_ENABLE)),
            other => panic!("{other:?}"),
        }
        assert_eq!(ra.initialization_options, l.initialization_options);
        let unknown = restrict(&def("new-server", "x"), l.clone());
        assert!(matches!(unknown.restriction, Restriction::Refused { .. }));
        let yaml = restrict(&def("yaml-language-server", "x"), l.clone());
        assert_eq!(yaml.restriction, Restriction::Safe);
        assert_eq!(
            Launch {
                restriction: Restriction::None,
                ..yaml
            },
            l
        );

        let j = restrict(&def("jdtls", "jdtls"), l.clone());
        assert!(matches!(j.restriction, Restriction::Restricted { .. }));
        let o = j.initialization_options.unwrap();
        assert_eq!(o["keep"], 1);
        assert_eq!(o["settings"]["java"]["import"]["gradle"]["enabled"], false);
        assert_eq!(o["settings"]["java"]["import"]["maven"]["enabled"], false);
        assert_eq!(j.settings["java"]["import"]["gradle"]["enabled"], false);
        assert_eq!(j.settings["java"]["home"], "/jdk");

        let g = restrict(&def("gopls", "gopls"), l.clone());
        assert_eq!(g.env["GOTOOLCHAIN"], "local");
    }

    #[cfg(unix)]
    #[test]
    fn typescript_uses_the_servers_own_typescript_or_none() {
        use std::os::unix::fs::PermissionsExt;
        let prefix = tempfile::tempdir().unwrap();
        let ws = tempfile::tempdir().unwrap();
        let bin = prefix.path().join("node_modules/.bin");
        std::fs::create_dir_all(&bin).unwrap();
        let exe = bin.join("typescript-language-server");
        std::fs::write(&exe, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let d = def("typescript-language-server", "typescript-language-server");
        // Without its own TypeScript it is refused (the folder's would load).
        let r = restrict(&d, launch(ws.path(), &bin));
        assert!(
            matches!(r.restriction, Restriction::Refused { .. }),
            "{r:?}"
        );
        let lib = prefix.path().join("node_modules/typescript/lib");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(lib.join("tsserver.js"), "").unwrap();
        let r = restrict(&d, launch(ws.path(), &bin));
        assert!(
            matches!(r.restriction, Restriction::Restricted { .. }),
            "{r:?}"
        );
        let path = r.initialization_options.unwrap()["tsserver"]["path"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            path.ends_with("node_modules/typescript/lib/tsserver.js"),
            "{path}"
        );
        assert!(!path.starts_with(&*ws.path().to_string_lossy()));
    }
}
