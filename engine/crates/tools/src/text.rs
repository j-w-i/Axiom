//! Text as a model and a person are shown it.

use std::fmt::Write;

/// Longest line shown whole, in characters.
pub const LINE: usize = 2000;

/// Lines from `from` (1 is the first), numbered, at most `lines` of them and about `bytes` bytes.
/// Says what is left out.
pub fn numbered(text: &str, from: usize, lines: usize, bytes: usize) -> String {
    let from = from.max(1);
    let all = text.lines().count();
    if all == 0 {
        return "(the file is empty)".into();
    }
    if from > all {
        return format!("(the file has only {all} lines)");
    }
    let mut out = String::new();
    let mut last = from - 1;
    for (i, l) in text.lines().enumerate().skip(from - 1).take(lines) {
        if out.len() > bytes {
            break;
        }
        let _ = match l.char_indices().nth(LINE) {
            Some((cut, _)) => {
                writeln!(out, "{:>6}\t{}… ({} characters)", i + 1, &l[..cut], l.chars().count())
            }
            None => writeln!(out, "{:>6}\t{l}", i + 1),
        };
        last = i + 1;
    }
    if last < all {
        let _ = writeln!(out, "({} more lines; read on from line {})", all - last, last + 1);
    }
    out
}

/// Output of a program without what would command a terminal: escape sequences and controls go;
/// a line that was drawn over keeps its last drawing.
pub fn plain(b: &[u8]) -> String {
    let s = String::from_utf8_lossy(b);
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\u{1b}' => match it.next() {
                Some('[') => {
                    while let Some(&n) = it.peek() {
                        if n.is_control() {
                            break;
                        }
                        it.next();
                        if ('\u{40}'..='\u{7e}').contains(&n) {
                            break;
                        }
                    }
                }
                // Strings, up to their end or a line's.
                Some(']' | 'P' | '_' | '^' | 'X') => {
                    while let Some(&n) = it.peek() {
                        if n == '\n' {
                            break;
                        }
                        it.next();
                        if n == '\u{7}' || (n == '\u{1b}' && it.next_if_eq(&'\\').is_some()) {
                            break;
                        }
                    }
                }
                Some('(' | ')' | '*' | '+' | '#' | '%') => {
                    it.next_if(|n| !n.is_control());
                }
                Some('\n') => out.push('\n'),
                _ => {}
            },
            '\r' => {
                if it.peek() != Some(&'\n') {
                    out.truncate(out.rfind('\n').map_or(0, |i| i + 1));
                }
            }
            '\u{8}' => {
                if !out.ends_with('\n') {
                    out.pop();
                }
            }
            '\n' | '\t' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// The beginning and the end of what does not fit.
#[derive(Debug)]
pub struct Ends {
    head: Vec<u8>,
    tail: std::collections::VecDeque<u8>,
    most_head: usize,
    most_tail: usize,
    pub total: u64,
}

impl Ends {
    pub fn new(head: usize, tail: usize) -> Self {
        Self {
            head: Vec::new(),
            tail: Default::default(),
            most_head: head,
            most_tail: tail,
            total: 0,
        }
    }

    pub fn add(&mut self, b: &[u8]) {
        self.total += b.len() as u64;
        let to_head = (self.most_head - self.head.len()).min(b.len());
        self.head.extend_from_slice(&b[..to_head]);
        let rest = &b[to_head..];
        let rest = &rest[rest.len().saturating_sub(self.most_tail)..];
        let over = (self.tail.len() + rest.len()).saturating_sub(self.most_tail);
        self.tail.drain(..over);
        self.tail.extend(rest);
    }

    /// Bytes that were left out.
    pub fn cut(&self) -> u64 {
        self.total - (self.head.len() + self.tail.len()) as u64
    }

    pub fn text(&self) -> String {
        let tail: Vec<u8> = self.tail.iter().copied().collect();
        if self.cut() == 0 {
            return plain(&[self.head.as_slice(), &tail].concat());
        }
        // Whole lines where there are any.
        let head = self
            .head
            .iter()
            .rposition(|b| *b == b'\n')
            .map_or(&self.head[..], |i| &self.head[..=i]);
        let tail = tail.iter().position(|b| *b == b'\n').map_or(&tail[..], |i| &tail[i + 1..]);
        let left = self.total - (head.len() + tail.len()) as u64;
        format!("{}\n… {left} bytes left out …\n\n{}", plain(head).trim_end(), plain(tail))
    }
}

