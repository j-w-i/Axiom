//! Changing text: by replacing, and by a patch of several files. Nothing here touches a file.

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    #[error("the text to replace is empty")]
    Empty,
    #[error("the new text is the same as the old")]
    Same,
    #[error("the text to replace is not in the file{0}")]
    NotFound(String),
    #[error(
        "the text to replace is in the file {0} times; give more of what is around it, or say all"
    )]
    Many(usize),
    #[error("line {0} of the patch: {1}")]
    Patch(usize, &'static str),
    #[error("line {line} of the patch: what it should change is not in {path}{hint}")]
    Hunk { line: usize, path: String, hint: String },
}

/// `old` for `new`. What ends lines in the file does not have to be known to who edits it.
pub fn replace(text: &str, old: &str, new: &str, all: bool) -> Result<String, EditError> {
    if old.is_empty() {
        return Err(EditError::Empty);
    }
    let crlf = text.contains("\r\n");
    let (old, new) =
        if crlf { (to_crlf(old), to_crlf(new)) } else { (old.to_owned(), new.to_owned()) };
    if old == new {
        return Err(EditError::Same);
    }
    match text.matches(&old).count() {
        0 => Err(EditError::NotFound(hint(text, &old))),
        1 => Ok(text.replacen(&old, &new, 1)),
        _ if all => Ok(text.replace(&old, &new)),
        n => Err(EditError::Many(n)),
    }
}

fn to_crlf(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\n', "\r\n")
}

