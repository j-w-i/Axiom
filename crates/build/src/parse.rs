//! Diagnostics parsers: pure functions from tool output to normalised [`Diagnostic`]s. No I/O.
//!
//! A [`Parser`] is fed output line by line (per stream) and returns the diagnostics each line
//! completes; [`Parser::finish`] returns the rest (and the whole SARIF document's results).
//! Every location is made workspace-relative; a path outside the workspace drops the
//! location (the diagnostic stays, its text intact) so a diagnostic never points outside.

use axiom_protocol::diagnostics::{Diagnostic, Location, RelatedInfo, Severity};
use axiom_protocol::provider::DiagnosticsFormat;
use axiom_protocol::task::Stream;
use axiom_protocol::WorkspacePath;
use regex::Regex;
use serde_json::Value;
use std::path::{Component, Path, PathBuf};

/// Parser state for one run.
pub struct Parser {
    kind: Kind,
    roots: Vec<PathBuf>,
    /// A diagnostic still collecting continuation lines (gcc notes).
    pending: Option<Diagnostic>,
    /// SARIF: the whole stdout.
    document: String,
}

enum Kind {
    None,
    CargoJson,
    GccText(Regex),
    Regex(Option<Regex>),
    Sarif,
}

/// `file:line[:col]: severity: message`, with an optional Windows drive letter.
const GCC: &str = r"^(?P<file>(?:[A-Za-z]:)?[^:]+?):(?P<line>\d+):(?:(?P<column>\d+):)?\s*(?P<severity>fatal error|error|warning|note|remark):\s*(?P<message>.*)$";

impl Parser {
    /// `roots`: the workspace root as the tool may spell it (as opened, and canonical); the
    /// task's working directory is the first.
    pub fn new(format: &DiagnosticsFormat, roots: Vec<PathBuf>) -> Parser {
        let kind = match format {
            DiagnosticsFormat::None => Kind::None,
            DiagnosticsFormat::CargoJson => Kind::CargoJson,
            DiagnosticsFormat::GccText => match Regex::new(GCC) {
                Ok(r) => Kind::GccText(r),
                Err(_) => Kind::None,
            },
            DiagnosticsFormat::Sarif => Kind::Sarif,
            DiagnosticsFormat::Regex { pattern } => Kind::Regex(Regex::new(pattern).ok()),
        };
        Parser {
            kind,
            roots,
            pending: None,
            document: String::new(),
        }
    }

    /// One complete output line (without its line ending).
    pub fn line(&mut self, stream: Stream, line: &str) -> Vec<Diagnostic> {
        let line = line.strip_suffix('\r').unwrap_or(line);
        match &self.kind {
            Kind::None => Vec::new(),
            Kind::CargoJson => {
                if stream == Stream::Stdout {
                    cargo_line(line, &self.roots).into_iter().collect()
                } else {
                    Vec::new()
                }
            }
            Kind::GccText(re) => {
                let re = re.clone();
                self.gcc_line(&re, line)
            }
            Kind::Regex(Some(re)) => regex_line(re, line, &self.roots).into_iter().collect(),
            Kind::Regex(None) => Vec::new(),
            Kind::Sarif => {
                if stream == Stream::Stdout && self.document.len() < 64 << 20 {
                    self.document.push_str(line);
                    self.document.push('\n');
                }
                Vec::new()
            }
        }
    }

    /// End of output: what is still pending.
    pub fn finish(&mut self) -> Vec<Diagnostic> {
        let mut out: Vec<Diagnostic> = self.pending.take().into_iter().collect();
        if matches!(self.kind, Kind::Sarif) {
            out.extend(sarif(&std::mem::take(&mut self.document), &self.roots));
        }
        out
    }

