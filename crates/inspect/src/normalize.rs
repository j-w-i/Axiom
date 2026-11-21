//! What a page reports, made safe and useful (ADR-0019 §4; `inspection.md` §Source candidates):
//! - source locations resolve to workspace-relative paths; anything outside the workspace is
//!   dropped, and files under `node_modules` are marked `generated`;
//! - confidences are capped at their method's ceiling;
//! - an element without its own stamp gets its nearest stamped ancestor as the `owner`, below
//!   0.6 (presented as uncertain);
//! - an element with no candidate at all and some text gets `heuristic` candidates from a
//!   bounded search of the workspace's source files (≤ 0.4);
//! - sizes are bounded (5000 nodes, 200 characters of text per node).

use axiom_protocol::diagnostics::Location;
use axiom_protocol::inspection::{
    HitTestResult, MappingMethod, NodeId, Snapshot, SourceCandidate, SourceRelation, UiNode,
};
use axiom_protocol::WorkspacePath;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

pub const MAX_NODES: usize = 5000;
pub const MAX_TEXT: usize = 200;
/// The owner relation's confidence: below 0.6, so it is never presented as exact.
pub const OWNER_CONFIDENCE: f32 = 0.5;
/// The heuristic search's best confidence.
pub const HEURISTIC_CONFIDENCE: f32 = 0.4;

const SOURCE_EXTENSIONS: &[&str] = &[
    "js", "jsx", "ts", "tsx", "mjs", "vue", "svelte", "html", "astro",
];
const SKIP_DIRS: &[&str] = &["node_modules", ".git", "dist", "build", "target", ".axiom"];
const MAX_SEARCH_FILES: usize = 2000;
const MAX_SEARCH_BYTES: u64 = 1024 * 1024;

pub struct Normalizer {
    root: PathBuf,
}

impl Normalizer {
    pub fn new(root: PathBuf) -> Self {
        let root = std::fs::canonicalize(&root).unwrap_or(root);
        Self { root }
    }

    /// A whole snapshot: bounded, sources resolved, owners from the tree (no text search: a
    /// snapshot is too big for it).
    pub fn snapshot(&self, mut s: Snapshot) -> Snapshot {
        if s.nodes.len() > MAX_NODES {
            s.nodes.truncate(MAX_NODES);
            s.truncated = true;
        }
        let nodes: Vec<UiNode> = s.nodes.into_iter().map(|n| self.own(n)).collect();
        let index: HashMap<NodeId, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        let owners: Vec<Option<Location>> = nodes
            .iter()
            .map(|n| {
                if has_own(n) {
                    return None;
                }
                let mut p = n.parent.as_ref().and_then(|id| index.get(id).copied());
                let mut guard = 0;
                while let Some(i) = p {
                    if let Some(l) = own_location(&nodes[i]) {
                        return Some(l);
                    }
                    p = nodes[i]
                        .parent
                        .as_ref()
                        .and_then(|id| index.get(id).copied());
                    guard += 1;
                    if guard > MAX_NODES {
                        break;
                    }
                }
                None
            })
            .collect();
        s.nodes = nodes
            .into_iter()
            .zip(owners)
            .map(|(mut n, o)| {
                if let Some(l) = o {
                    n.sources.push(owner(l));
                }
                n
            })
            .collect();
        s
    }

    /// A hit-test chain (node first, then its ancestors): owners from the chain, and a text
    /// search for the hit node when nothing else locates it.
    pub fn chain(&self, r: HitTestResult) -> HitTestResult {
        let mut chain: Vec<UiNode> = r.chain.into_iter().map(|n| self.own(n)).collect();
        for i in 0..chain.len() {
            if has_own(&chain[i]) {
                continue;
            }
            if let Some(l) = chain[i + 1..].iter().find_map(own_location) {
                chain[i].sources.push(owner(l));
            }
        }
        if let Some(first) = chain.first_mut() {
            if first.sources.is_empty() {
                first.sources = self.search(first.text.as_deref());
            }
        }
        HitTestResult { chain }
    }

    /// One node (details): its own sources, the given owner, else a text search.
    pub fn node(&self, n: UiNode, owner_location: Option<Location>) -> UiNode {
        let mut n = self.own(n);
        if !has_own(&n) {
            if let Some(l) = owner_location {
                n.sources.push(owner(l));
            }
        }
        if n.sources.is_empty() {
            n.sources = self.search(n.text.as_deref());
        }
        n
    }

