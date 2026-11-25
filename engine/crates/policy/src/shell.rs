//! Shell commands a person's rules may speak about: those that can be read completely (SI-16). The
//! grammar is closed and small; whatever is not in it is not analysable and is asked about. This
//! classifies; the sandbox contains.
//!
//! In it: simple commands of literal words, joined by `&&`, `||`, `;`, `|` or a line's end, with
//! `2>&1` and output to `/dev/null`. Not in it: every expansion, substitution, glob, assignment,
//! redirection to a file, background job, compound command, comment, and the shell's own commands
//! that take code, names or arithmetic in their arguments.

/// The words of one command, as the command would get them. Never empty.
pub type Words = Vec<String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NotAnalysable {
    #[error("the command is empty")]
    Empty,
    #[error("the command is too long to read")]
    Long,
    #[error("the command has a character that is not read here")]
    Character(char),
    #[error("a quote is not closed")]
    Quote,
    #[error("a command is missing beside an operator")]
    Operator,
    #[error("the command sets a variable")]
    Assignment,
    #[error("the shell itself would interpret this")]
    Shell,
}

const LONGEST: usize = 16 * 1024;

/// What the shell does itself. Of its commands only these take nothing that it would interpret.
const HARMLESS: &[&str] = &["echo", "pwd", "true", "false", "cd"];

/// Reserved words and the rest of what bash, zsh and dash do themselves.
const SHELL: &[&str] = &[
    "if",
    "then",
    "else",
    "elif",
    "fi",
    "case",
    "esac",
    "for",
    "select",
    "while",
    "until",
    "do",
    "done",
    "in",
    "function",
    "time",
    "coproc",
    "{",
    "}",
    "!",
    "[[",
    "]]",
    "[",
    "test",
    ".",
    ":",
    "source",
    "eval",
    "exec",
    "command",
    "builtin",
    "trap",
    "alias",
    "unalias",
    "set",
    "unset",
    "shift",
    "export",
    "readonly",
    "declare",
    "typeset",
    "local",
    "let",
    "read",
    "readarray",
    "mapfile",
    "printf",
    "getopts",
    "hash",
    "type",
    "ulimit",
    "umask",
    "wait",
    "jobs",
    "bg",
    "fg",
    "kill",
    "disown",
    "suspend",
    "exit",
    "logout",
    "return",
    "break",
    "continue",
    "enable",
    "bind",
    "complete",
    "compgen",
    "compopt",
    "caller",
    "dirs",
    "pushd",
    "popd",
    "fc",
    "history",
    "help",
    "shopt",
    "times",
    "nocorrect",
    "noglob",
    "repeat",
    "foreach",
    "end",
    "always",
    "print",
    "vared",
    "zmodload",
    "autoload",
    "functions",
    "setopt",
    "unsetopt",
    "emulate",
    "bindkey",
    "zle",
    "zstyle",
    "chdir",
    "integer",
    "float",
    "private",
    "limit",
    "unlimit",
    "sched",
    "which",
    "whence",
    "where",
    "rehash",
    "r",
    "ttyctl",
    "zcompile",
    "zformat",
    "zparseopts",
    "zregexparse",
    "comparguments",
    "compadd",
    "getln",
    "pushln",
    "log",
    "stat",
    "strftime",
    "-",
];

/// Plain in every position of a word.
fn plain(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(c, '_' | '-' | '.' | '/' | ':' | '@' | '%' | '+' | ',' | '=')
}

/// Inside quotes: what a person sees is what there is.
fn quoted(c: char) -> bool {
    c.is_ascii_graphic() || matches!(c, ' ' | '\t' | '\n') || (!c.is_ascii() && c.is_alphanumeric())
}