    fn gcc_line(&mut self, re: &Regex, line: &str) -> Vec<Diagnostic> {
        let Some(c) = re.captures(line) else {
            // Source excerpts, carets and "In file included from" lines belong to the
            // diagnostic being collected; they are in its `rendered` text.
            if let Some(p) = &mut self.pending {
                let r = p.rendered.get_or_insert_with(String::new);
                if r.len() < 16 * 1024 {
                    r.push('\n');
                    r.push_str(line);
                }
            }
            return Vec::new();
        };
        let location = location(
            &c["file"],
            c["line"].parse().ok(),
            c.name("column").and_then(|m| m.as_str().parse().ok()),
            None,
            None,
            &self.roots,
        );
        let (message, code) = split_gcc_code(c["message"].trim());
        if &c["severity"] == "note" {
            if let Some(p) = &mut self.pending {
                match location {
                    Some(location) => p.related.push(RelatedInfo { message, location }),
                    None => {
                        let r = p.rendered.get_or_insert_with(String::new);
                        r.push('\n');
                        r.push_str(line);
                    }
                }
                return Vec::new();
            }
        }
        let severity = match &c["severity"] {
            "fatal error" | "error" => Severity::Error,
            "warning" => Severity::Warning,
            "note" => Severity::Info,
            _ => Severity::Hint,
        };
        let next = Diagnostic {
            severity,
            message,
            code,
            source: "cc".into(),
            location,
            related: Vec::new(),
            rendered: Some(line.to_string()),
        };
        self.pending.replace(next).into_iter().collect()
    }
}

/// `message [-Wflag]` → (`message`, `-Wflag`).
fn split_gcc_code(msg: &str) -> (String, Option<String>) {
    if let Some(open) = msg.rfind(" [") {
        if msg.ends_with(']') {
            let code = &msg[open + 2..msg.len() - 1];
            if code.starts_with('-') && !code.contains(' ') {
                return (msg[..open].to_string(), Some(code.to_string()));
            }
        }
    }
    (msg.to_string(), None)
}

