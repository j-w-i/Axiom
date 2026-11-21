//! Golden tests for the diagnostics parsers (M2-BUILD-001), over real tool output where a
//! tool exists (fixtures/README.md), and a fuzz pass: no parser may panic.

use axiom_build::parse::{workspace_path, Parser};
use axiom_protocol::diagnostics::{Location, Severity};
use axiom_protocol::provider::DiagnosticsFormat;
use axiom_protocol::task::Stream;
use axiom_protocol::WorkspacePath;
use std::path::PathBuf;

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

fn roots() -> Vec<PathBuf> {
    vec![PathBuf::from(if cfg!(windows) {
        r"C:\work\e0308"
    } else {
        "/work/e0308"
    })]
}

fn run(
    format: DiagnosticsFormat,
    stream: Stream,
    text: &str,
) -> Vec<axiom_protocol::diagnostics::Diagnostic> {
    let mut p = Parser::new(&format, roots());
    let mut out = Vec::new();
    for l in text.lines() {
        out.extend(p.line(stream, l));
    }
    out.extend(p.finish());
    out
}

fn loc(file: &str, line: u32, column: Option<u32>) -> (WorkspacePath, u32, Option<u32>) {
    (WorkspacePath(file.into()), line, column)
}

fn at(l: &Option<Location>) -> Option<(WorkspacePath, u32, Option<u32>)> {
    l.as_ref().map(|l| (l.file.clone(), l.line, l.column))
}

#[test]
fn cargo_json_e0308() {
    let d = run(
        DiagnosticsFormat::CargoJson,
        Stream::Stdout,
        &fixture("cargo_e0308.jsonl"),
    );
    // One error; "aborting due to …" is not a diagnostic.
    assert_eq!(d.len(), 1, "{d:#?}");
    let e = &d[0];
    assert_eq!(e.severity, Severity::Error);
    assert_eq!(e.code.as_deref(), Some("E0308"));
    assert_eq!(e.message, "mismatched types");
    assert_eq!(e.source, "rustc");
    assert_eq!(at(&e.location), Some(loc("src/main.rs", 2, Some(18))));
    let l = e.location.as_ref().unwrap();
    assert_eq!((l.end_line, l.end_column), (Some(2), Some(24)));
    // The secondary span ("expected due to this") is related information.
    assert!(
        e.related
            .iter()
            .any(|r| r.message == "expected due to this" && r.location.column == Some(12)),
        "{:?}",
        e.related
    );
    let rendered = e.rendered.as_deref().unwrap();
    assert!(
        rendered.contains("error[E0308]: mismatched types"),
        "{rendered}"
    );
    assert!(!rendered.contains('\u{1b}'), "ANSI stripped");
    // Stderr lines (cargo's progress) are ignored for cargo_json.
    assert!(run(
        DiagnosticsFormat::CargoJson,
        Stream::Stderr,
        &fixture("cargo_e0308.jsonl")
    )
    .is_empty());
}

