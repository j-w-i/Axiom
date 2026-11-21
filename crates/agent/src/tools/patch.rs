//! Search/replace edits for `fs.applyPatch` (pure). Each search block must match exactly once in
//! the text as it is when that edit applies (edits apply in order). A failure says which edit
//! failed and, for a miss, where the closest region is, so the model can correct itself.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Edit {
    /// Exact text to find (whitespace and indentation included). Must occur exactly once.
    pub search: String,
    /// Replacement text.
    pub replace: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PatchError {
    #[error("edit {index}: the search text is empty")]
    Empty { index: usize },
    #[error("edit {index}: the search text was not found")]
    NoMatch {
        index: usize,
        /// The most similar region, 1-based start line and the text.
        closest: Option<(usize, String)>,
    },
    #[error("edit {index}: the search text occurs {count} times (lines {lines:?}); include more context so it is unique")]
    Ambiguous {
        index: usize,
        count: usize,
        lines: Vec<usize>,
    },
}

impl PatchError {
    /// A one-line hint for the model.
    pub fn hint(&self) -> String {
        match self {
            PatchError::Empty { .. } => "Give the exact existing text to replace.".into(),
            PatchError::NoMatch {
                closest: Some((line, text)),
                ..
            } => format!(
                "Re-read the file; the closest text starts at line {line}:\n{text}\nCopy the search text exactly, including indentation."
            ),
            PatchError::NoMatch { closest: None, .. } => {
                "Re-read the file with fs.read and copy the search text exactly.".into()
            }
            PatchError::Ambiguous { .. } => {
                "Add surrounding lines to the search text so it matches once.".into()
            }
        }
    }
}

/// Applies `edits` in order. CRLF files accept LF search/replace text.
pub fn apply(text: &str, edits: &[Edit]) -> Result<String, PatchError> {
    let crlf = text.contains("\r\n");
    let mut out = text.to_string();
    for (i, e) in edits.iter().enumerate() {
        let index = i + 1;
        if e.search.is_empty() {
            return Err(PatchError::Empty { index });
        }
        let (search, replace) = if crlf && !e.search.contains("\r\n") {
            (
                e.search.replace('\n', "\r\n"),
                e.replace.replace('\n', "\r\n"),
            )
        } else {
            (e.search.clone(), e.replace.clone())
        };
        let found: Vec<usize> = out.match_indices(&search).map(|(p, _)| p).collect();
        match found.len() {
            1 => {
                let p = found[0];
                out.replace_range(p..p + search.len(), &replace);
            }
            0 => {
                return Err(PatchError::NoMatch {
                    index,
                    closest: closest(&out, &e.search),
                })
            }
            n => {
                return Err(PatchError::Ambiguous {
                    index,
                    count: n,
                    lines: found
                        .iter()
                        .take(5)
                        .map(|p| out[..*p].matches('\n').count() + 1)
                        .collect(),
                })
            }
        }
    }
    Ok(out)
}

/// The region of `text` most like `search`: the window of as many lines, scored by how many of
/// its trimmed lines equal one of the search's trimmed lines; ties prefer a window whose first
/// line is the search's first line, then the earliest.
fn closest(text: &str, search: &str) -> Option<(usize, String)> {
    let want: Vec<&str> = search
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if want.is_empty() {
        return None;
    }
    let lines: Vec<&str> = text.lines().collect();
    let n = search.lines().count().max(1);
    let mut best: Option<((usize, bool), usize)> = None; // ((score, starts aligned), start)
    for start in 0..lines.len() {
        let window = &lines[start..(start + n).min(lines.len())];
        let count = window
            .iter()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .filter(|l| want.contains(l))
            .count();
        let score = (count, lines[start].trim() == want[0]);
        if count > 0 && best.is_none_or(|(s, _)| score > s) {
            best = Some((score, start));
        }
    }
    let (_, start) = best?;
    let end = (start + n).min(lines.len());
    Some((start + 1, lines[start..end].join("\n")))
}

/// A unified diff of `old` → `new` for `path`, and the added/removed line counts.
pub fn diff(path: &str, old: &str, new: &str) -> (String, usize, usize) {
    let d = similar::TextDiff::from_lines(old, new);
    let mut added = 0;
    let mut removed = 0;
    for c in d.iter_all_changes() {
        match c.tag() {
            similar::ChangeTag::Insert => added += 1,
            similar::ChangeTag::Delete => removed += 1,
            similar::ChangeTag::Equal => {}
        }
    }
    let text = d
        .unified_diff()
        .context_radius(3)
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .to_string();
    (text, added, removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(s: &str, r: &str) -> Edit {
        Edit {
            search: s.into(),
            replace: r.into(),
        }
    }

    #[test]
    fn patch_applies_edits_in_order() {
        let t = "fn a() {}\nfn b() {}\n";
        let out = apply(t, &[e("fn a() {}", "fn a() { 1 }"), e("fn b", "pub fn b")]).unwrap();
        assert_eq!(out, "fn a() { 1 }\npub fn b() {}\n");
    }

    #[test]
    fn patch_refuses_zero_and_many_matches() {
        let t = "x\ny\nx\n";
        assert_eq!(
            apply(t, &[e("x", "z")]),
            Err(PatchError::Ambiguous {
                index: 1,
                count: 2,
                lines: vec![1, 3]
            })
        );
        let err = apply("fn main() {\n    let a = 1;\n}\n", &[e("let a = 2;", "")]).unwrap_err();
        assert!(matches!(err, PatchError::NoMatch { index: 1, .. }));
        assert_eq!(apply(t, &[e("", "q")]), Err(PatchError::Empty { index: 1 }));
    }

    #[test]
    fn patch_hint_points_at_the_closest_region() {
        let t = "fn main() {\n    let total = add(1, 2);\n    println!(\"{total}\");\n}\n";
        let err = apply(
            t,
            &[e("let total = add(1, 2);\nprintln!(\"{}\", total);", "")],
        )
        .unwrap_err();
        let PatchError::NoMatch {
            closest: Some((line, text)),
            ..
        } = &err
        else {
            panic!("{err:?}")
        };
        assert_eq!(*line, 2);
        assert!(text.contains("let total"));
        assert!(err.hint().contains("line 2"));
    }

    #[test]
    fn patch_keeps_crlf() {
        let out = apply("a\r\nb\r\n", &[e("a\nb", "a\nc")]).unwrap();
        assert_eq!(out, "a\r\nc\r\n");
    }

    #[test]
    fn diff_counts_and_headers() {
        let (d, a, r) = diff("src/x.rs", "a\nb\n", "a\nc\nd\n");
        assert_eq!((a, r), (2, 1));
        assert!(d.starts_with("--- a/src/x.rs\n+++ b/src/x.rs\n"));
        assert!(d.contains("-b\n+c\n+d\n"));
    }
}
