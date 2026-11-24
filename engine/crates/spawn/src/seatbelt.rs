//! macOS: a Seatbelt profile. Closed by default; what is opened is named one by one. Paths travel as
//! parameters, never inside the profile's text. Probed on macOS 27.0 (tests/macos.rs).

use std::{ffi::OsString, fmt::Write};

use axiom_policy::{Place, paths};

use crate::{Job, Real};

const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// Of the system, beside the profile's places: what every program needs to start. No Mach service
/// is open: none of those probed was needed, and several lead out (LaunchServices, the keychain,
/// Apple events). No socket either, so names cannot be resolved.
const BASE: &str = r#"(version 1)
(deny default)
(allow process-fork)
(allow signal (target same-sandbox))
(allow process-info* (target same-sandbox))
(allow sysctl-read
  (sysctl-name-prefix "hw.")
  (sysctl-name-prefix "machdep.cpu.")
  (sysctl-name "kern.argmax" "kern.bootargs" "kern.hostname" "kern.maxfilesperproc" "kern.maxproc"
    "kern.osproductversion" "kern.osrelease" "kern.ostype" "kern.osversion" "kern.osvariant_status"
    "kern.secure_kernel" "kern.usrstack64" "kern.version" "kern.waketime" "kern.ngroups"
    "kern.iossupportversion" "sysctl.proc_cputype" "security.mac.lockdown_mode_state" "vm.loadavg"))
(allow file-read* (literal "/") (literal "/dev/null") (literal "/dev/zero") (literal "/dev/random")
  (literal "/dev/urandom") (literal "/dev/dtracehelper") (subpath "/dev/fd"))
(allow file-write-data file-ioctl (literal "/dev/null") (literal "/dev/zero") (literal "/dev/dtracehelper"))
(allow ipc-posix-sem)
(allow mach-lookup (global-name "com.apple.system.opendirectoryd.libinfo"))
"#;

pub fn wrap(job: &Job, real: &Real) -> (OsString, Vec<OsString>) {
    let (text, params) = profile(real);
    let mut args: Vec<OsString> = vec!["-p".into(), text.into()];
    for (k, v) in params {
        args.push("-D".into());
        args.push(format!("{k}={v}").into());
    }
    args.push(job.program.as_path().into());
    args.extend(job.args.iter().map(OsString::from));
    (SANDBOX_EXEC.into(), args)
}

fn profile(real: &Real) -> (String, Vec<(String, String)>) {
    let mut s = BASE.to_owned();
    let mut params = Vec::new();
    let mut name = |p: &Place| {
        let k = format!("P{}", params.len());
        params.push((k.clone(), p.to_string()));
        k
    };
    let mut rule = |s: &mut String, what: &str, places: &[Place], how: &str| {
        for p in places {
            let k = name(p);
            let _ = writeln!(s, "({what} ({how} (param \"{k}\")))");
        }
    };
    let up: std::collections::BTreeSet<Place> =
        real.read.iter().flat_map(|p| std::iter::successors(p.parent(), Place::parent)).collect();
    rule(&mut s, "allow file-read-metadata", &up.into_iter().collect::<Vec<_>>(), "literal");
    rule(&mut s, "allow process-exec file-read* file-map-executable", &real.read, "subpath");
    rule(&mut s, "allow file-write*", &real.write, "subpath");
    // The last that matches decides.
    for (n, _) in paths::PROTECTED {
        let _ = writeln!(s, "(deny file-write* (regex #\"/{}(/|$)\"))", any_case(n));
    }
    for (n, _) in paths::PROTECTED_ENDINGS {
        let _ = writeln!(s, "(deny file-write* (regex #\"{}(/|$)\"))", any_case(n));
    }
    s.push_str("(deny file-write-setugid)\n(deny file-mknod)\n");
    rule(
        &mut s,
        "deny file-read* file-write* process-exec file-map-executable",
        &real.hidden,
        "subpath",
    );
    (s, params)
}

/// The name as a regular expression for every spelling that APFS takes for it. Besides the two
/// cases of a letter these are the long s and the Kelvin sign.
fn any_case(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        match c {
            's' => out.push_str("([sS]|\u{17F})"),
            'k' => out.push_str("([kK]|\u{212A})"),
            c if c.is_ascii_lowercase() => {
                let _ = write!(out, "[{c}{}]", c.to_ascii_uppercase());
            }
            c if c.is_ascii_alphanumeric() || c == '_' || c == '-' => out.push(c),
            c => {
                out.push('\\');
                out.push(c);
            }
        }
    }
    out
}