/// The commands of `s`, in order, or why it cannot be read.
pub fn parse(s: &str) -> Result<Vec<Words>, NotAnalysable> {
    use NotAnalysable as E;
    if s.len() > LONGEST {
        return Err(E::Long);
    }
    let c: Vec<char> = s.chars().collect();
    let at = |i: usize| c.get(i).copied();
    let mut all: Vec<Words> = Vec::new();
    let mut words: Words = Vec::new();
    // Whether an operator waits for the command after it; whether a line may end here.
    let mut owed = false;
    let mut i = 0;
    while let Some(ch) = at(i) {
        match ch {
            ' ' | '\t' => i += 1,
            '\n' => {
                if !words.is_empty() {
                    finish(&mut all, &mut words)?;
                    owed = false;
                }
                i += 1;
            }
            ';' => {
                if words.is_empty() || at(i + 1) == Some(';') {
                    return Err(E::Operator);
                }
                finish(&mut all, &mut words)?;
                owed = false;
                i += 1;
            }
            '&' | '|' => {
                let two = at(i + 1) == Some(ch);
                if ch == '&' && !two {
                    return Err(E::Character('&'));
                }
                if at(i + 1) == Some('&') && ch == '|' {
                    return Err(E::Character('&'));
                }
                if words.is_empty() {
                    return Err(E::Operator);
                }
                finish(&mut all, &mut words)?;
                owed = true;
                i += if two { 2 } else { 1 };
            }
            '>' => {
                if words.is_empty() {
                    return Err(E::Operator);
                }
                i = to_null(&c, i + 1).ok_or(E::Character('>'))?;
            }
            _ => {
                // A number right before `>` is a file descriptor, never part of a word.
                if ch == '2' && at(i + 1) == Some('>') {
                    if words.is_empty() {
                        return Err(E::Operator);
                    }
                    i = if at(i + 2) == Some('&') && at(i + 3) == Some('1') {
                        i + 4
                    } else {
                        to_null(&c, i + 2).ok_or(E::Character('>'))?
                    };
                    if !ends(at(i)) {
                        return Err(E::Character('>'));
                    }
                    continue;
                }
                let first = words.is_empty();
                let (w, bare, to) = word(&c, i)?;
                // zsh puts the path of a program for `=program`, also after empty quotes.
                if bare.starts_with('=') {
                    return Err(E::Character('='));
                }
                if first {
                    if bare.contains('=') {
                        return Err(E::Assignment);
                    }
                    if w.is_empty() {
                        return Err(E::Empty);
                    }
                }
                words.push(w);
                i = to;
            }
        }
    }
    if !words.is_empty() {
        finish(&mut all, &mut words)?;
        owed = false;
    }
    if owed {
        return Err(E::Operator);
    }
    if all.is_empty() {
        return Err(E::Empty);
    }
    Ok(all)
}

/// Whether a word may end before this.
fn ends(c: Option<char>) -> bool {
    matches!(c, None | Some(' ' | '\t' | '\n' | ';' | '&' | '|'))
}

/// Past `/dev/null`, with blanks before it, or nothing.
fn to_null(c: &[char], mut i: usize) -> Option<usize> {
    while matches!(c.get(i), Some(' ' | '\t')) {
        i += 1;
    }
    let null: Vec<char> = "/dev/null".chars().collect();
    let end = i + null.len();
    (c.get(i..end)? == null.as_slice() && ends(c.get(end).copied())).then_some(end)
}

/// A word from `i`, its unquoted part, and where it ends.
fn word(c: &[char], mut i: usize) -> Result<(String, String, usize), NotAnalysable> {
    use NotAnalysable as E;
    let (mut w, mut bare) = (String::new(), String::new());
    while let Some(&ch) = c.get(i) {
        match ch {
            '\'' | '"' => {
                i += 1;
                loop {
                    match c.get(i) {
                        None => return Err(E::Quote),
                        Some(&q) if q == ch => break,
                        Some(&x) if ch == '"' && matches!(x, '$' | '`' | '\\' | '!') => {
                            return Err(E::Character(x));
                        }
                        Some(&x) if quoted(x) => w.push(x),
                        Some(&x) => return Err(E::Character(x)),
                    }
                    i += 1;
                }
                i += 1;
            }
            x if plain(x) => {
                w.push(x);
                bare.push(x);
                i += 1;
            }
            // `a>b`: the number or word before belongs to nothing we read.
            x if ends(Some(x)) => break,
            x => return Err(E::Character(x)),
        }
    }
    Ok((w, bare, i))
}