/// What may have gone wrong, for who tries again.
fn hint(text: &str, old: &str) -> String {
    let bare =
        |s: &str| s.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n");
    let (t, o) = (bare(text), bare(old));
    if !o.is_empty() && t.contains(&o) {
        "; it is there with other spaces or indentation".into()
    } else {
        String::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilePatch {
    Add { path: String, text: String },
    Remove { path: String },
    Update { path: String, to: Option<String>, hunks: Vec<Hunk> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// A line of the file to look behind.
    pub after: Option<String>,
    pub old: Vec<String>,
    pub new: Vec<String>,
    /// Whether it must be at the file's end.
    pub end: bool,
    line: usize,
}

/// Reads a patch: `*** Begin Patch`, then for each file `*** Add File: p`, `*** Delete File: p` or
/// `*** Update File: p` (with `*** Move to: q`), and `*** End Patch`.
pub fn parse_patch(s: &str) -> Result<Vec<FilePatch>, EditError> {
    let bad = |n: usize, why| Err(EditError::Patch(n + 1, why));
    let lines: Vec<&str> = s.trim_matches(['\n', '\r', ' ']).lines().collect();
    let mut files = Vec::new();
    if lines.first().map(|l| l.trim_end()) != Some("*** Begin Patch") {
        return bad(0, "a patch begins with *** Begin Patch");
    }
    if lines.last().map(|l| l.trim_end()) != Some("*** End Patch") {
        return bad(lines.len().saturating_sub(1), "a patch ends with *** End Patch");
    }
    let body = &lines[..lines.len() - 1];
    let mut i = 1;
    while i < body.len() {
        let l = body[i];
        if l.trim().is_empty() {
            i += 1;
            continue;
        }
        let path = |p: &str| p.trim().to_owned();
        if let Some(p) = l.strip_prefix("*** Add File: ") {
            let mut text = String::new();
            i += 1;
            while let Some(x) = body.get(i).filter(|x| !x.starts_with("*** ")) {
                let Some(x) = x.strip_prefix('+') else {
                    return bad(i, "every line of a new file begins with +");
                };
                text.push_str(x);
                text.push('\n');
                i += 1;
            }
            files.push(FilePatch::Add { path: path(p), text });
        } else if let Some(p) = l.strip_prefix("*** Delete File: ") {
            files.push(FilePatch::Remove { path: path(p) });
            i += 1;
        } else if let Some(p) = l.strip_prefix("*** Update File: ") {
            i += 1;
            let to = body.get(i).and_then(|x| x.strip_prefix("*** Move to: ")).map(path);
            if to.is_some() {
                i += 1;
            }
            let mut hunks: Vec<Hunk> = Vec::new();
            while let Some(x) = body.get(i) {
                if x.starts_with("*** End of File") {
                    match hunks.last_mut() {
                        Some(h) => h.end = true,
                        None => return bad(i, "a file's end without a change before it"),
                    }
                    i += 1;
                    continue;
                }
                if x.starts_with("*** ") {
                    break;
                }
                let empty = |after: Option<String>| Hunk {
                    after,
                    old: vec![],
                    new: vec![],
                    end: false,
                    line: i + 1,
                };
                if let Some(a) = x.strip_prefix("@@") {
                    let a = a.trim();
                    // A line-number header says nothing that can be relied on.
                    let a = (!a.is_empty() && !a.starts_with('-')).then(|| a.to_owned());
                    hunks.push(empty(a));
                    i += 1;
                    continue;
                }
                if hunks.is_empty() {
                    hunks.push(empty(None));
                }
                let Some(h) = hunks.last_mut() else { break };
                match x.chars().next() {
                    Some(' ') => {
                        h.old.push(x[1..].into());
                        h.new.push(x[1..].into());
                    }
                    // Many leave the space of an empty line out.
                    None => {
                        h.old.push(String::new());
                        h.new.push(String::new());
                    }
                    Some('-') => h.old.push(x[1..].into()),
                    Some('+') => h.new.push(x[1..].into()),
                    _ => return bad(i, "a line begins with a space, - or +"),
                }
                i += 1;
            }
            if to.is_none() && hunks.iter().all(|h| h.old == h.new) {
                return bad(i.saturating_sub(1), "the file is not changed");
            }
            files.push(FilePatch::Update { path: path(p), to, hunks });
        } else {
            return bad(i, "expected *** Add File, *** Update File or *** Delete File");
        }
    }
    if files.is_empty() {
        return bad(0, "the patch changes nothing");
    }
    Ok(files)
}

/// The text after `hunks`, each found after the one before.
pub fn apply(text: &str, hunks: &[Hunk], path: &str) -> Result<String, EditError> {
    let crlf = text.contains("\r\n");
    let ends = text.is_empty() || text.ends_with('\n');
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut at = 0;
    for h in hunks {
        let lost = || EditError::Hunk {
            line: h.line,
            path: path.into(),
            hint: hint(text, &h.old.join("\n")),
        };
        let mut from = at;
        if let Some(a) = &h.after {
            from = (at..lines.len()).find(|i| lines[*i].trim() == a.trim()).ok_or_else(lost)? + 1;
        }
        let (mut old, mut new) = (h.old.as_slice(), h.new.as_slice());
        let pos = if old.is_empty() {
            // Only added: behind the line it names, or at the end.
            if h.after.is_some() { from } else { lines.len() }
        } else {
            loop {
                if let Some(p) = seek(&lines, old, from, h.end) {
                    break p;
                }
                // Empty lines after a hunk are often only the patch's own.
                match (old, new) {
                    ([o @ .., x], [n @ .., y]) if x.is_empty() && y.is_empty() && !o.is_empty() => {
                        (old, new) = (o, n);
                    }
                    _ => return Err(lost()),
                }
            }
        };
        out.extend(lines[at..pos].iter().map(|s| (*s).to_owned()));
        out.extend(new.iter().cloned());
        at = pos + old.len();
    }
    out.extend(lines[at..].iter().map(|s| (*s).to_owned()));
    let nl = if crlf { "\r\n" } else { "\n" };
    let mut s = out.join(nl);
    if ends && !out.is_empty() {
        s.push_str(nl);
    }
    Ok(s)
}

/// Where `old` is: exactly, or else with other spaces at the ends of lines.
fn seek(lines: &[&str], old: &[String], from: usize, end: bool) -> Option<usize> {
    let last = lines.len().checked_sub(old.len())?;
    let fits = |same: fn(&str, &str) -> bool| {
        (from..=last)
            .find(|i| (!end || *i == last) && lines[*i..].iter().zip(old).all(|(a, b)| same(a, b)))
    };
    fits(|a, b| a == b).or_else(|| fits(|a, b| a.trim_end() == b.trim_end()))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn replaces_what_is_there_once() {
        assert_eq!(replace("a b c", "b", "x", false).unwrap(), "a x c");
        assert_eq!(replace("a b b", "b", "x", false), Err(EditError::Many(2)));
        assert_eq!(replace("a b b", "b", "x", true).unwrap(), "a x x");
        assert_eq!(replace("a", "b", "x", true), Err(EditError::NotFound(String::new())));
        assert_eq!(replace("a", "", "x", true), Err(EditError::Empty));
        assert_eq!(replace("a", "a", "a", true), Err(EditError::Same));
        assert_eq!(replace("a\r\nb\r\n", "a\nb", "x\ny", false).unwrap(), "x\r\ny\r\n");
        let e = replace("  if x {\n    y\n  }", "if x {\n  y\n}", "z", false).unwrap_err();
        assert!(e.to_string().contains("indentation"), "{e}");
    }

    fn one(s: &str) -> FilePatch {
        parse_patch(&format!("*** Begin Patch\n{s}\n*** End Patch")).unwrap().remove(0)
    }
    fn update(text: &str, s: &str) -> Result<String, EditError> {
        let FilePatch::Update { hunks, .. } = one(&format!("*** Update File: f\n{s}")) else {
            panic!()
        };
        apply(text, &hunks, "f")
    }

    #[test]
    fn reads_a_patch() {
        let p = parse_patch(
            "*** Begin Patch\n*** Add File: a\n+x\n+\n*** Delete File: b\n*** Update File: c\n*** Move to: d\n@@ fn f\n a\n-b\n+c\n*** End of File\n*** End Patch\n",
        )
        .unwrap();
        assert_eq!(p[0], FilePatch::Add { path: "a".into(), text: "x\n\n".into() });
        assert_eq!(p[1], FilePatch::Remove { path: "b".into() });
        let FilePatch::Update { path, to, hunks } = &p[2] else { panic!() };
        assert_eq!((path.as_str(), to.as_deref()), ("c", Some("d")));
        assert_eq!((hunks[0].after.as_deref(), hunks[0].end), (Some("fn f"), true));
        for (s, n) in [
            ("", 1),
            ("*** Begin Patch", 1),
            ("*** Begin Patch\n*** End Patch", 1),
            ("*** Begin Patch\nx\n*** End Patch", 2),
            ("*** Begin Patch\n*** Add File: a\nx\n*** End Patch", 3),
            ("*** Begin Patch\n*** Update File: a\n*x\n*** End Patch", 3),
            ("*** Begin Patch\n*** Update File: a\n b\n*** End Patch", 3),
            ("*** Begin Patch\n*** Update File: a\n*** End of File\n*** End Patch", 3),
            ("x\n*** Begin Patch\n*** Delete File: a\n*** End Patch", 1),
        ] {
            match parse_patch(s) {
                Err(EditError::Patch(l, _)) => assert_eq!(l, n, "{s:?}"),
                x => panic!("{s:?}: {x:?}"),
            }
        }
    }

    #[test]
    fn applies_in_order() {
        let t = "fn a\n  x\nfn b\n  x\nend\n";
        assert_eq!(update(t, "@@ fn b\n-  x\n+  y").unwrap(), "fn a\n  x\nfn b\n  y\nend\n");
        assert_eq!(update(t, "-  x\n+  1\n@@\n-  x\n+  2").unwrap(), "fn a\n  1\nfn b\n  2\nend\n");
        assert_eq!(update(t, "@@\n+last").unwrap(), format!("{t}last\n"));
        assert_eq!(update(t, "@@ fn a\n+  0").unwrap(), "fn a\n  0\n  x\nfn b\n  x\nend\n");
        assert_eq!(update(t, "@@ -1,2 +1,2 @@\n-fn a\n+fn z").unwrap(), t.replace("fn a", "fn z"));
        assert_eq!(update("a\nx\nx", "-x\n+y\n*** End of File").unwrap(), "a\nx\ny");
        assert_eq!(update("a  \nb\n", " a\n-b\n+c").unwrap(), "a\nc\n");
        assert_eq!(update("a\n\nb\n", " a\n\n-b\n+c").unwrap(), "a\n\nc\n");
        assert_eq!(update("a\nb\n", " a\n-b\n+c\n\n").unwrap(), "a\nc\n");
        assert_eq!(update("a\r\nb\r\n", " a\n-b\n+c").unwrap(), "a\r\nc\r\n");
        assert_eq!(update("a\nb", "-b\n+c").unwrap(), "a\nc", "no end of line is not added");
        assert_eq!(update("a\n", "-a").unwrap(), "");
        for s in ["-zz\n+y", "@@ fn c\n-  x\n+y", "-end\n+e\n@@\n-fn a\n+f"] {
            assert!(matches!(update(t, s), Err(EditError::Hunk { .. })), "{s}");
        }
    }

    proptest! {
        /// A patch made from two texts makes the second of the first.
        #[test]
        fn a_patch_does_what_it_says(a in prop::collection::vec("[ab]{0,2}", 1..8), b in prop::collection::vec("[ab]{0,2}", 1..8)) {
            let (ta, tb) = (a.join("\n") + "\n", b.join("\n") + "\n");
            prop_assume!(ta != tb);
            let mut s = String::new();
            for l in &a { s += &format!("-{l}\n"); }
            for l in &b { s += &format!("+{l}\n"); }
            prop_assert_eq!(update(&ta, s.trim_end_matches('\n')).ok(), Some(tb));
        }

        #[test]
        fn nothing_makes_it_panic(s in "(\\*\\*\\* (Begin Patch|End Patch|Add File: a|Update File: a|Delete File: a|Move to: b|End of File)\n|@@ ?a?\n|[ +-]?[ab]?\n){0,12}", t in "[ab\n]{0,10}") {
            if let Ok(p) = parse_patch(&s) {
                for f in p {
                    if let FilePatch::Update { hunks, .. } = f {
                        let _ = apply(&t, &hunks, "f");
                    }
                }
            }
        }
    }
}