/// What changes between two texts, with three lines around it.
pub fn diff(path: &str, before: Option<&str>, after: Option<&str>) -> String {
    let d = similar::TextDiff::from_lines(before.unwrap_or(""), after.unwrap_or(""));
    let old = if before.is_some() { path } else { "(new)" };
    let new = if after.is_some() { path } else { "(removed)" };
    d.unified_diff().context_radius(3).header(old, new).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn lines_are_numbered_and_bounded() {
        assert_eq!(numbered("a\nb\n", 1, 10, 1000), "     1\ta\n     2\tb\n");
        assert_eq!(
            numbered("a\nb\nc", 2, 1, 1000),
            "     2\tb\n(1 more lines; read on from line 3)\n"
        );
        assert_eq!(numbered("a", 5, 1, 1000), "(the file has only 1 lines)");
        assert_eq!(numbered("", 1, 1, 1000), "(the file is empty)");
        assert_eq!(numbered("a\r\nb", 0, 9, 99), "     1\ta\n     2\tb\n");
        let long = numbered(&"é".repeat(LINE + 5), 1, 1, 100_000);
        assert!(long.contains("… (2005 characters)") && long.chars().count() < LINE + 60);
        let many = numbered(&"x\n".repeat(10_000), 1, 10_000, 800);
        assert!(many.len() < 1000 && many.contains("more lines"), "{many}");
    }

    #[test]
    fn a_terminal_is_not_commanded() {
        for (i, o) in [
            ("a\x1b[31mb\x1b[0mc", "abc"),
            ("\x1b]0;title\x07x", "x"),
            ("\x1b]52;c;aGk=\x1b\\x", "x"),
            ("\x1b]8;;http://e\x07l\x1b]8;;\x07", "l"),
            ("a\x1b]never ends\nb", "a\nb"),
            ("\x1bP+q\x1b\\x", "x"),
            ("\x1b(Bx", "x"),
            ("10%\r20%\r30%\nx", "30%\nx"),
            ("a\r\nb", "a\nb"),
            ("ab\x08c", "ac"),
            ("a\x00\x07\x7f\u{9b}31mb", "a31mb"),
            ("x\x1b", "x"),
            ("x\x1b[", "x"),
            ("x\x1b[12\ny", "x\ny"),
            ("\x1b\ny", "\ny"),
            ("\ta", "\ta"),
        ] {
            assert_eq!(plain(i.as_bytes()), o, "{i:?}");
        }
        assert_eq!(plain(b"a\xffb"), "a\u{fffd}b");
    }

    #[test]
    fn both_ends_are_kept() {
        let mut e = Ends::new(10, 10);
        e.add(b"line1\nline2\n");
        assert_eq!((e.cut(), e.text().as_str()), (0, "line1\nline2\n"));
        for _ in 0..100 {
            e.add(b"xxxxxxxxx\n");
        }
        e.add(b"a\nend\n");
        let t = e.text();
        assert!(
            t.starts_with("line1\n") && t.ends_with("\na\nend\n") && t.contains("bytes left out"),
            "{t}"
        );
    }

    #[test]
    fn a_diff_names_what_is_new_and_gone() {
        assert_eq!(
            diff("f", Some("a\nb\n"), Some("a\nc\n")),
            "--- f\n+++ f\n@@ -1,2 +1,2 @@\n a\n-b\n+c\n"
        );
        assert!(diff("f", None, Some("a\n")).starts_with("--- (new)\n+++ f\n"));
        assert!(diff("f", Some("a\n"), None).contains("+++ (removed)\n"));
    }

    proptest! {
        #[test]
        fn ends_do_not_depend_on_pieces(data in prop::collection::vec(any::<u8>(), 0..200), cuts in prop::collection::vec(0usize..200, 0..6)) {
            let mut one = Ends::new(20, 30);
            one.add(&data);
            let mut cuts: Vec<usize> = cuts.into_iter().map(|c| c.min(data.len())).collect();
            cuts.push(0);
            cuts.push(data.len());
            cuts.sort_unstable();
            let mut many = Ends::new(20, 30);
            for w in cuts.windows(2) {
                many.add(&data[w[0]..w[1]]);
            }
            prop_assert_eq!(one.text(), many.text());
            prop_assert_eq!(one.cut(), data.len().saturating_sub(50) as u64);
        }

        #[test]
        fn no_control_gets_through(data in prop::collection::vec(any::<u8>(), 0..100)) {
            let t = plain(&data);
            prop_assert!(!t.chars().any(|c| c.is_control() && c != '\n' && c != '\t'), "{:?}", t);
        }
    }
}
