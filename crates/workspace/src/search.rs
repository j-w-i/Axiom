//! Text search over the workspace (M3-TOOLS-001, the `workspace.search` tool): every file the
//! ignore rules keep (`.gitignore`, `.ignore`, `.git/info/exclude`; not the user's global
//! ignores), skipping `.git`, binary files (a NUL in the first 8 KiB) and files over 2 MiB.
//! Results are in path order, then line order, and paged by an offset cursor.

use crate::error::WsError;
use crate::workspace::Workspace;
use axiom_protocol::WorkspacePath;

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_LINE_CHARS: usize = 300;

#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub pattern: String,
    /// Treat `pattern` as a regular expression (else literal).
    pub regex: bool,
    pub case_sensitive: bool,
    /// Only paths matching this glob (workspace-relative, `**` allowed).
    pub glob: Option<String>,
    /// Skip this many matches (the cursor).
    pub offset: usize,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    pub path: WorkspacePath,
    /// 1-based.
    pub line: u32,
    /// 1-based, in characters.
    pub column: u32,
    /// The line, trimmed to 300 characters.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub matches: Vec<SearchMatch>,
    /// Offset of the next page, if there are more matches.
    pub next_offset: Option<usize>,
    pub files_searched: usize,
}

impl Workspace {
    pub async fn search(&self, q: SearchQuery) -> Result<SearchResult, WsError> {
        let root = self.root().to_path_buf();
        tokio::task::spawn_blocking(move || search_blocking(&root, &q))
            .await
            .map_err(|e| WsError::Io(std::io::Error::other(e.to_string())))?
    }
}

fn search_blocking(root: &std::path::Path, q: &SearchQuery) -> Result<SearchResult, WsError> {
    let pat = if q.regex {
        q.pattern.clone()
    } else {
        regex::escape(&q.pattern)
    };
    let re = regex::RegexBuilder::new(&pat)
        .case_insensitive(!q.case_sensitive)
        .size_limit(1 << 20)
        .build()
        .map_err(|e| WsError::InvalidPath(format!("invalid pattern: {e}")))?;
    let glob = match &q.glob {
        Some(g) => Some(
            ignore::overrides::OverrideBuilder::new(root)
                .add(g)
                .and_then(|b| b.build())
                .map_err(|e| WsError::InvalidPath(format!("invalid glob: {e}")))?,
        ),
        None => None,
    };
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_global(false)
        .git_ignore(true)
        .git_exclude(true)
        .ignore(true)
        .require_git(false)
        .parents(false)
        .sort_by_file_path(|a, b| a.cmp(b))
        .filter_entry(|e| e.file_name() != ".git")
        .build();
    let mut matches = Vec::new();
    let mut seen = 0usize;
    let mut files = 0usize;
    let mut more = false;
    'files: for entry in walker {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(root) else {
            continue;
        };
        if let Some(g) = &glob {
            if !g.matched(rel, false).is_whitelist() {
                continue;
            }
        }
        if entry.metadata().map(|m| m.len()).unwrap_or(u64::MAX) > MAX_FILE_BYTES {
            continue;
        }
        let Ok(bytes) = std::fs::read(entry.path()) else {
            continue;
        };
        if bytes[..bytes.len().min(8192)].contains(&0) {
            continue;
        }
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        files += 1;
        let rel = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        for (i, line) in text.lines().enumerate() {
            if let Some(m) = re.find(line) {
                if seen >= q.offset {
                    if matches.len() == q.limit {
                        more = true;
                        break 'files;
                    }
                    let column = line[..m.start()].chars().count() as u32 + 1;
                    let text: String = line.chars().take(MAX_LINE_CHARS).collect();
                    matches.push(SearchMatch {
                        path: WorkspacePath(rel.clone()),
                        line: i as u32 + 1,
                        column,
                        text,
                    });
                }
                seen += 1;
            }
        }
    }
    Ok(SearchResult {
        next_offset: more.then_some(q.offset + matches.len()),
        matches,
        files_searched: files,
    })
}
