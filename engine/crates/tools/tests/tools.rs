//! Tools as an engine uses them: planned, then done.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use axiom_policy::{
    Access, Budget, Caps, Decision, Keep, Net, Place, Policy, Reach, Tools, rules::Source,
};
use axiom_tools::{Output, Plan, Seen, Site, plan, schema_digest, specs, system};
use serde_json::{Value, json};

struct T {
    _d: tempfile::TempDir,
    root: PathBuf,
    site: Site,
    seen: Seen,
}

fn t(access: Access) -> T {
    let d = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(d.path()).unwrap();
    for x in ["home/.ssh", "home/w/src", "home/w/.git", "own", "out", "tmp"] {
        std::fs::create_dir_all(root.join(x)).unwrap();
    }
    for (f, c) in [
        ("home/.ssh/id", "SECRET"),
        ("own/token", "SECRET"),
        ("out/f", "outside"),
        ("home/w/a.txt", "one\ntwo\nthree\n"),
        ("home/w/src/b.rs", "fn main() {}\n"),
        ("home/w/.git/config", "orig"),
        ("home/w/bin", "a\0b"),
    ] {
        std::fs::write(root.join(f), c).unwrap();
    }
    let p = |s: &str| Place::new(root.join(s)).unwrap();
    let home = p("home");
    let (path, system) = system(&home);
    let site = Site {
        workspace: p("home/w"),
        caps: Caps {
            read: Reach::of([p("home/w"), p("tmp")]),
            write: Reach::of([p("home/w")]),
            net: Net::none(),
            run: true,
            tools: Tools::All,
            budget: Budget::default(),
        },
        keep: Keep::new(home, vec![p("own")]),
        access,
        policy: Policy::default(),
        system,
        scratch: p("tmp"),
        path,
        chain: vec![],
    };
    T { _d: d, site, seen: Seen::default(), root }
}

impl T {
    fn plan(&self, tool: &str, v: Value) -> Plan {
        plan(tool, &v, &self.site, &self.seen)
    }
    /// Planned and done, as if a person said yes to everything.
    async fn run(&self, tool: &str, v: Value) -> Output {
        match self.plan(tool, v) {
            Plan::No(why) => Output { ok: false, text: format!("NO {why}"), detail: Value::Null },
            Plan::Go(s, _) | Plan::Ask(_, s, _) => s.run(&self.site, &self.seen, |_| {}).await,
        }
    }
    fn file(&self, s: &str) -> Option<String> {
        std::fs::read_to_string(self.root.join("home/w").join(s)).ok()
    }
    fn decision(&self, tool: &str, v: Value) -> Decision {
        match self.plan(tool, v) {
            Plan::No(_) => Decision::Deny,
            Plan::Go(..) => Decision::Allow,
            Plan::Ask(..) => Decision::Ask,
        }
    }
}

#[tokio::test]
async fn reading() {
    let t = t(Access::Ask);
    let o = t.run("read", json!({"path": "a.txt"})).await;
    assert_eq!((o.ok, o.text.as_str()), (true, "     1\tone\n     2\ttwo\n     3\tthree\n"));
    let o = t.run("read", json!({"path": "a.txt", "from": 2, "lines": 1})).await;
    assert!(o.text.starts_with("     2\ttwo\n(1 more"));
    for (p, says) in [
        ("none", "does not exist"),
        ("bin", "not text"),
        ("src", "not a plain file"),
        ("../.ssh/id", "kept"),
        ("../.SSH/id", "kept"),
        ("../../own/token", "kept"),
        ("../../own/none", "kept"),
        ("../../out/f", "not inside"),
        ("/etc/hosts", "not inside"),
        ("", "not a plain file"),
        ("a\nb", "character"),
    ] {
        let o = t.run("read", json!({ "path": p })).await;
        assert!(!o.ok && o.text.contains(says) && !o.text.contains("SECRET"), "{p}: {o:?}");
    }
    std::os::unix::fs::symlink("../../.ssh/id", t.root.join("home/w/src/l")).unwrap();
    std::os::unix::fs::symlink(t.root.join("own/token"), t.root.join("home/w/src/m")).unwrap();
    for p in ["src/l", "src/m"] {
        let o = t.run("read", json!({ "path": p })).await;
        assert!(!o.ok && !o.text.contains("SECRET"), "{o:?}");
    }
}

