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

// ---- `workspace/search` (protocol 0.4, M4-UX-001) ---------------------------------------------

/// What the Search page and quick open ask for.
#[derive(Debug, Clone, Default)]
pub struct FindQuery {
    pub query: String,
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
    /// Comma-separated globs a path must match.
    pub include: Option<String>,
    /// Comma-separated globs of paths to skip.
    pub exclude: Option<String>,
    /// List file paths instead of matching text.
    pub files: bool,
    pub limit: usize,
}

/// One match, with UTF-16 columns (Monaco's) and a preview trimmed around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindMatch {
    pub path: WorkspacePath,
    pub line: u32,
    pub column: u32,
    pub end_column: u32,
    pub preview: String,
    pub preview_column: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FindResult {
    pub matches: Vec<FindMatch>,
    pub files: Vec<WorkspacePath>,
    pub truncated: bool,
    pub files_searched: u32,
}

/// Default and largest number of results.
pub const FIND_DEFAULT_LIMIT: usize = 2000;
pub const FIND_MAX_LIMIT: usize = 20_000;
/// Characters of context kept before a match in its preview.
const PREVIEW_BEFORE: usize = 60;

impl Workspace {
    /// Text search or the file list, honouring `.gitignore`; `cancel` stops it between files.
    pub async fn find(
        &self,
        q: FindQuery,
        cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<FindResult, WsError> {
        let root = self.root().to_path_buf();
        tokio::task::spawn_blocking(move || find_blocking(&root, &q, &cancel))
            .await
            .map_err(|e| WsError::Io(std::io::Error::other(e.to_string())))?
    }
}

fn utf16_len(s: &str) -> u32 {
    s.encode_utf16().count() as u32
}

fn overrides(
    root: &std::path::Path,
    include: Option<&str>,
    exclude: Option<&str>,
) -> Result<Option<ignore::overrides::Override>, WsError> {
    let split = |s: Option<&str>| -> Vec<String> {
        s.unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|g| !g.is_empty())
            .map(str::to_string)
            .collect()
    };
    let (inc, exc) = (split(include), split(exclude));
    if inc.is_empty() && exc.is_empty() {
        return Ok(None);
    }
    let mut b = ignore::overrides::OverrideBuilder::new(root);
    for g in &inc {
        b.add(g)
            .map_err(|e| WsError::InvalidPath(format!("invalid glob {g}: {e}")))?;
    }
    if inc.is_empty() {
        // Exclusions alone: everything else is included.
        b.add("**")
            .map_err(|e| WsError::InvalidPath(e.to_string()))?;
    }
    for g in &exc {
        b.add(&format!("!{g}"))
            .map_err(|e| WsError::InvalidPath(format!("invalid glob {g}: {e}")))?;
    }
    b.build()
        .map(Some)
        .map_err(|e| WsError::InvalidPath(format!("invalid glob: {e}")))
}

fn find_blocking(
    root: &std::path::Path,
    q: &FindQuery,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<FindResult, WsError> {
    use std::sync::atomic::Ordering;
    // Text: 2 000 by default; the file list (quick open): everything up to the maximum.
    let limit = match (q.limit, q.files) {
        (0, true) => FIND_MAX_LIMIT,
        (0, false) => FIND_DEFAULT_LIMIT,
        (n, _) => n.min(FIND_MAX_LIMIT),
    };
    let re = if q.files {
        None
    } else {
        if q.query.is_empty() {
            return Ok(FindResult::default());
        }
        let mut pat = if q.regex {
            q.query.clone()
        } else {
            regex::escape(&q.query)
        };
        if q.whole_word {
            pat = format!(r"\b(?:{pat})\b");
        }
        Some(
            regex::RegexBuilder::new(&pat)
                .case_insensitive(!q.case_sensitive)
                .size_limit(1 << 20)
                .build()
                .map_err(|e| WsError::InvalidPath(format!("invalid pattern: {e}")))?,
        )
    };
    let filter = overrides(root, q.include.as_deref(), q.exclude.as_deref())?;
    let builder = || {
        let mut b = ignore::WalkBuilder::new(root);
        b.hidden(false)
            .git_global(false)
            .git_ignore(true)
            .git_exclude(true)
            .ignore(true)
            .require_git(false)
            .parents(false)
            .filter_entry(|e| e.file_name() != ".git");
        b
    };
    let rel_of = |p: &std::path::Path| -> Option<String> {
        let rel = p.strip_prefix(root).ok()?;
        if let Some(f) = &filter {
            if !f.matched(rel, false).is_whitelist() {
                return None;
            }
        }
        Some(
            rel.components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/"),
        )
    };
    let mut out = FindResult::default();
    let Some(re) = re else {
        // The file list: sequential (cheap), in path order.
        let mut b = builder();
        b.sort_by_file_path(|a, c| a.cmp(c));
        for entry in b.build() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let Ok(entry) = entry else { continue };
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            let Some(rel) = rel_of(entry.path()) else {
                continue;
            };
            if out.files.len() == limit {
                out.truncated = true;
                break;
            }
            out.files.push(WorkspacePath(rel));
            out.files_searched += 1;
        }
        return Ok(out);
    };
    // Text: files are read and matched on several threads; the results are put in path order
    // afterwards, so the answer does not depend on scheduling.
    // (matches per file, how many are held, whether any were dropped).
    #[allow(clippy::type_complexity)]
    let found: std::sync::Mutex<(Vec<(String, Vec<FindMatch>)>, usize, bool)> =
        std::sync::Mutex::new((Vec::new(), 0, false));
    let searched = std::sync::atomic::AtomicU32::new(0);
    builder().build_parallel().run(|| {
        Box::new(|entry| {
            use ignore::WalkState;
            if cancel.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }
            let Ok(entry) = entry else {
                return WalkState::Continue;
            };
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                return WalkState::Continue;
            }
            let Some(rel) = rel_of(entry.path()) else {
                return WalkState::Continue;
            };
            if entry.metadata().map(|m| m.len()).unwrap_or(u64::MAX) > MAX_FILE_BYTES {
                return WalkState::Continue;
            }
            let Ok(bytes) = std::fs::read(entry.path()) else {
                return WalkState::Continue;
            };
            if bytes[..bytes.len().min(8192)].contains(&0) {
                return WalkState::Continue;
            }
            let Ok(text) = String::from_utf8(bytes) else {
                return WalkState::Continue;
            };
            searched.fetch_add(1, Ordering::Relaxed);
            let Some((ms, more)) = file_matches(&text, &rel, &re, limit, cancel) else {
                return WalkState::Quit;
            };
            if !ms.is_empty() {
                let mut f = found.lock().unwrap_or_else(|p| p.into_inner());
                // The file alone had more than the limit: the answer is truncated.
                f.2 |= more;
                f.0.push((rel, ms));
                f.1 += f.0.last().map_or(0, |x| x.1.len());
                // Bounded memory, same answer whatever the scheduling: past twice the limit, keep
                // only the files that sort first (whole files) while they hold the limit.
                if f.1 > limit * 2 {
                    f.0.sort_by(|a, b| a.0.cmp(&b.0));
                    let mut kept = 0;
                    let mut cut = f.0.len();
                    for (i, (_, m)) in f.0.iter().enumerate() {
                        if kept >= limit {
                            cut = i;
                            break;
                        }
                        kept += m.len();
                    }
                    f.0.truncate(cut);
                    f.1 = kept;
                    f.2 = true;
                }
            }
            WalkState::Continue
        })
    });
    let (mut found, _, dropped) = found.into_inner().unwrap_or_else(|p| p.into_inner());
    found.sort_by(|a, b| a.0.cmp(&b.0));
    out.files_searched = searched.load(Ordering::Relaxed);
    out.truncated = dropped;
    for (_, ms) in found {
        for m in ms {
            if out.matches.len() == limit {
                out.truncated = true;
                return Ok(out);
            }
            out.matches.push(m);
        }
    }
    Ok(out)
}

