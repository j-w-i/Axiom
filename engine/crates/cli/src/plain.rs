//! What agents, scripts and the host write never reaches the terminal as control codes, and never
//! looks like what it is not (SI-15).

use std::fmt::Write;

/// Prose: many lines, as they are, with everything that does not print made visible. The marks
/// that make one emoji or one word of several characters stay where they do that.
pub fn block(s: &str) -> String {
    let mut before = '\n';
    escape(
        s,
        |c| {
            let joins = match c {
                '\u{200c}' | '\u{200d}' => word(before) || emoji(before) || before == '\u{fe0f}',
                '\u{fe0e}' | '\u{fe0f}' => emoji(before) || "0123456789#*".contains(before),
                '\n' | '\t' => true,
                _ => false,
            };
            before = c;
            joins
        },
        |o, c| o.extend(c.escape_unicode()),
    )
}

/// One line, whatever it holds: an approval cannot forge the line after it, or hide a part of
/// itself.
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

fn escape(s: &str, mut keep: impl FnMut(char) -> bool, put: impl Fn(&mut String, char)) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let kept = keep(c);
        if hidden(c) && !kept { put(&mut out, c) } else { out.push(c) }
    }
    out
}

/// What draws nothing, or moves what is drawn. Code points by number, sorted, each range with both
/// ends; from the Unicode Character Database 15.0: DerivedGeneralCategory.txt (Cc, Cf, Zl, Zp) and
/// DerivedCoreProperties.txt (Default_Ignorable_Code_Point, which has the variation selectors, the
/// tags and the Hangul fillers); and four that are drawn as a blank though they are letters, marks
/// or symbols: U+2800, U+16FE4, U+1D159, U+1DA9B.
const HIDDEN: &[(u32, u32)] = &[
    (0x00, 0x1f),
    (0x7f, 0x9f),
    (0xad, 0xad),
    (0x34f, 0x34f),
    (0x600, 0x605),
    (0x61c, 0x61c),
    (0x6dd, 0x6dd),
    (0x70f, 0x70f),
    (0x890, 0x891),
    (0x8e2, 0x8e2),
    (0x115f, 0x1160),
    (0x17b4, 0x17b5),
    (0x180b, 0x180f),
    (0x200b, 0x200f),
    (0x2028, 0x202e),
    (0x2060, 0x206f),
    (0x2800, 0x2800),
    (0x3164, 0x3164),
    (0xfe00, 0xfe0f),
    (0xfeff, 0xfeff),
    (0xffa0, 0xffa0),
    (0xfff0, 0xfffb),
    (0x110bd, 0x110bd),
    (0x110cd, 0x110cd),
    (0x13430, 0x1343f),
    (0x16fe4, 0x16fe4),
    (0x1bca0, 0x1bca3),
    (0x1d159, 0x1d159),
    (0x1d173, 0x1d17a),
    (0x1da9b, 0x1da9b),
    (0xe0000, 0xe0fff),
];

fn hidden(c: char) -> bool {
    within(c, HIDDEN)
}

/// Letters of the scripts in which a joiner, or its opposite, changes the word.
fn word(c: char) -> bool {
    within(
        c,
        &[(0x600, 0x8ff), (0x900, 0xdff), (0x1000, 0x109f), (0x1800, 0x18af), (0xfb50, 0xfefc)],
    )
}
fn emoji(c: char) -> bool {
    within(c, &[(0xa9, 0xa9), (0xae, 0xae), (0x203c, 0x3299), (0x1f000, 0x1faff)])
}

fn within(c: char, ranges: &[(u32, u32)]) -> bool {
    let c = c as u32;
    let after = ranges.partition_point(|(_, last)| *last < c);
    ranges.get(after).is_some_and(|(first, _)| *first <= c)
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
    fn an_approval_hides_nothing_at_all() {
        let all = (0..=0x10ffff).filter_map(char::from_u32);
        let bad: Vec<char> = all.filter(|c| hidden(*c)).collect();
        for c in &bad {
            let shown = line(&format!("ls{c}x"));
            assert!(
                shown.starts_with("ls\\") && shown.ends_with('x') && shown.is_ascii(),
                "{shown}"
            );
        }
        // The isolates and their neighbours, fillers, blanks, selectors, tags.
        for c in (0x2065..=0x206f).chain([
            0x34f, 0x115f, 0x1160, 0x17b4, 0x180e, 0x2800, 0x3164, 0xfe00, 0xfe0f, 0xffa0, 0xfff8,
            0x16fe4, 0x1d159, 0x1da9b, 0xe0001, 0xe007f, 0xe0100, 0xe01ef, 0xe0fff,
        ]) {
            assert!(bad.contains(&char::from_u32(c).unwrap()), "{c:x}");
        }
        // Nothing that is drawn.
        for c in
            " ~é\u{a1}\u{5d0}\u{627}\u{3042}\u{ac00}\u{2060e}\u{fffd}\u{1f600}\u{1d15a}".chars()
        {
            assert!(!hidden(c), "{:x}", c as u32);
        }
        assert!(HIDDEN.windows(2).all(|w| w[0].0 <= w[0].1 && w[0].1 < w[1].0), "sorted");
    }

    #[test]
    fn prose_keeps_the_marks_that_make_an_emoji_or_a_word() {
        for s in [
            "\u{1f469}\u{200d}\u{1f4bb} and \u{2764}\u{fe0f}\u{200d}\u{1f525}",
            "1\u{fe0f}\u{20e3}",
            "\u{645}\u{6cc}\u{200c}\u{62e}",
            "\u{915}\u{94d}\u{200d}",
        ] {
            assert_eq!(block(s), s);
            assert!(line(s).contains("\\u{"), "not in an approval");
        }
        // Where they make nothing, they hide something.
        assert_eq!(
            block("a\u{200d}b\u{fe0f}c\u{1f600}\u{fe00}"),
            "a\\u{200d}b\\u{fe0f}c\u{1f600}\\u{fe00}"
        );
    }

    #[test]
    fn json_is_still_json() {
        let v = serde_json::json!({ "t": EVIL, "n": "a\nb" });
        let text = json(&serde_json::to_string_pretty(&v).unwrap_or_default());
        assert_eq!(serde_json::from_str::<serde_json::Value>(&text).ok(), Some(v));
    }
}