#[tokio::test]
async fn writing_and_editing() {
    let t = t(Access::Work);
    let o = t.run("write", json!({"path": "x/y/new.txt", "content": "hi\n"})).await;
    assert!(o.ok, "{o:?}");
    assert_eq!(t.file("x/y/new.txt").as_deref(), Some("hi\n"));
    assert!(o.detail["diff"].as_str().unwrap().contains("+hi"));
    // What it wrote it knows.
    assert!(t.run("edit", json!({"path": "x/y/new.txt", "old": "hi", "new": "ho"})).await.ok);
    // What it has not read it does not write over.
    for v in [json!({"path": "a.txt", "content": "x"})] {
        let o = t.run("write", v).await;
        assert!(o.text.contains("read a.txt before"), "{o:?}");
    }
    let o = t.run("edit", json!({"path": "a.txt", "old": "two", "new": "2"})).await;
    assert!(o.text.contains("read a.txt before"), "{o:?}");
    t.run("read", json!({"path": "a.txt"})).await;
    assert!(t.run("edit", json!({"path": "a.txt", "old": "two", "new": "2"})).await.ok);
    assert_eq!(t.file("a.txt").as_deref(), Some("one\n2\nthree\n"));
    // Changed by someone else.
    std::fs::write(t.root.join("home/w/a.txt"), "other\n").unwrap();
    let o = t.run("edit", json!({"path": "a.txt", "old": "other", "new": "x"})).await;
    assert!(o.text.contains("has changed since"), "{o:?}");
    assert_eq!(t.file("a.txt").as_deref(), Some("other\n"));
}

/// What a person said yes to is what is done, or nothing.
#[tokio::test]
async fn between_the_question_and_the_yes() {
    let t = t(Access::Ask);
    t.run("read", json!({"path": "a.txt"})).await;
    let Plan::Ask(ask, step, _) =
        t.plan("edit", json!({"path": "a.txt", "old": "two", "new": "2"}))
    else {
        panic!()
    };
    assert!(
        ask.subject.contains("-two\n+2\n")
            && ask.scope == "changes a.txt. Asked because this agent asks before each change.",
        "{ask:?}"
    );
    std::fs::write(t.root.join("home/w/a.txt"), "one\ntwo\nmore\n").unwrap();
    let o = step.run(&t.site, &t.seen, |_| {}).await;
    assert!(!o.ok && o.text.contains("changed meanwhile"), "{o:?}");
    assert_eq!(t.file("a.txt").as_deref(), Some("one\ntwo\nmore\n"));
    // A new file that someone else made meanwhile.
    let Plan::Ask(_, step, _) = t.plan("write", json!({"path": "n.txt", "content": "mine"})) else {
        panic!()
    };
    std::fs::write(t.root.join("home/w/n.txt"), "theirs").unwrap();
    assert!(!step.run(&t.site, &t.seen, |_| {}).await.ok);
    assert_eq!(t.file("n.txt").as_deref(), Some("theirs"));
    // A folder that became a link out.
    let Plan::Ask(_, step, _) = t.plan("write", json!({"path": "src/n/m.txt", "content": "mine"}))
    else {
        panic!()
    };
    std::fs::rename(t.root.join("home/w/src"), t.root.join("home/w/src2")).unwrap();
    std::os::unix::fs::symlink(t.root.join("out"), t.root.join("home/w/src")).unwrap();
    assert!(!step.run(&t.site, &t.seen, |_| {}).await.ok);
    assert_eq!(std::fs::read_dir(t.root.join("out")).unwrap().count(), 1);
}

#[tokio::test]
async fn where_nothing_is_written() {
    let t = t(Access::Work);
    for p in [
        ".git/config",
        ".git/hooks/x",
        ".GIT/x",
        ".vscode/tasks.json",
        "sub/.vscode/x",
        ".envrc",
        "../x",
        "../../out/f",
        "../../out/new",
        "/tmp/axiom-tools-test",
        "../.ssh/id",
        "../../own/token",
        "src/../../x",
        "../.zshrc",
    ] {
        let o = t.run("write", json!({"path": p, "content": "x"})).await;
        assert!(!o.ok, "{p}: {o:?}");
    }
    std::os::unix::fs::symlink("../../out", t.root.join("home/w/l")).unwrap();
    std::os::unix::fs::symlink(".git", t.root.join("home/w/g")).unwrap();
    std::os::unix::fs::symlink(".git/hooks", t.root.join("home/w/h")).unwrap();
    for p in ["l/new", "g/config", "g/new", "g/x/y", "h/pre-commit", "h"] {
        let o = t.run("write", json!({"path": p, "content": "x"})).await;
        assert!(!o.ok, "{p}: {o:?}");
    }
    assert_eq!(t.file(".git/config").as_deref(), Some("orig"));
    assert!(!t.root.join("home/w/.git/new").exists() && !t.root.join("home/w/.git/hooks").exists());
    assert_eq!(std::fs::read_dir(t.root.join("out")).unwrap().count(), 1);
    assert!(!Path::new("/tmp/axiom-tools-test").exists());
    let ro = self::t(Access::Read);
    assert!(!ro.run("write", json!({"path": "n", "content": "x"})).await.ok);
}