/// How many lines or matches pass between two looks at the cancel flag.
const CANCEL_EVERY: usize = 1024;

/// A position in a line: bytes, characters and UTF-16 units from its start.
#[derive(Clone, Copy, Default)]
struct Cursor {
    byte: usize,
    utf16: u32,
}

impl Cursor {
    /// Moves forward to `to` (a character boundary at or after the cursor).
    fn advance(&mut self, line: &str, to: usize) {
        self.utf16 += utf16_len(&line[self.byte..to]);
        self.byte = to;
    }
}

/// The matches of one file, at most `limit` of them, in one forward pass over each line (the
/// columns and previews never rescan a line from its start). `Some((matches, more))`, where
/// `more` says the file had matches beyond the limit; `None` once `cancel` is set.
fn file_matches(
    text: &str,
    rel: &str,
    re: &regex::Regex,
    limit: usize,
    cancel: &std::sync::atomic::AtomicBool,
) -> Option<(Vec<FindMatch>, bool)> {
    use std::sync::atomic::Ordering;
    let mut ms = Vec::new();
    let mut ticks = 0usize;
    for (i, line) in text.lines().enumerate() {
        ticks += 1;
        if ticks.is_multiple_of(CANCEL_EVERY) && cancel.load(Ordering::Relaxed) {
            return None;
        }
        let mut at = Cursor::default();
        for m in re.find_iter(line) {
            if m.start() == m.end() {
                continue;
            }
            ticks += 1;
            if ticks.is_multiple_of(CANCEL_EVERY) && cancel.load(Ordering::Relaxed) {
                return None;
            }
            if ms.len() == limit {
                return Some((ms, true));
            }
            at.advance(line, m.start());
            let column = at.utf16 + 1;
            // The preview starts at most PREVIEW_BEFORE characters before the match: walk back
            // from the match, never from the start of the line.
            let start_byte = line[..m.start()]
                .char_indices()
                .rev()
                .take(PREVIEW_BEFORE)
                .last()
                .map_or(m.start(), |(b, _)| b);
            let preview: String = line[start_byte..].chars().take(MAX_LINE_CHARS).collect();
            let preview_column = utf16_len(&line[start_byte..m.start()]) + 1;
            at.advance(line, m.end());
            ms.push(FindMatch {
                path: WorkspacePath(rel.to_string()),
                line: i as u32 + 1,
                column,
                end_column: at.utf16 + 1,
                preview,
                preview_column,
            });
        }
    }
    Some((ms, false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// AUD-006: the cancel flag is honoured inside one big file, not only between files.
    #[test]
    fn a_file_scan_stops_when_cancelled() {
        let re = regex::Regex::new("b").unwrap();
        let text = "ab\n".repeat(10 * CANCEL_EVERY);
        let go = AtomicBool::new(false);
        let (ms, more) = file_matches(&text, "f", &re, 5, &go).unwrap();
        assert_eq!((ms.len(), more), (5, true));
        let stop = AtomicBool::new(true);
        assert!(file_matches(&text, "f", &re, usize::MAX, &stop).is_none());
    }
}
