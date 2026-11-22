//! Parsing and validation of `provider.toml` beyond what serde enforces (`providers.md`).

use axiom_protocol::provider::{Checksum, InstallMethod, ProviderManifest, VersionSource};
use regex::Regex;

/// The only manifest schema this build understands.
pub const SCHEMA: &str = "axiom.provider/1";

/// Template variables a manifest may use (`providers.md` §Template variables).
pub const TEMPLATE_VARS: [&str; 3] = ["version", "root", "workspace"];

/// Parses and validates a manifest. Every problem found is returned, not only the first.
pub fn parse(text: &str) -> Result<ProviderManifest, Vec<String>> {
    let m: ProviderManifest = toml::from_str(text).map_err(|e| vec![one_line(&e.to_string())])?;
    let errors = validate(&m);
    if errors.is_empty() {
        Ok(m)
    } else {
        Err(errors)
    }
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Checks the rules serde cannot express. Empty = valid.
pub fn validate(m: &ProviderManifest) -> Vec<String> {
    let mut errors = Vec::new();
    if m.schema != SCHEMA {
        errors.push(format!("schema must be \"{SCHEMA}\", not \"{}\"", m.schema));
    }
    if !valid_id(&m.id) {
        errors.push(format!(
            "id \"{}\" must be lowercase letters, digits and '-', starting with a letter or digit",
            m.id
        ));
    }
    if m.process.is_some() {
        errors.push("helper processes (`process`) are not supported yet".into());
    }

    let mut tool_ids = Vec::new();
    for t in &m.tools {
        if tool_ids.contains(&t.id.as_str()) {
            errors.push(format!("tool id \"{}\" is used twice", t.id));
        }
        tool_ids.push(&t.id);
        if t.program.is_empty() || t.program.contains(['/', '\\']) {
            errors.push(format!(
                "tool \"{}\": program must be a bare executable name",
                t.id
            ));
        }
        if let Err(e) = version_regex(&t.version_regex) {
            errors.push(format!("tool \"{}\": {e}", t.id));
        }
    }

    let mut task_ids = Vec::new();
    for t in &m.tasks {
        if task_ids.contains(&t.id.as_str()) {
            errors.push(format!("task id \"{}\" is used twice", t.id));
        }
        task_ids.push(&t.id);
        if t.command.is_empty() {
            errors.push(format!("task \"{}\": command is empty", t.id));
        }
        for s in &t.command {
            check_template(s, &format!("task \"{}\" command", t.id), &mut errors);
        }
    }

    for (i, method) in m.install.iter().enumerate() {
        let at = format!("install[{i}]");
        match method {
            InstallMethod::Archive {
                versions,
                artifacts,
                bin_dirs,
                env,
                provides,
                setup,
            } => {
                for t in provides {
                    if !m.tools.iter().any(|x| &x.id == t) {
                        errors.push(format!(
                            "{at}: provides \"{t}\", which is not one of the tools"
                        ));
                    }
                }
                for s in setup {
                    check_template(s, &format!("{at} setup"), &mut errors);
                }
                match versions {
                    VersionSource::Static { versions } if versions.is_empty() => {
                        errors.push(format!("{at}: the static version list is empty"));
                    }
                    VersionSource::Json { url, .. } => check_template(url, &at, &mut errors),
                    _ => {}
                }
                if artifacts.is_empty() {
                    errors.push(format!(
                        "{at}: an archive install needs at least one artifact"
                    ));
                }
                for a in artifacts {
                    check_template(&a.url, &format!("{at} artifact url"), &mut errors);
                    use axiom_protocol::provider::ArchiveFormat as F;
                    if let Some(n) = &a.file_name {
                        if !matches!(a.format, F::File | F::Gz)
                            || n.is_empty()
                            || n.contains(['/', '\\'])
                            || n.contains("${")
                            || n.starts_with('.')
                        {
                            errors.push(format!(
                                "{at}: `file_name` is a plain name, for \"file\" and \"gz\" artifacts"
                            ));
                        }
                    }
                    if matches!(a.format, F::File | F::Gz) {
                        if a.strip_components != 0 {
                            errors.push(format!(
                                "{at}: a \"file\" artifact has no components to strip"
                            ));
                        }
                        let name = a.url.rsplit('/').next().unwrap_or_default();
                        if a.file_name.is_none()
                            && (name.is_empty() || name.contains("${") || name.starts_with('.'))
                        {
                            errors.push(format!(
                                "{at}: a \"file\" artifact URL must end in a fixed file name"
                            ));
                        }
                    }
                    match &a.checksum {
                        Checksum::Sha256 { values } => {
                            if values.is_empty() {
                                errors.push(format!("{at}: artifact {} has no checksum", a.url));
                            }
                            for (v, d) in values {
                                if !is_sha256_hex(d) {
                                    errors.push(format!(
                                        "{at}: checksum for {v} is not 64 lowercase hex digits"
                                    ));
                                }
                            }
                        }
                        Checksum::Sha256Url { url } => {
                            if url.is_empty() {
                                errors.push(format!("{at}: artifact {} has no checksum", a.url));
                            }
                            check_template(url, &format!("{at} checksum url"), &mut errors);
                        }
                    }
                }
                check_bin_dirs(bin_dirs, &at, &mut errors);
                for (k, v) in env {
                    check_template(v, &format!("{at} env {k}"), &mut errors);
                }
            }
            InstallMethod::Delegate {
                using,
                install,
                uninstall,
                list_versions,
                env,
                bin_dirs,
                versions,
                ..
            } => {
                let program = m.tools.iter().find(|t| &t.id == using).map(|t| &t.program);
                if program.is_none() {
                    errors.push(format!(
                        "{at}: `using = \"{using}\"` is not one of the tools"
                    ));
                }
                // A delegate method can only ever run its delegate.
                for (what, argv) in [
                    ("install", install),
                    ("uninstall", uninstall),
                    ("list_versions", list_versions),
                ] {
                    if let (Some(first), Some(p)) = (argv.first(), program) {
                        if first != p {
                            errors.push(format!("{at}: {what} must start with \"{p}\""));
                        }
                    }
                }
                check_bin_dirs(bin_dirs, &at, &mut errors);
                if let Some(VersionSource::Json { url, .. }) = versions {
                    check_template(url, &at, &mut errors);
                }
                for (what, argv) in [
                    ("install", install),
                    ("uninstall", uninstall),
                    ("list_versions", list_versions),
                ] {
                    for s in argv {
                        check_template(s, &format!("{at} {what}"), &mut errors);
                    }
                }
                if install.is_empty() || list_versions.is_empty() {
                    errors.push(format!("{at}: install and list_versions must not be empty"));
                }
                for (k, v) in env {
                    check_template(v, &format!("{at} env {k}"), &mut errors);
                }
            }
            InstallMethod::SystemPackage { packages } => {
                if packages.values().all(Vec::is_empty) {
                    errors.push(format!("{at}: no packages listed"));
                }
            }
        }
    }
    errors
}

fn check_bin_dirs(dirs: &[String], at: &str, errors: &mut Vec<String>) {
    for d in dirs {
        if d.starts_with(['/', '\\']) || d.contains(':') || d.split(['/', '\\']).any(|c| c == "..")
        {
            errors.push(format!("{at}: bin_dir \"{d}\" must stay inside the root"));
        }
    }
}

/// `[a-z0-9][a-z0-9-]*`.
pub fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Compiles a `version_regex`; it must have exactly one capture group.
pub fn version_regex(pattern: &str) -> Result<Regex, String> {
    let re =
        Regex::new(pattern).map_err(|e| format!("version_regex: {}", one_line(&e.to_string())))?;
    match re.captures_len() - 1 {
        1 => Ok(re),
        n => Err(format!(
            "version_regex must have exactly one capture group, it has {n}"
        )),
    }
}

/// Expands `${version}` and `${root}` (the only variables available outside a workspace).
pub fn expand(s: &str, version: &str, root: &str) -> String {
    s.replace("${version}", version).replace("${root}", root)
}

/// The `${name}` variables a string uses, or an error for a malformed `${`.
pub fn template_vars(s: &str) -> Result<Vec<&str>, String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find("${") {
        let after = &rest[i + 2..];
        let Some(end) = after.find('}') else {
            return Err(format!("unterminated `${{` in \"{s}\""));
        };
        out.push(&after[..end]);
        rest = &after[end + 1..];
    }
    Ok(out)
}