#[tokio::test]
async fn patches_are_whole() {
    let t = t(Access::Work);
    let o = t
        .run("patch", json!({"patch": "*** Begin Patch\n*** Update File: a.txt\n-two\n+2\n*** Add File: d/n.txt\n+new\n*** Update File: src/b.rs\n*** Move to: src/c.rs\n-fn main() {}\n+fn main() { }\n*** End Patch"}))
        .await;
    assert!(o.ok, "{o:?}");
    assert_eq!(t.file("a.txt").as_deref(), Some("one\n2\nthree\n"));
    assert_eq!(t.file("d/n.txt").as_deref(), Some("new\n"));
    assert_eq!(
        (t.file("src/b.rs"), t.file("src/c.rs").as_deref()),
        (None, Some("fn main() { }\n"))
    );
    for bad in [
        "*** Update File: a.txt\n-nothing\n+2\n*** Add File: z\n+x",
        "*** Add File: z\n+x\n*** Add File: .git/x\n+x",
        "*** Add File: z\n+x\n*** Add File: ../x\n+x",
        "*** Add File: z\n+x\n*** Add File: ./z\n+y",
        "*** Add File: z\n+x\n*** Add File: a.txt\n+y",
        "*** Add File: z\n+x\n*** Delete File: none",
        "*** Add File: z\n+x\n*** Delete File: src/c.rs",
    ] {
        let o = t
            .run("patch", json!({"patch": format!("*** Begin Patch\n{bad}\n*** End Patch")}))
            .await;
        if bad.ends_with("src/c.rs") {
            // Known: it wrote it.
            assert!(o.ok, "{o:?}");
            continue;
        }
        assert!(!o.ok, "{bad}: {o:?}");
        assert_eq!(t.file("z"), None, "{bad}");
    }
    // The second cannot be written: the first is put back.
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir(t.root.join("home/w/locked")).unwrap();
    std::fs::write(t.root.join("home/w/locked/f"), "x\n").unwrap();
    let Plan::Go(step, _) = t.plan("patch", json!({"patch": "*** Begin Patch\n*** Update File: a.txt\n-one\n+1\n*** Add File: first\n+x\n*** Update File: locked/f\n-x\n+y\n*** End Patch"})) else { panic!() };
    std::fs::set_permissions(t.root.join("home/w/locked"), std::fs::Permissions::from_mode(0o555))
        .unwrap();
    let o = step.run(&t.site, &t.seen, |_| {}).await;
    std::fs::set_permissions(t.root.join("home/w/locked"), std::fs::Permissions::from_mode(0o755))
        .unwrap();
    assert!(!o.ok && o.text.contains("nothing was changed"), "{o:?}");
    assert_eq!(t.file("a.txt").as_deref(), Some("one\n2\nthree\n"));
    assert_eq!(t.file("first"), None);
}

#[tokio::test]
async fn looking() {
    let t = t(Access::Ask);
    let o = t.run("list", json!({})).await;
    assert_eq!(o.text, ".git/\na.txt\nbin\nsrc/\n");
    assert_eq!(t.run("glob", json!({"pattern": "*.rs"})).await.text, "src/b.rs\n");
    assert!(t.run("search", json!({"pattern": "tw."})).await.text.starts_with("a.txt:2:two\n"));
    assert!(
        t.run("search", json!({"pattern": "SECRET", "path": ".."}))
            .await
            .text
            .contains("not inside")
    );
    let mut wide = t.site.clone();
    wide.caps.read = Reach::of([Place::new(&t.root).unwrap()]);
    let Plan::Go(s, _) =
        plan("search", &json!({"pattern": "SECRET|outside", "path": "../.."}), &wide, &t.seen)
    else {
        panic!()
    };
    let o = s.run(&wide, &t.seen, |_| {}).await;
    assert!(o.text.contains("out/f:1:outside") && !o.text.contains("SECRET"), "{o:?}");
}