#[test]
fn gcc_text_multiline() {
    let d = run(
        DiagnosticsFormat::GccText,
        Stream::Stderr,
        &fixture("gcc_too_few_args.txt"),
    );
    assert_eq!(d.len(), 2, "{d:#?}");
    let (e, w) = (&d[0], &d[1]);
    assert_eq!(e.severity, Severity::Error);
    assert_eq!(
        e.message,
        "too few arguments to function ‘add’; expected 2, have 1"
    );
    assert_eq!(at(&e.location), Some(loc("main.c", 7, Some(13))));
    // The note that follows becomes related information, the excerpt part of `rendered`.
    assert_eq!(e.related.len(), 1);
    assert_eq!(e.related[0].message, "declared here");
    assert_eq!(
        (e.related[0].location.line, e.related[0].location.column),
        (3, Some(12))
    );
    assert!(e.rendered.as_deref().unwrap().contains("int r = add(1);"));
    assert_eq!(w.severity, Severity::Warning);
    assert_eq!(w.message, "unused variable ‘unused’");
    assert_eq!(w.code.as_deref(), Some("-Wunused-variable"));
    assert_eq!(at(&w.location), Some(loc("main.c", 6, Some(9))));

    // Clang's form (no column, fatal error) and a Windows drive letter.
    let clang = "lib/x.h:4: fatal error: 'missing.h' file not found\nC:\\work\\e0308\\src\\a.c:1:2: warning: w";
    let d = run(DiagnosticsFormat::GccText, Stream::Stderr, clang);
    assert_eq!(d[0].severity, Severity::Error);
    assert_eq!(at(&d[0].location), Some(loc("lib/x.h", 4, None)));
    if cfg!(windows) {
        assert_eq!(at(&d[1].location), Some(loc("src/a.c", 1, Some(2))));
    } else {
        assert_eq!(
            d[1].location, None,
            "a Windows absolute path is not inside a Unix root"
        );
    }
}

#[test]
fn regex_named_groups() {
    let pattern = r"^(?P<severity>ERROR|WARN) (?P<file>[^ ]+) line (?P<line>\d+)(?: col (?P<column>\d+))?: (?P<message>.+?)(?: \((?P<code>[A-Z]\d+)\))?$";
    let out = "starting\nERROR src/a.py line 3 col 5: bad thing (E101)\nWARN src/b.py line 10: meh\nnot a match\n";
    let d = run(
        DiagnosticsFormat::Regex {
            pattern: pattern.into(),
        },
        Stream::Stdout,
        out,
    );
    assert_eq!(d.len(), 2, "{d:#?}");
    assert_eq!(
        (d[0].severity, d[0].message.as_str(), d[0].code.as_deref()),
        (Severity::Error, "bad thing", Some("E101"))
    );
    assert_eq!(at(&d[0].location), Some(loc("src/a.py", 3, Some(5))));
    assert_eq!(
        (d[1].severity, at(&d[1].location)),
        (Severity::Warning, Some(loc("src/b.py", 10, None)))
    );
    // An invalid pattern yields nothing rather than failing the build.
    assert!(run(
        DiagnosticsFormat::Regex {
            pattern: "(".into()
        },
        Stream::Stdout,
        out
    )
    .is_empty());
}

#[test]
fn sarif_minimal() {
    let root = &roots()[0];
    let abs_uri = if cfg!(windows) {
        "file:///C:/work/e0308/src/lib%20x.rs".to_string()
    } else {
        format!("file://{}/src/lib%20x.rs", root.display())
    };
    let doc = format!(
        r#"{{"version":"2.1.0","runs":[{{"tool":{{"driver":{{"name":"clippy-sarif"}}}},"results":[
          {{"ruleId":"clippy::needless_return","level":"warning","message":{{"text":"unneeded return"}},
            "locations":[{{"physicalLocation":{{"artifactLocation":{{"uri":"src/main.rs"}},"region":{{"startLine":4,"startColumn":5,"endLine":4,"endColumn":14}}}}}}]}},
          {{"ruleId":"R2","level":"error","message":{{"text":"absolute"}},
            "locations":[{{"physicalLocation":{{"artifactLocation":{{"uri":"{abs_uri}"}},"region":{{"startLine":1}}}}}}]}},
          {{"level":"note","message":{{"text":"no location"}}}}
        ]}}]}}"#
    );
    let d = run(DiagnosticsFormat::Sarif, Stream::Stdout, &doc);
    assert_eq!(d.len(), 3, "{d:#?}");
    assert_eq!(
        (d[0].severity, d[0].code.as_deref(), d[0].source.as_str()),
        (
            Severity::Warning,
            Some("clippy::needless_return"),
            "clippy-sarif"
        )
    );
    let l = d[0].location.as_ref().unwrap();
    assert_eq!(
        (
            l.file.0.as_str(),
            l.line,
            l.column,
            l.end_line,
            l.end_column
        ),
        ("src/main.rs", 4, Some(5), Some(4), Some(14))
    );
    assert_eq!(at(&d[1].location), Some(loc("src/lib x.rs", 1, None)));
    assert_eq!((d[2].severity, &d[2].location), (Severity::Info, &None));
    assert!(run(DiagnosticsFormat::Sarif, Stream::Stdout, "{not json").is_empty());
}