    /// The node's own reported candidates, resolved and capped; text bounded.
    fn own(&self, mut n: UiNode) -> UiNode {
        if let Some(t) = &n.text {
            if t.chars().count() > MAX_TEXT {
                n.text = Some(t.chars().take(MAX_TEXT).collect());
            }
        }
        n.sources = n
            .sources
            .into_iter()
            .filter_map(|mut c| {
                let (file, generated) = self.resolve(&c.location.file.0)?;
                c.location.file = WorkspacePath(file);
                c.generated |= generated;
                c.confidence = c.confidence.clamp(0.0, c.method.max_confidence());
                Some(c)
            })
            .collect();
        n.sources
            .sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
        n
    }

    /// A reported file as a workspace path: relative to the root, or absolute inside it.
    /// `None` outside the workspace (or with `..`).
    pub fn resolve(&self, file: &str) -> Option<(String, bool)> {
        let file = file.strip_prefix("file://").unwrap_or(file);
        let p = Path::new(file);
        let abs = if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.root.join(p)
        };
        let mut clean = PathBuf::new();
        for c in abs.components() {
            match c {
                Component::ParentDir => return None,
                Component::CurDir => {}
                other => clean.push(other),
            }
        }
        let clean = std::fs::canonicalize(&clean).unwrap_or(clean);
        let rel = clean.strip_prefix(&self.root).ok()?;
        if rel.as_os_str().is_empty() {
            return None;
        }
        let parts: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let generated = parts.iter().any(|p| p == "node_modules");
        Some((parts.join("/"), generated))
    }

    /// Workspace source files that contain [text] verbatim, as heuristic candidates (at most
    /// three, 0.4 then lower). Empty for short or missing text.
    pub fn search(&self, text: Option<&str>) -> Vec<SourceCandidate> {
        let Some(text) = text.map(str::trim).filter(|t| t.chars().count() >= 3) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut seen = 0usize;
        let mut stack = vec![self.root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut entries: Vec<_> = entries.flatten().collect();
            entries.sort_by_key(|e| e.file_name());
            for e in entries {
                let path = e.path();
                let name = e.file_name().to_string_lossy().into_owned();
                let Ok(ft) = e.file_type() else { continue };
                if ft.is_dir() {
                    if !SKIP_DIRS.contains(&name.as_str()) && !name.starts_with('.') {
                        stack.push(path);
                    }
                    continue;
                }
                let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("");
                if !ft.is_file() || !SOURCE_EXTENSIONS.contains(&ext) {
                    continue;
                }
                seen += 1;
                if seen > MAX_SEARCH_FILES {
                    return out;
                }
                if e.metadata()
                    .map(|m| m.len() > MAX_SEARCH_BYTES)
                    .unwrap_or(true)
                {
                    continue;
                }
                let Ok(content) = std::fs::read_to_string(&path) else {
                    continue;
                };
                for (i, line) in content.lines().enumerate() {
                    if let Some(col) = line.find(text) {
                        let Some((file, generated)) = self.resolve(&path.to_string_lossy()) else {
                            break;
                        };
                        let confidence = HEURISTIC_CONFIDENCE - 0.05 * out.len() as f32;
                        out.push(SourceCandidate {
                            location: Location {
                                file: WorkspacePath(file),
                                line: i as u32 + 1,
                                column: Some(line[..col].chars().count() as u32 + 1),
                                end_line: None,
                                end_column: None,
                            },
                            relation: SourceRelation::Instantiation,
                            method: MappingMethod::Heuristic,
                            confidence,
                            generated,
                        });
                        if out.len() == 3 {
                            return out;
                        }
                        break;
                    }
                }
            }
        }
        out
    }
}

/// The node's own exact location (an instantiation stamp).
fn own_location(n: &UiNode) -> Option<Location> {
    n.sources
        .iter()
        .find(|c| {
            c.relation == SourceRelation::Instantiation
                && matches!(
                    c.method,
                    MappingMethod::BuildInstrumentation | MappingMethod::RuntimeMetadata
                )
                && !c.generated
        })
        .map(|c| c.location.clone())
}

fn has_own(n: &UiNode) -> bool {
    own_location(n).is_some()
}

fn owner(location: Location) -> SourceCandidate {
    SourceCandidate {
        location,
        relation: SourceRelation::Owner,
        method: MappingMethod::BuildInstrumentation,
        confidence: OWNER_CONFIDENCE,
        generated: false,
    }
}

/// A node normalised against [root] alone (for callers without a session).
pub fn normalize_node(root: &Path, n: UiNode) -> UiNode {
    Normalizer::new(root.to_path_buf()).node(n, None)
}