#[tokio::test]
async fn commands() {
    let t = t(Access::Work);
    let o = t.run("shell", json!({"command": "echo hi; echo err >&2; cat a.txt | wc -l"})).await;
    assert!(
        o.ok && o.text.contains("hi\n") && o.text.contains("err\n") && o.text.contains('3'),
        "{o:?}"
    );
    let o = t.run("shell", json!({"command": "pwd", "cwd": "src"})).await;
    assert_eq!(o.text.trim(), t.root.join("home/w/src").display().to_string());
    let o = t.run("shell", json!({"command": "exit 3"})).await;
    assert!(!o.ok && o.text == "(exit 3)\n" && o.detail["exit"] == 3, "{o:?}");
    assert_eq!(t.run("shell", json!({"command": "true"})).await.text, "(no output)\n");
    let o = t.run("shell", json!({"command": "echo x > ../x"})).await;
    assert!(!o.ok && o.text.contains("sandbox may have"), "{o:?}");
    let o = t
        .run("shell", json!({"command": "printf '\\033[31mred\\033[0m\\n'; printf 'a\\rb\\n'"}))
        .await;
    assert_eq!(o.text, "red\nb\n");
    let o = t.run("shell", json!({"command": "seq 1 60000"})).await;
    assert!(
        o.text.starts_with("1\n2\n") && o.text.contains("\n60000\n") && o.text.len() < 60_000,
        "{}",
        o.text.len()
    );
    let f = o
        .text
        .lines()
        .find_map(|l| l.strip_prefix("(all of it: ")?.strip_suffix(')'))
        .unwrap()
        .to_owned();
    assert_eq!(std::fs::read_to_string(&f).unwrap().lines().count(), 60_000);
    let r = t.run("read", json!({"path": f, "from": 59_999})).await;
    assert!(r.text.contains("60000"), "it can be read on: {r:?}");
    let now = Instant::now();
    let o = t.run("shell", json!({"command": "echo a; sleep 60", "seconds": 1})).await;
    assert!(!o.ok && o.text.starts_with("a\n") && o.text.contains("too long"), "{o:?}");
    assert!(now.elapsed() < Duration::from_secs(20));
    // What it leaves running does not hold it up.
    let now = Instant::now();
    let o = t.run("shell", json!({"command": "(sleep 60; echo late) & echo now"})).await;
    assert!(o.text.starts_with("now") && now.elapsed() < Duration::from_secs(20), "{o:?}");
    for bad in [
        json!({"command": " "}),
        json!({"command": "ls", "cwd": "../.."}),
        json!({"command": "ls", "cwd": "a.txt"}),
        json!({}),
        json!({"command": "ls", "x": 1}),
    ] {
        assert!(!t.run("shell", bad.clone()).await.ok, "{bad}");
    }
    let left: Vec<_> =
        std::fs::read_dir(t.root.join("tmp")).unwrap().map(|e| e.unwrap().path()).collect();
    assert_eq!(left, [PathBuf::from(f)], "only what is needed stays");
}