fn finish(all: &mut Vec<Words>, words: &mut Words) -> Result<(), NotAnalysable> {
    let w = std::mem::take(words);
    if w.first().is_some_and(|p| SHELL.contains(&p.as_str()) && !HARMLESS.contains(&p.as_str())) {
        return Err(NotAnalysable::Shell);
    }
    all.push(w);
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;
    #[cfg(unix)]
    use proptest::prelude::*;

    fn words(s: &str) -> Vec<Vec<String>> {
        parse(s).unwrap_or_else(|e| panic!("{s:?}: {e:?}"))
    }

    #[test]
    fn reads_what_is_plain() {
        assert_eq!(words("cargo test -p foo"), [["cargo", "test", "-p", "foo"]]);
        assert_eq!(words("  a  'b c'd\"e f\"  "), [vec!["a", "b cde f"]]);
        assert_eq!(
            words("a 1 && b | c || d; e\nf\n\n"),
            [vec!["a", "1"], vec!["b"], vec!["c"], vec!["d"], vec!["e"], vec!["f"]]
        );
        assert_eq!(words("a &&\n b"), [["a"], ["b"]]);
        assert_eq!(words("make 2>&1 | tail -5"), [vec!["make"], vec!["tail", "-5"]]);
        assert_eq!(words("make >/dev/null 2>/dev/null"), [["make"]]);
        assert_eq!(words("make > /dev/null 2>&1; x"), [["make"], ["x"]]);
        assert_eq!(words("git commit -m 'a && b $(c)'"), [["git", "commit", "-m", "a && b $(c)"]]);
        assert_eq!(words("a '' \"\""), [["a", "", ""]]);
        assert_eq!(words("x --a=b"), [["x", "--a=b"]]);
        assert_eq!(words("echo x2"), [["echo", "x2"]]);
    }

    /// Each of these got past an allow-list somewhere (threat-model T16).
    #[test]
    fn the_rest_is_asked_about() {
        for s in [
            "",
            "  ",
            "\n",
            ";",
            "&&",
            "a &&",
            "a |",
            "| a",
            "a && && b",
            "a;;b",
            "a & b",
            "a &",
            "a |& b",
            "a &> b",
            "a > b",
            "a >> b",
            "a < b",
            "a <<< b",
            "a << b",
            "a 2> b",
            "a 2>&1x",
            "a >/dev/nullx",
            "a > /dev/null/x",
            "a >| b",
            "a >! b",
            "a 1>/dev/null",
            "a 3>&1",
            "a2>/dev/null",
            "> /dev/null",
            "2>&1",
            "a >&2",
            "a >/dev/null>x",
            "a x>/dev/null",
            "echo $x",
            "echo ${x@P}",
            "echo $(id)",
            "echo `id`",
            "echo $((1))",
            "echo \"$x\"",
            "echo \"`id`\"",
            "echo \"a\\\"b\"",
            "echo \"!x\"",
            "echo a\\ b",
            "echo \\x",
            "echo *",
            "echo ?",
            "echo [a]",
            "echo {a,b}",
            "echo ~",
            "~/x",
            "echo a~b",
            "echo #x",
            "a #b",
            "(a)",
            "a()",
            "{ a; }",
            "! a",
            "a ! b",
            "echo ^a",
            "PAGER=x git log",
            "A+=b c",
            "a=b",
            "FOO= x",
            "if a; then b; fi",
            "for x in a; do b; done",
            "while a; do b; done",
            "time a",
            "[[ a ]]",
            "[ a ]",
            "eval 'a'",
            "exec a",
            ". a",
            "source a",
            "command a",
            "builtin eval a",
            "trap 'a' EXIT",
            "alias a=b",
            "export A=b",
            "declare -i x",
            "printf -v 'a[$(id)]' x",
            "test -v 'a[$(id)]'",
            "read 'a[$(id)]'",
            "unset 'a[$(id)]'",
            "let 'a[$(id)]=1'",
            "set -x",
            "enable -f a b",
            "hash -p a b",
            "mapfile -C a",
            "'eval' a",
            "e'va'l a",
            "\"eval\" a",
            "ev\"\"al a",
            "''",
            "\"\" a",
            "echo 'a",
            "echo \"a",
            "a\rb",
            "a\0b",
            "a\u{b}b",
            "a\u{a0}b",
            "a\u{2003}b",
            "a\u{202e}b",
            "'a\u{202e}b'",
            "'a\u{200b}b'",
            "'a\u{7f}b'",
            "'a\rb'",
            "a\u{85}b",
            "=ls",
            "é",
            "a%b)",
            "a\\\nb",
            "echo $'a'",
            "echo $\"a\"",
        ] {
            assert!(parse(s).is_err(), "{s:?} -> {:?}", parse(s));
        }
        assert_eq!(parse(&"a ".repeat(LONGEST)), Err(NotAnalysable::Long));
    }

    /// What the reader says a shell would run is what the shell runs: every command is replaced by a
    /// function that writes its name and arguments.
    #[cfg(unix)]
    fn as_run_by(shell: &str, s: &str, parsed: &[Words]) -> Option<Vec<Words>> {
        use std::process::{Command, Stdio};
        let mut script = String::from("set -f\n");
        let names: std::collections::BTreeSet<&str> =
            parsed.iter().map(|w| w[0].as_str()).collect();
        for n in &names {
            script +=
                &format!("{n}() {{ printf '%s\\0' \"$0\" \"$#\" {n} \"$@\" >&9; return 0; }}\n");
        }
        script += s;
        let dir = tempfile::tempdir().ok()?;
        let out = dir.path().join("o");
        let full = format!("exec 9>>'{}'\n{script}", out.display());
        let st = Command::new(shell)
            .args(["-c", &full, "sh"])
            .env_clear()
            .current_dir(dir.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .ok()?;
        assert!(st.success(), "{shell} on {s:?}: {st}");
        let raw = std::fs::read(&out).ok()?;
        let mut it = raw.split(|b| *b == 0).map(|b| String::from_utf8_lossy(b).into_owned());
        let mut got = Vec::new();
        while let Some(_zero) = it.next().filter(|z| !z.is_empty()) {
            let n: usize = it.next()?.parse().ok()?;
            let w: Words = it.by_ref().take(n + 1).collect();
            got.push(w);
        }
        Some(got)
    }

    #[cfg(unix)]
    fn piece() -> impl Strategy<Value = String> {
        prop_oneof![
            4 => "[a-c0-2./:@%+,=_-]{1,4}",
            2 => "'[ a-c$`\"*;|&<>()!#~\\\\\n]{0,5}'",
            2 => "\"[ a-c';|&<>()*#~\n]{0,5}\"",
            1 => prop::sample::select(vec![" ", "  ", "\t"]).prop_map(String::from),
            1 => prop::sample::select(vec!["2>&1", ">/dev/null", "> /dev/null", "2>/dev/null"]).prop_map(String::from),
            1 => prop::sample::select(vec!["$x", "`b`", "\\", "*", "~", "#", "&", "(", ")", "<", ">", "!", "{", "}", "[", "?"]).prop_map(String::from),
        ]
    }

    #[cfg(unix)]
    fn line() -> impl Strategy<Value = String> {
        let cmd = ("[a-c]x[a-c]", prop::collection::vec(piece(), 0..5), any::<bool>())
            .prop_map(|(n, p, sp)| format!("{n} {}", p.join(if sp { " " } else { "" })))
            .boxed();
        (
            cmd.clone(),
            prop::collection::vec(
                (prop::sample::select(vec![" && ", "; ", "\n", "&&", ";"]), cmd),
                0..3,
            ),
        )
            .prop_map(|(a, rest)| rest.into_iter().fold(a, |s, (op, c)| s + op + &c))
    }

    #[cfg(unix)]
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(400))]

        #[test]
        fn agrees_with_real_shells(s in line()) {
            if let Ok(p) = parse(&s) {
                for sh in ["/bin/bash", "/bin/zsh", "/bin/sh"] {
                    if !std::path::Path::new(sh).exists() {
                        continue;
                    }
                    let got = as_run_by(sh, &s, &p);
                    prop_assert_eq!(got.as_ref(), Some(&p), "{} on {:?}", sh, s);
                }
            }
        }

        #[test]
        fn never_panics_and_never_makes_up_text(s in "\\PC{0,40}") {
            if let Ok(p) = parse(&s) {
                for w in p.iter().flatten() {
                    prop_assert!(w.chars().all(|c| s.contains(c)));
                }
            }
        }
    }
}