fn check_template(s: &str, at: &str, errors: &mut Vec<String>) {
    match template_vars(s) {
        Err(e) => errors.push(format!("{at}: {e}")),
        Ok(vars) => {
            for v in vars {
                if !TEMPLATE_VARS.contains(&v) {
                    errors.push(format!(
                        "{at}: unknown template variable `${{{v}}}` (allowed: version, root, workspace)"
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: &str = r#"schema = "axiom.provider/1"
id = "demo"
name = "Demo"
description = "d"
version = "0.1.0"
"#;

    #[test]
    fn regex_one_group() {
        assert!(version_regex(r"v(\d+)").is_ok());
        assert!(version_regex(r"v\d+")
            .unwrap_err()
            .contains("exactly one capture group, it has 0"));
        assert!(version_regex(r"(\d+)\.(\d+)")
            .unwrap_err()
            .contains("it has 2"));
        assert!(
            version_regex(r"(?:x)(\d+)").is_ok(),
            "non-capturing groups do not count"
        );
        assert!(version_regex(r"(")
            .unwrap_err()
            .starts_with("version_regex:"));
        let text = format!(
            "{MIN}[[tools]]\nid = \"t\"\nprogram = \"t\"\nversion_regex = 't (\\d+)\\.(\\d+)'\n"
        );
        let errs = parse(&text).unwrap_err();
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs[0].starts_with("tool \"t\""), "{errs:?}");
    }

    #[test]
    fn template_vars() {
        assert_eq!(
            super::template_vars("${root}/bin/${version}").unwrap(),
            ["root", "version"]
        );
        assert!(super::template_vars("${root").is_err());
        let text = format!(
            "{MIN}[[tools]]\nid = \"t\"\nprogram = \"t\"\nversion_regex = '(x)'\n\
             [[tasks]]\nid = \"b\"\nkind = \"build\"\nlabel = \"b\"\ncommand = [\"t\", \"${{home}}\", \"${{workspace}}\"]\n"
        );
        let errs = parse(&text).unwrap_err();
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(
            errs[0].contains("unknown template variable `${home}`"),
            "{errs:?}"
        );
    }

    #[test]
    fn ids_schema_and_helpers() {
        assert!(valid_id("apple-xtool") && valid_id("c99") && !valid_id("Rust") && !valid_id("-x"));
        let text = MIN
            .replace("axiom.provider/1", "axiom.provider/2")
            .replace("\"demo\"", "\"De mo\"")
            + "[process]\ncommand = [\"helper\"]\n";
        let errs = parse(&text).unwrap_err();
        assert_eq!(errs.len(), 3, "{errs:?}");
    }

    #[test]
    fn archive_needs_valid_checksums() {
        let text = format!(
            "{MIN}[[install]]\nkind = \"archive\"\nversions = {{ kind = \"static\", versions = [\"1\"] }}\n\
             bin_dirs = [\"../escape\"]\n\
             [[install.artifacts]]\nplatform = {{ os = \"linux\" }}\nurl = \"https://x/${{version}}.tar.gz\"\nformat = \"tar.gz\"\n\
             checksum = {{ kind = \"sha256\", values = {{ \"1\" = \"ABC\" }} }}\n"
        );
        let errs = parse(&text).unwrap_err();
        assert_eq!(errs.len(), 2, "{errs:?}");
        assert!(errs.iter().any(|e| e.contains("64 lowercase hex")));
        assert!(errs.iter().any(|e| e.contains("inside the root")));
    }

    #[test]
    fn serde_errors_are_one_line() {
        let errs = parse("schema = \"axiom.provider/1\"\nid = 3\n").unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(!errs[0].contains('\n'));
    }
}
