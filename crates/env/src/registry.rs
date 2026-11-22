//! The provider registry: built-in, user and workspace manifests (`providers.md` §Discovery).

use crate::manifest;
use axiom_protocol::env::{ProviderInfo, ProviderSource};
use axiom_protocol::provider::ProviderManifest;
use std::path::{Path, PathBuf};

/// The built-in providers, compiled in so the daemon needs no data files to find them. The
/// contract test checks that this list matches `providers/*/provider.toml` exactly.
pub const BUILTIN: &[(&str, &str)] = &[
    (
        "cmake",
        include_str!("../../../providers/cmake/provider.toml"),
    ),
    (
        "dart",
        include_str!("../../../providers/dart/provider.toml"),
    ),
    (
        "dotnet",
        include_str!("../../../providers/dotnet/provider.toml"),
    ),
    ("go", include_str!("../../../providers/go/provider.toml")),
    (
        "java",
        include_str!("../../../providers/java/provider.toml"),
    ),
    (
        "lsp-basedpyright",
        include_str!("../../../providers/lsp-basedpyright/provider.toml"),
    ),
    (
        "lsp-bash",
        include_str!("../../../providers/lsp-bash/provider.toml"),
    ),
    (
        "lsp-clangd",
        include_str!("../../../providers/lsp-clangd/provider.toml"),
    ),
    (
        "lsp-csharp",
        include_str!("../../../providers/lsp-csharp/provider.toml"),
    ),
    (
        "lsp-dart",
        include_str!("../../../providers/lsp-dart/provider.toml"),
    ),
    (
        "lsp-dockerfile",
        include_str!("../../../providers/lsp-dockerfile/provider.toml"),
    ),
    (
        "lsp-gopls",
        include_str!("../../../providers/lsp-gopls/provider.toml"),
    ),
    (
        "lsp-jdtls",
        include_str!("../../../providers/lsp-jdtls/provider.toml"),
    ),
    (
        "lsp-kotlin",
        include_str!("../../../providers/lsp-kotlin/provider.toml"),
    ),
    (
        "lsp-marksman",
        include_str!("../../../providers/lsp-marksman/provider.toml"),
    ),
    (
        "lsp-rust-analyzer",
        include_str!("../../../providers/lsp-rust-analyzer/provider.toml"),
    ),
    (
        "lsp-sourcekit",
        include_str!("../../../providers/lsp-sourcekit/provider.toml"),
    ),
    (
        "lsp-taplo",
        include_str!("../../../providers/lsp-taplo/provider.toml"),
    ),
    (
        "lsp-typescript",
        include_str!("../../../providers/lsp-typescript/provider.toml"),
    ),
    (
        "lsp-web",
        include_str!("../../../providers/lsp-web/provider.toml"),
    ),
    (
        "lsp-yaml",
        include_str!("../../../providers/lsp-yaml/provider.toml"),
    ),
    (
        "ninja",
        include_str!("../../../providers/ninja/provider.toml"),
    ),
    (
        "node",
        include_str!("../../../providers/node/provider.toml"),
    ),
    (
        "python",
        include_str!("../../../providers/python/provider.toml"),
    ),
    (
        "rust",
        include_str!("../../../providers/rust/provider.toml"),
    ),
];

/// Where built-in manifests come from.
#[derive(Debug, Clone)]
pub enum Builtin {
    /// The compiled-in [`BUILTIN`] set.
    Embedded,
    /// A directory laid out like `providers/` (tests, development).
    Dir(PathBuf),
}

/// The three manifest locations.
#[derive(Debug, Clone)]
pub struct Sources {
    pub builtin: Builtin,
    /// `<data dir>/providers`; absent or missing = no user providers.
    pub user_dir: Option<PathBuf>,
    /// `<workspace>/.axiom/providers`.
    pub workspace_dir: Option<PathBuf>,
    /// Whether the user trusts the workspace; until then its providers are listed, not loaded.
    pub workspace_trusted: bool,
}