/// Makes a tool's path workspace-relative. `roots` are the spellings of the workspace root
/// the tool may use (as opened, and canonical). `None` = outside the workspace, or unusable.
pub fn workspace_path(file: &str, roots: &[PathBuf]) -> Option<WorkspacePath> {
    let file = file.trim();
    if file.is_empty() || file.starts_with('<') {
        return None; // `<built-in>`, `<command-line>`
    }
    let p = Path::new(file);
    let rel: PathBuf = if p.is_absolute() || file.starts_with('/') || file.starts_with('\\') {
        roots
            .iter()
            .find_map(|r| p.strip_prefix(r).ok())?
            .to_path_buf()
    } else {
        p.to_path_buf()
    };
    let mut parts: Vec<String> = Vec::new();
    for c in rel.components() {
        match c {
            Component::Normal(s) => parts.push(s.to_str()?.to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    if parts.is_empty() {
        return None;
    }
    let wp = WorkspacePath(parts.join("/"));
    wp.is_lexically_valid().then_some(wp)
}

fn location(
    file: &str,
    line: Option<u32>,
    column: Option<u32>,
    end_line: Option<u32>,
    end_column: Option<u32>,
    root: &[PathBuf],
) -> Option<Location> {
    let line = line.filter(|l| *l > 0)?;
    Some(Location {
        file: workspace_path(file, root)?,
        line,
        column: column.filter(|c| *c > 0),
        end_line,
        end_column,
    })
}

/// Removes ANSI escape sequences (cargo's `rendered` is coloured with the `-ansi` format).
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for d in chars.by_ref() {
                    if ('@'..='~').contains(&d) {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn cargo_severity(level: &str) -> Option<Severity> {
    Some(match level {
        "error" | "error: internal compiler error" => Severity::Error,
        "warning" => Severity::Warning,
        "note" => Severity::Info,
        "help" => Severity::Hint,
        _ => return None, // failure-note and anything newer
    })
}

fn u32_of(v: &Value) -> Option<u32> {
    v.as_u64().and_then(|n| u32::try_from(n).ok())
}

fn cargo_span_location(span: &Value, root: &[PathBuf]) -> Option<Location> {
    location(
        span["file_name"].as_str()?,
        u32_of(&span["line_start"]),
        u32_of(&span["column_start"]),
        u32_of(&span["line_end"]),
        u32_of(&span["column_end"]),
        root,
    )
}

/// One line of `cargo --message-format=json*`.
pub fn cargo_line(line: &str, root: &[PathBuf]) -> Option<Diagnostic> {
    if !line.starts_with('{') {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    if v["reason"] != "compiler-message" {
        return None;
    }
    let msg = &v["message"];
    let text = msg["message"].as_str()?;
    if text.starts_with("aborting due to") {
        return None;
    }
    let severity = cargo_severity(msg["level"].as_str()?)?;
    let spans = msg["spans"].as_array().cloned().unwrap_or_default();
    let primary = spans
        .iter()
        .find(|s| s["is_primary"].as_bool() == Some(true))
        .or_else(|| spans.first());
    let mut related = Vec::new();
    for s in &spans {
        if std::ptr::eq(s, primary.unwrap_or(&Value::Null)) {
            continue;
        }
        if let (Some(label), Some(location)) = (s["label"].as_str(), cargo_span_location(s, root)) {
            related.push(RelatedInfo {
                message: label.to_string(),
                location,
            });
        }
    }
    for child in msg["children"].as_array().into_iter().flatten() {
        let Some(m) = child["message"].as_str() else {
            continue;
        };
        let loc = child["spans"]
            .as_array()
            .and_then(|s| s.first())
            .and_then(|s| cargo_span_location(s, root));
        if let Some(location) = loc {
            related.push(RelatedInfo {
                message: m.to_string(),
                location,
            });
        }
    }
    Some(Diagnostic {
        severity,
        message: text.to_string(),
        code: msg["code"]["code"].as_str().map(String::from),
        source: "rustc".into(),
        location: primary.and_then(|s| cargo_span_location(s, root)),
        related,
        rendered: msg["rendered"].as_str().map(strip_ansi),
    })
}

/// One line against a manifest's regex (named groups `file`, `line`, optional `column`,
/// `severity`, `code`, `message`).
pub fn regex_line(re: &Regex, line: &str, root: &[PathBuf]) -> Option<Diagnostic> {
    let c = re.captures(line)?;
    let get = |n: &str| c.name(n).map(|m| m.as_str());
    let severity = match get("severity").map(str::to_ascii_lowercase).as_deref() {
        Some("warning" | "warn") => Severity::Warning,
        Some("info" | "note") => Severity::Info,
        Some("hint" | "help") => Severity::Hint,
        _ => Severity::Error,
    };
    Some(Diagnostic {
        severity,
        message: get("message").unwrap_or(line).trim().to_string(),
        code: get("code").map(String::from),
        source: "regex".into(),
        location: get("file").and_then(|f| {
            location(
                f,
                get("line").and_then(|l| l.parse().ok()),
                get("column").and_then(|l| l.parse().ok()),
                None,
                None,
                root,
            )
        }),
        related: Vec::new(),
        rendered: Some(line.to_string()),
    })
}

/// SARIF 2.1.0 results.
pub fn sarif(document: &str, root: &[PathBuf]) -> Vec<Diagnostic> {
    let Ok(v) = serde_json::from_str::<Value>(document) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for run in v["runs"].as_array().into_iter().flatten() {
        let tool = run["tool"]["driver"]["name"]
            .as_str()
            .unwrap_or("sarif")
            .to_string();
        for r in run["results"].as_array().into_iter().flatten() {
            let Some(message) = r["message"]["text"].as_str() else {
                continue;
            };
            let severity = match r["level"].as_str().unwrap_or("warning") {
                "error" => Severity::Error,
                "warning" => Severity::Warning,
                "note" => Severity::Info,
                _ => Severity::Hint,
            };
            let phys = &r["locations"][0]["physicalLocation"];
            let region = &phys["region"];
            let loc = phys["artifactLocation"]["uri"].as_str().and_then(|u| {
                location(
                    &uri_to_path(u),
                    u32_of(&region["startLine"]),
                    u32_of(&region["startColumn"]),
                    u32_of(&region["endLine"]),
                    u32_of(&region["endColumn"]),
                    root,
                )
            });
            out.push(Diagnostic {
                severity,
                message: message.to_string(),
                code: r["ruleId"].as_str().map(String::from),
                source: tool.clone(),
                location: loc,
                related: Vec::new(),
                rendered: None,
            });
        }
    }
    out
}

/// `file:///a/b%20c` → `/a/b c`; `file:///C:/x` → `C:/x`; relative URIs unchanged.
fn uri_to_path(uri: &str) -> String {
    let rest = match uri.strip_prefix("file://") {
        Some(r) => {
            let r = r.strip_prefix("localhost").unwrap_or(r);
            // Windows drive: `/C:/x`.
            let b = r.as_bytes();
            if b.len() > 3 && b[0] == b'/' && b[2] == b':' && b[1].is_ascii_alphabetic() {
                &r[1..]
            } else {
                r
            }
        }
        None => uri,
    };
    percent_decode(rest)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(h) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(h);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