#[tokio::test]
async fn output_is_heard_as_it_comes_and_dropping_ends_it() {
    let t = t(Access::Work);
    let m = format!("5{:05}", std::process::id() % 100_000);
    let Plan::Go(step, _) = t.plan("shell", json!({"command": format!("echo up; sleep {m}")}))
    else {
        panic!()
    };
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let (site, seen) = (t.site.clone(), t.seen.clone());
    let job = tokio::spawn(async move {
        step.run(&site, &seen, move |b| {
            let _ = tx.try_send(b.to_vec());
        })
        .await
    });
    let heard = tokio::time::timeout(Duration::from_secs(20), rx.recv()).await.unwrap().unwrap();
    assert_eq!(heard, b"up\n");
    job.abort();
    let now = Instant::now();
    loop {
        let o = std::process::Command::new("ps").args(["-axo", "args="]).output().unwrap();
        if !String::from_utf8_lossy(&o.stdout).lines().any(|l| l.ends_with(&format!("sleep {m}"))) {
            break;
        }
        assert!(now.elapsed() < Duration::from_secs(5), "it still runs");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn access_and_rules_decide() {
    use Decision::{Allow, Ask, Deny};
    let cases = [
        ("read", json!({"path": "a.txt"}), [Allow, Allow, Allow]),
        ("write", json!({"path": "n", "content": "x"}), [Deny, Ask, Allow]),
        ("write", json!({"path": "Makefile", "content": "x"}), [Deny, Ask, Ask]),
        ("shell", json!({"command": "ls"}), [Allow, Ask, Allow]),
        ("shell", json!({"command": "ls $(x)"}), [Ask, Ask, Ask]),
        ("nothing", json!({}), [Deny, Deny, Deny]),
    ];
    for (tool, v, want) in cases {
        for (a, w) in [Access::Read, Access::Ask, Access::Work].into_iter().zip(want) {
            assert_eq!(t(a).decision(tool, v.clone()), w, "{tool} {v} {a:?}");
        }
    }
    // Reading only: a command runs, and cannot write.
    let ro = t(Access::Read);
    let o = ro.run("shell", json!({"command": "touch new"})).await;
    assert!(!o.ok && ro.file("new").is_none(), "{o:?}");
    let mut t = t(Access::Work);
    t.site.policy = Policy::parse(
        "version=1\n[[rule]]\nid='e'\ntool='read'\npaths=['**/*.rs']\ndecision='deny'\n[[rule]]\nid='g'\ntool='shell'\nargv=['git',{rest='any'}]\ndecision='ask'",
        Source::Workspace,
    )
    .unwrap();
    assert_eq!(t.decision("read", json!({"path": "src/b.rs"})), Deny);
    assert_eq!(t.decision("shell", json!({"command": "git status"})), Ask);
    let o = t.run("search", json!({"pattern": "main"})).await;
    assert!(!o.text.contains("fn main") && o.text.contains("not allowed"), "{o:?}");
    let o = t.run("edit", json!({"path": "src/b.rs", "old": "main", "new": "x"})).await;
    assert!(!o.ok && o.text.contains("may not be read"), "{o:?}");
    t.site.caps.tools = Tools::Only(["read".to_string()].into());
    assert_eq!(t.decision("shell", json!({"command": "ls"})), Deny);
    t.site.caps.tools = Tools::All;
    t.site.caps.run = false;
    assert_eq!(t.decision("shell", json!({"command": "ls"})), Deny);
}

#[tokio::test]
async fn real_programs_work_in_the_sandbox() {
    let mut t = t(Access::Work);
    // The tools of whoever runs this.
    let home = Place::new(std::fs::canonicalize(std::env::var("HOME").unwrap()).unwrap()).unwrap();
    (t.site.path, t.site.system) = system(&home);
    t.site.keep = Keep::new(home, vec![Place::new(t.root.join("own")).unwrap()]);
    let mut ran = Vec::new();
    for (p, c) in [
        ("git", "git --version && git log 2>&1 | head -1; git diff --no-index a.txt a.txt"),
        ("python3", "python3 -c 'print(6*7)'"),
        ("node", "node -e 'console.log(6*7)'"),
        ("cargo", "cargo --version"),
        (
            "rustc",
            "rustc --version && printf 'fn main(){println!(\"{}\",6*7)}' > m.rs && rustc m.rs && ./m",
        ),
        ("perl", "perl -e 'print 6*7'"),
        ("make", "printf 'a:\\n\\techo 42\\n' > M && make -f M"),
        (
            "cc",
            "printf '#include <stdio.h>\\nint main(){printf(\"42\");}' > c.c && cc c.c -o c && ./c",
        ),
    ] {
        if std::process::Command::new("sh")
            .args(["-c", &format!("command -v {p}")])
            .output()
            .unwrap()
            .status
            .success()
        {
            let o = t.run("shell", json!({"command": c})).await;
            ran.push((p, o.ok, o.text));
        }
    }
    for (p, ok, text) in &ran {
        assert!(ok, "{p}: {text}");
    }
    assert!(ran.len() > 2, "{ran:?}");
}

#[test]
fn what_is_offered() {
    let s = specs();
    let names: Vec<&str> = s.iter().map(|s| s.name).collect();
    assert_eq!(names, ["read", "write", "edit", "patch", "list", "glob", "search", "shell"]);
    let mut digests: Vec<String> = s.iter().map(schema_digest).collect();
    assert_eq!(digests, specs().iter().map(schema_digest).collect::<Vec<_>>());
    digests.sort();
    digests.dedup();
    assert_eq!(digests.len(), s.len());
    let size: usize = s.iter().map(|s| s.schema.to_string().len() + s.description.len()).sum();
    assert!(size < 4500, "what every request carries: {size} bytes");
}