/// One manifest found in one location.
#[derive(Debug, Clone)]
pub struct Entry {
    pub info: ProviderInfo,
    /// Directory holding `provider.toml` (`None` for embedded built-ins).
    pub dir: Option<PathBuf>,
    /// The validated manifest; `None` when it failed validation.
    pub manifest: Option<ProviderManifest>,
    /// Whether this entry is the effective provider for its id (valid, trusted, not
    /// overridden by a later source).
    pub active: bool,
}

/// The providers found, in source order (built-in, user, workspace), each sorted by id.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    entries: Vec<Entry>,
}

impl Registry {
    /// Reads every source. Never fails: unreadable or invalid manifests become entries with
    /// `errors`, and a missing directory contributes nothing.
    pub fn load(sources: &Sources) -> Registry {
        let mut entries = Vec::new();
        match &sources.builtin {
            Builtin::Embedded => {
                for (name, text) in BUILTIN {
                    entries.push(entry(
                        ProviderSource::Builtin,
                        name,
                        None,
                        Ok(text.to_string()),
                        true,
                    ));
                }
            }
            Builtin::Dir(d) => scan(d, ProviderSource::Builtin, true, &mut entries),
        }
        if let Some(d) = &sources.user_dir {
            scan(d, ProviderSource::User, true, &mut entries);
        }
        if let Some(d) = &sources.workspace_dir {
            scan(
                d,
                ProviderSource::Workspace,
                sources.workspace_trusted,
                &mut entries,
            );
        }
        // Later sources override earlier ones with the same id; only loadable entries count.
        let mut seen: Vec<String> = Vec::new();
        for e in entries.iter_mut().rev() {
            if e.manifest.is_some() && e.info.trusted && !seen.contains(&e.info.id) {
                e.active = true;
                seen.push(e.info.id.clone());
            }
        }
        Registry { entries }
    }

    /// Every entry, for `env/providers`.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The effective providers, in source order.
    pub fn active(&self) -> impl Iterator<Item = (&Entry, &ProviderManifest)> {
        self.entries
            .iter()
            .filter(|e| e.active)
            .filter_map(|e| e.manifest.as_ref().map(|m| (e, m)))
    }

    /// The effective provider with this id.
    pub fn get(&self, id: &str) -> Option<&ProviderManifest> {
        self.active().find(|(e, _)| e.info.id == id).map(|(_, m)| m)
    }

    /// `ProviderInfo` of every entry.
    pub fn infos(&self) -> Vec<ProviderInfo> {
        self.entries.iter().map(|e| e.info.clone()).collect()
    }
}

fn scan(dir: &Path, source: ProviderSource, trusted: bool, out: &mut Vec<Entry>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut dirs: Vec<PathBuf> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("provider.toml").is_file())
        .collect();
    dirs.sort();
    for d in dirs {
        let name = d
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let text = std::fs::read_to_string(d.join("provider.toml")).map_err(|e| e.to_string());
        out.push(entry(source, &name, Some(d), text, trusted));
    }
}

fn entry(
    source: ProviderSource,
    dir_name: &str,
    dir: Option<PathBuf>,
    text: Result<String, String>,
    trusted: bool,
) -> Entry {
    let parsed = match text {
        Ok(t) => manifest::parse(&t),
        Err(e) => Err(vec![format!("cannot read provider.toml: {e}")]),
    };
    match parsed {
        Ok(m) => Entry {
            info: ProviderInfo {
                id: m.id.clone(),
                name: m.name.clone(),
                description: m.description.clone(),
                version: m.version.clone(),
                source,
                trusted,
                errors: Vec::new(),
            },
            dir,
            manifest: Some(m),
            active: false,
        },
        Err(errors) => Entry {
            info: ProviderInfo {
                id: dir_name.to_string(),
                name: dir_name.to_string(),
                description: String::new(),
                version: String::new(),
                source,
                trusted,
                errors,
            },
            dir,
            manifest: None,
            active: false,
        },
    }
}
