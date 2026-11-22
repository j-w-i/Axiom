//! Per-directory ignore matching built from the `ignore` crate's
//! `Gitignore` matchers (`.gitignore`, `.ignore`, `.git/info/exclude`).
//! The user's global gitignore is deliberately not consulted.

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Most directories whose matchers are kept; past this the cache starts over (AUD-099: it held
/// one entry per directory ever classified, including every one under ignored trees).
pub(crate) const MAX_CACHED_DIRS: usize = 4096;

/// What an ignore file looked like when its matcher was built: `None` if absent, else its
/// modification time and length. A matcher whose files no longer look like this is rebuilt, so a
/// `.gitignore` changed outside the daemon (a `git checkout`) is seen without a watch.
type Stamp = Vec<Option<(std::time::SystemTime, u64)>>;

fn stamp(files: &[PathBuf]) -> Stamp {
    files
        .iter()
        .map(|f| {
            std::fs::metadata(f)
                .ok()
                .filter(|m| m.is_file())
                .map(|m| (m.modified().unwrap_or(std::time::UNIX_EPOCH), m.len()))
        })
        .collect()
}

#[derive(Default)]
pub(crate) struct IgnoreCache {
    /// Keyed by workspace-relative directory ("" = root).
    dirs: Mutex<HashMap<String, (Stamp, Arc<Gitignore>)>>,
    exclude: Mutex<Option<(Stamp, Arc<Gitignore>)>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn build(root: &Path, files: &[PathBuf]) -> Arc<Gitignore> {
    let mut b = GitignoreBuilder::new(root);
    // Later files take precedence; a missing file is simply skipped.
    for f in files {
        if f.is_file() {
            let _ = b.add(f);
        }
    }
    Arc::new(b.build().unwrap_or_else(|_| Gitignore::empty()))
}

impl IgnoreCache {
    fn dir_matcher(&self, ws_root: &Path, rel: &str) -> Arc<Gitignore> {
        let dir = if rel.is_empty() {
            ws_root.to_path_buf()
        } else {
            ws_root.join(rel)
        };
        // `.ignore` outranks `.gitignore`, as in ripgrep.
        let files = [dir.join(".gitignore"), dir.join(".ignore")];
        let now = stamp(&files);
        if let Some((st, m)) = lock(&self.dirs).get(rel) {
            if *st == now {
                return m.clone();
            }
        }
        let m = build(&dir, &files);
        let mut dirs = lock(&self.dirs);
        if dirs.len() >= MAX_CACHED_DIRS {
            dirs.clear();
        }
        dirs.insert(rel.to_string(), (now, m.clone()));
        m
    }

    fn exclude_matcher(&self, ws_root: &Path) -> Arc<Gitignore> {
        let files = [ws_root.join(".git").join("info").join("exclude")];
        let now = stamp(&files);
        if let Some((st, m)) = lock(&self.exclude).as_ref() {
            if *st == now {
                return m.clone();
            }
        }
        let m = build(ws_root, &files);
        *lock(&self.exclude) = Some((now, m.clone()));
        m
    }

    #[cfg(test)]
    pub(crate) fn cached_dirs(&self) -> usize {
        lock(&self.dirs).len()
    }

    /// Forgets what was learned about ignore files after a write touched one.
    /// `rel` is the workspace-relative path of the written file.
    pub(crate) fn invalidate_for_write(&self, rel: &str) {
        let (dir, name) = match rel.rsplit_once('/') {
            Some((d, n)) => (d, n),
            None => ("", rel),
        };
        if name == ".gitignore" || name == ".ignore" {
            lock(&self.dirs).remove(dir);
        }
        if rel == ".git/info/exclude" {
            *lock(&self.exclude) = None;
        }
    }

    /// Returns, for each entry of `names`, whether it is ignored. `dir_rel`
    /// is the listed directory; `entries` are `(name, is_dir)`.
    pub(crate) fn classify(
        &self,
        ws_root: &Path,
        dir_rel: &str,
        entries: &[(String, bool)],
    ) -> Vec<bool> {
        let comps: Vec<&str> = if dir_rel.is_empty() {
            vec![]
        } else {
            dir_rel.split('/').collect()
        };
        // chain[i] is the matcher for the directory made of the first i components.
        let mut chain: Vec<Arc<Gitignore>> = Vec::with_capacity(comps.len() + 1);
        let mut prefixes: Vec<String> = vec![String::new()];
        for c in &comps {
            let last = prefixes.last().cloned().unwrap_or_default();
            prefixes.push(if last.is_empty() {
                (*c).to_string()
            } else {
                format!("{last}/{c}")
            });
        }
        for p in &prefixes {
            chain.push(self.dir_matcher(ws_root, p));
        }
        let exclude = self.exclude_matcher(ws_root);

        // If any ancestor directory of the listed directory is ignored, so is
        // everything beneath it.
        for k in 1..prefixes.len() {
            let path = ws_root.join(&prefixes[k]);
            if decide(&chain[..k], &exclude, &path, true) {
                return vec![true; entries.len()];
            }
        }
        entries
            .iter()
            .map(|(name, is_dir)| {
                if name == ".git" {
                    return true;
                }
                let path = ws_root.join(dir_rel).join(name);
                decide(&chain, &exclude, &path, *is_dir)
            })
            .collect()
    }
}

/// Deepest matcher with an opinion wins; `.git/info/exclude` is the weakest.
fn decide(chain: &[Arc<Gitignore>], exclude: &Gitignore, path: &Path, is_dir: bool) -> bool {
    for m in chain.iter().rev() {
        let r = m.matched(path, is_dir);
        if r.is_ignore() {
            return true;
        }
        if r.is_whitelist() {
            return false;
        }
    }
    exclude.matched(path, is_dir).is_ignore()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AUD-099: classifying many distinct directories keeps the cache bounded.
    #[test]
    fn the_cache_stays_bounded() {
        let d = tempfile::tempdir().unwrap();
        let c = IgnoreCache::default();
        for i in 0..(MAX_CACHED_DIRS + 500) {
            c.classify(d.path(), &format!("d{i}"), &[("x".into(), false)]);
        }
        assert!(c.cached_dirs() <= MAX_CACHED_DIRS);
    }

    /// AUD-099: a `.gitignore` edited outside the daemon is seen without a watch.
    #[test]
    fn an_ignore_file_changed_behind_the_cache_is_reread() {
        let d = tempfile::tempdir().unwrap();
        let c = IgnoreCache::default();
        let probe = [("a.log".to_string(), false)];
        assert_eq!(c.classify(d.path(), "", &probe), vec![false]);
        std::fs::write(d.path().join(".gitignore"), "*.log\n").unwrap();
        assert_eq!(c.classify(d.path(), "", &probe), vec![true]);
        std::fs::write(d.path().join(".gitignore"), "*.tmp\n*.bak\n").unwrap();
        assert_eq!(c.classify(d.path(), "", &probe), vec![false]);
    }
}