#[test]
fn paths_outside_workspace_dropped() {
    let r = roots();
    assert_eq!(
        workspace_path("src/main.rs", &r),
        Some(WorkspacePath("src/main.rs".into()))
    );
    assert_eq!(
        workspace_path("./src/../lib.rs", &r),
        Some(WorkspacePath("lib.rs".into()))
    );
    assert_eq!(workspace_path("../other/x.rs", &r), None);
    assert_eq!(workspace_path("<built-in>", &r), None);
    let outside = if cfg!(windows) {
        r"C:\Users\me\.cargo\registry\src\x.rs"
    } else {
        "/home/me/.cargo/registry/src/x.rs"
    };
    assert_eq!(workspace_path(outside, &r), None);
    // The diagnostic is kept (text intact); only its location goes.
    let line = format!("{outside}:3:1: error: from a dependency");
    let d = run(DiagnosticsFormat::GccText, Stream::Stderr, &line);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].location, None);
    assert_eq!(d[0].message, "from a dependency");
    // A cargo span in a dependency: location dropped, message kept.
    let json = r#"{"reason":"compiler-message","message":{"message":"dep warning","code":null,"level":"warning","spans":[{"file_name":"/home/me/.cargo/registry/src/dep/lib.rs","line_start":1,"line_end":1,"column_start":1,"column_end":2,"is_primary":true,"label":null}],"children":[],"rendered":"warning: dep warning"}}"#;
    let d = run(DiagnosticsFormat::CargoJson, Stream::Stdout, json);
    assert_eq!((d.len(), &d[0].location), (1, &None));
}

/// A few hundred random and mangled inputs through every parser: none may panic.
#[test]
fn parsers_do_not_panic_on_garbage() {
    let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let alphabet: Vec<char> =
        "{}[]\":,0123456789abc:/\\.%-\n \u{1b}é∑\u{0}-Wfile:line:error:note:warning:"
            .chars()
            .collect();
    let samples = [fixture("cargo_e0308.jsonl"), fixture("gcc_too_few_args.txt"), r#"{"runs":[{"results":[{"message":{"text":"x"},"locations":[{"physicalLocation":{"artifactLocation":{"uri":"file:///%"},"region":{"startLine":-1}}}]}]}]}"#.to_string()];
    let formats = [
        DiagnosticsFormat::CargoJson,
        DiagnosticsFormat::GccText,
        DiagnosticsFormat::Sarif,
        DiagnosticsFormat::Regex {
            pattern: r"(?P<file>\S+):(?P<line>\d+): (?P<message>.*)".into(),
        },
    ];
    for i in 0..400 {
        let mut text: String = if i % 2 == 0 {
            (0..(next() % 400))
                .map(|_| alphabet[(next() as usize) % alphabet.len()])
                .collect()
        } else {
            // Truncate and splice a real sample at random places.
            let s = &samples[(next() as usize) % samples.len()];
            let chars: Vec<char> = s.chars().collect();
            let a = (next() as usize) % chars.len().max(1);
            let b = a + (next() as usize) % (chars.len() - a).max(1);
            chars[a..b].iter().collect()
        };
        if i % 7 == 0 {
            text.push_str(":99999999999999999999:-1: error: big");
        }
        for f in &formats {
            for s in [Stream::Stdout, Stream::Stderr] {
                let _ = run(f.clone(), s, &text);
            }
        }
    }
}
