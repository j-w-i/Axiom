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

/// `old` for `new`. What ends lines in the file does not have to be known to who edits it, and stays
/// as it is in every line that is not replaced.
pub fn replace(text: &str, old: &str, new: &str, all: bool) -> Result<String, EditError> {
    if old.is_empty() {
        return Err(EditError::Empty);
    }
    if !text.contains("\r\n") {
        return replace_in(text, &[], old, new, all);
    }
    // Looked for without the returns; `at` is where they were.
    let bare = text.replace("\r\n", "\n");
    let mut at = Vec::new();
    for (n, (i, _)) in text.match_indices("\r\n").enumerate() {
        at.push(i - n);
    }
    replace_in(&bare, &at, &old.replace("\r\n", "\n"), &new.replace("\r\n", "\n"), all)
}

/// In `text`, with returns before the ends of lines that `returns` names.
fn replace_in(
    text: &str,
    returns: &[usize],
    old: &str,
    new: &str,
    all: bool,
) -> Result<String, EditError> {
    if old == new {
        return Err(EditError::Same);
    }
    let found: Vec<usize> = text.match_indices(old).map(|(i, _)| i).collect();
    match found.len() {
        0 => return Err(EditError::NotFound(hint(text, old))),
        1 => {}
        _ if all => {}
        n => return Err(EditError::Many(n)),
    }
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    let keep = |out: &mut String, from: usize, to: usize| {
        let mut last = from;
        let (a, b) = (returns.partition_point(|r| *r < from), returns.partition_point(|r| *r < to));
        for r in &returns[a..b] {
            out.push_str(&text[last..*r]);
            out.push('\r');
            last = *r;
        }
        out.push_str(&text[last..to]);
    };
    for at in found {
        keep(&mut out, from, at);
        // As the line ends that is replaced, or the one before the end of the file.
        let end = text[at..].find('\n').map(|i| at + i).or_else(|| text.rfind('\n'));
        if end.is_some_and(|e| returns.binary_search(&e).is_ok()) {
            out.push_str(&new.replace('\n', "\r\n"));
        } else {
            out.push_str(new);
        }
        from = at + old.len();
    }
    keep(&mut out, from, text.len());
    Ok(out)
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
    pub lines: Vec<(Op, String)>,
    /// Whether it must be at the file's end.
    pub end: bool,
    line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Keep,
    Remove,
    Add,
}

