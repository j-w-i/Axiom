//! What agents, scripts and the host write never reaches the terminal as control codes, and never
//! looks like what it is not (SI-15).

use std::fmt::Write;

/// Many lines, as they are, with everything that does not print made visible.
pub fn block(s: &str) -> String {
    escape(s, |c| c == '\n' || c == '\t', |o, c| o.extend(c.escape_unicode()))
}

/// One line, whatever it holds: an approval cannot forge the line after it.
pub fn line(s: &str) -> String {
    escape(
        s,
        |_| false,
        |o, c| match c {
            '\n' | '\r' | '\t' => o.extend(c.escape_default()),
            c => o.extend(c.escape_unicode()),
        },
    )
}

/// JSON stays JSON. Its line breaks are the printer's: those in strings are escaped already.
pub fn json(s: &str) -> String {
    escape(
        s,
        |c| c == '\n',
        |o, c| {
            for u in c.encode_utf16(&mut [0; 2]) {
                let _ = write!(o, "\\u{u:04x}");
            }
        },
    )
}

fn escape(s: &str, keep: impl Fn(char) -> bool, put: impl Fn(&mut String, char)) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if hidden(c) && !keep(c) { put(&mut out, c) } else { out.push(c) }
    }
    out
}

/// Control codes (Cc), formatting (Cf: bidirectional overrides, zero-width characters, tags) and
/// the line and paragraph separators (Zl, Zp).
fn hidden(c: char) -> bool {
    c.is_control()
        || matches!(c,
            '\u{ad}' | '\u{600}'..='\u{605}' | '\u{61c}' | '\u{6dd}' | '\u{70f}'
            | '\u{890}'..='\u{891}' | '\u{8e2}' | '\u{180e}' | '\u{200b}'..='\u{200f}'
            | '\u{2028}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}'
            | '\u{fff9}'..='\u{fffb}' | '\u{110bd}' | '\u{110cd}' | '\u{13430}'..='\u{1343f}'
            | '\u{1bca0}'..='\u{1bca3}' | '\u{1d173}'..='\u{1d17a}' | '\u{e0000}'..='\u{e007f}')
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVIL: &str = "a\u{1b}]0;x\u{7}\u{9b}31m\u{7f}\u{202e}\u{200d}\u{2028}\u{e0041}é";

    #[test]
    fn nothing_hidden_is_left() {
        for f in [block, line, json] {
            assert!(!f(EVIL).chars().any(hidden), "{}", f(EVIL));
            assert!(f(EVIL).starts_with("a\\u"));
            assert!(f(EVIL).ends_with('é'));
        }
    }

    #[test]
    fn a_line_is_one_line_and_a_block_keeps_its_own() {
        assert_eq!(line("ls\r\n-- Completed\t"), "ls\\r\\n-- Completed\\t");
        assert_eq!(block("a\n\tb\r"), "a\n\tb\\u{d}");
    }

    #[test]
    fn json_is_still_json() {
        let v = serde_json::json!({ "t": EVIL, "n": "a\nb" });
        let text = json(&serde_json::to_string_pretty(&v).unwrap_or_default());
        assert_eq!(serde_json::from_str::<serde_json::Value>(&text).ok(), Some(v));
    }
}