impl Hunk {
    fn without(&self, op: Op) -> impl Iterator<Item = &str> {
        self.lines.iter().filter(move |l| l.0 != op).map(|l| l.1.as_str())
    }
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
                let empty =
                    |after: Option<String>| Hunk { after, lines: vec![], end: false, line: i + 1 };
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
                h.lines.push(match x.chars().next() {
                    Some(' ') => (Op::Keep, x[1..].into()),
                    // Many leave the space of an empty line out.
                    None => (Op::Keep, String::new()),
                    Some('-') => (Op::Remove, x[1..].into()),
                    Some('+') => (Op::Add, x[1..].into()),
                    _ => return bad(i, "a line begins with a space, - or +"),
                });
                i += 1;
            }
            if to.is_none() && hunks.iter().all(|h| h.without(Op::Add).eq(h.without(Op::Remove))) {
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

/// A line and what ends it.
fn split(l: &str) -> (&str, &str) {
    let bare = l.strip_suffix('\n').map_or(l, |x| x.strip_suffix('\r').unwrap_or(x));
    (bare, &l[bare.len()..])
}

/// The text after `hunks`, each found after the one before. What a hunk only names to be found
/// stays as it is in the file.
pub fn apply(text: &str, hunks: &[Hunk], path: &str) -> Result<String, EditError> {
    let whole: Vec<(&str, &str)> = text.split_inclusive('\n').map(split).collect();
    let lines: Vec<&str> = whole.iter().map(|l| l.0).collect();
    let usual = whole.first().map(|l| l.1).filter(|e| !e.is_empty()).unwrap_or("\n");
    let mut out: Vec<(&str, &str)> = Vec::new();
    let mut at = 0;
    for h in hunks {
        let mut ops = h.lines.as_slice();
        let mut old: Vec<&str> = h.without(Op::Add).collect();
        let lost = || EditError::Hunk {
            line: h.line,
            path: path.into(),
            hint: hint(text, &h.without(Op::Add).collect::<Vec<_>>().join("\n")),
        };
        let mut from = at;
        if let Some(a) = &h.after {
            from = (at..lines.len()).find(|i| lines[*i].trim() == a.trim()).ok_or_else(lost)? + 1;
        }
        let pos = if old.is_empty() {
            // Only added: behind the line it names, or at the end.
            if h.after.is_some() { from } else { lines.len() }
        } else {
            loop {
                if let Some(p) = seek(&lines, &old, from, h.end) {
                    break p;
                }
                // Empty lines after a hunk are often only the patch's own.
                match ops {
                    [o @ .., (Op::Keep, x)] if x.is_empty() && old.len() > 1 => {
                        ops = o;
                        old.pop();
                    }
                    _ => return Err(lost()),
                }
            }
        };
        out.extend(&whole[at..pos]);
        at = pos;
        for (op, l) in ops {
            match op {
                Op::Keep => {
                    out.push(whole[at]);
                    at += 1;
                }
                Op::Remove => at += 1,
                // As its neighbours end.
                Op::Add => {
                    let near = [whole.get(at), at.checked_sub(1).and_then(|i| whole.get(i))];
                    let end = near.into_iter().flatten().map(|l| l.1).find(|e| !e.is_empty());
                    out.push((l, end.unwrap_or(usual)));
                }
            }
        }
    }
    out.extend(&whole[at..]);
    let ends = text.is_empty() || text.ends_with('\n');
    let n = out.len();
    let mut s = String::with_capacity(text.len());
    for (i, (l, end)) in out.into_iter().enumerate() {
        s.push_str(l);
        match (i + 1 == n, ends, end) {
            (true, false, _) => {}
            // It was the last, and is not any more.
            (_, _, "") => s.push_str(usual),
            _ => s.push_str(end),
        }
    }
    Ok(s)
}

/// Where `old` is: exactly, or else with other spaces at the ends of lines.
fn seek(lines: &[&str], old: &[&str], from: usize, end: bool) -> Option<usize> {
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

    #[test]
    fn lines_end_as_they_did() {
        // Mixed: only what is replaced changes.
        let t = "a\r\nb\nc\r\nd\n";
        assert_eq!(replace(t, "b", "x", false).unwrap(), "a\r\nx\nc\r\nd\n");
        assert_eq!(replace(t, "a\nb\nc", "1\n2", false).unwrap(), "1\r\n2\r\nd\n");
        assert_eq!(replace(t, "b\nc", "1\n2\n3", false).unwrap(), "a\r\n1\n2\n3\r\nd\n");
        assert_eq!(replace(t, "a\r\nb", "x", false).unwrap(), "x\nc\r\nd\n");
        assert_eq!(replace("a\nb\n", "b", "x\r\ny", false).unwrap(), "a\nx\r\ny\n");
        assert_eq!(replace("x\r\nx\n", "x", "y\nz", true).unwrap(), "y\r\nz\r\ny\nz\n");
        assert_eq!(replace("a\r\nb", "b", "c\nd", false).unwrap(), "a\r\nc\r\nd");
        assert_eq!(replace("a\r\n", "a\r\n", "a\n", false), Err(EditError::Same));
        assert_eq!(update(t, " a\n-b\n+x\n+y\n c").unwrap(), "a\r\nx\r\ny\r\nc\r\nd\n");
        assert_eq!(update(t, " c\n+x").unwrap(), "a\r\nb\nc\r\nx\nd\n");
        // What is only named stays as it is, with its spaces.
        assert_eq!(update("a  \nb\nc\t\n", " a\n-b\n+x\n c").unwrap(), "a  \nx\nc\t\n");
        assert_eq!(update("a", " a\n+b").unwrap(), "a\nb");
        assert_eq!(update("a\r\nb", "+c").unwrap(), "a\r\nb\r\nc");
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
        assert_eq!(update("a  \nb\n", " a\n-b\n+c").unwrap(), "a  \nc\n");
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
        /// Whatever ends its lines: what is not replaced is as it was.
        #[test]
        fn a_replacement_changes_nothing_else(t in "([ab]|\r\n|\n|\r){0,14}", o in "[ab\n]{1,3}") {
            let Ok(r) = replace(&t, &o, "X", false) else { return Ok(()) };
            let (a, z) = r.split_once('X').unwrap();
            prop_assert!(t.starts_with(a) && t.ends_with(z) && a.len() + z.len() < t.len());
            prop_assert_eq!(t[a.len()..t.len() - z.len()].replace("\r\n", "\n"), o);
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
