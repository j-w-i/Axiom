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
    // On Linux not in /tmp, which is another one inside the sandbox.
    let d = if cfg!(target_os = "linux") {
        tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR"))
    } else {
        tempfile::tempdir()
    }
    .unwrap();
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
            web: false,
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

/// Nobody listens.
fn quiet() -> tokio::sync::mpsc::Sender<String> {
    tokio::sync::mpsc::channel(1).0
}

impl T {
    fn plan(&self, tool: &str, v: Value) -> Plan {
        plan(tool, &v, &self.site, &self.seen)
    }
    /// Planned and done, as if a person said yes to everything.
    async fn run(&self, tool: &str, v: Value) -> Output {
        match self.plan(tool, v) {
            Plan::No(why) => {
                Output { ok: false, text: format!("NO {why}"), rest: None, detail: Value::Null }
            }
            Plan::Go(s, _) | Plan::Ask(_, s, _) => s.run(&self.site, &self.seen, quiet()).await,
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
    assert_eq!(o.detail["summary"], "+1 −0");
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
    let o = step.run(&t.site, &t.seen, quiet()).await;
    assert!(!o.ok && o.text.contains("changed meanwhile"), "{o:?}");
    assert_eq!(t.file("a.txt").as_deref(), Some("one\ntwo\nmore\n"));
    // A new file that someone else made meanwhile.
    let Plan::Ask(_, step, _) = t.plan("write", json!({"path": "n.txt", "content": "mine"})) else {
        panic!()
    };
    std::fs::write(t.root.join("home/w/n.txt"), "theirs").unwrap();
    assert!(!step.run(&t.site, &t.seen, quiet()).await.ok);
    assert_eq!(t.file("n.txt").as_deref(), Some("theirs"));
    // A folder that became a link out.
    let Plan::Ask(_, step, _) = t.plan("write", json!({"path": "src/n/m.txt", "content": "mine"}))
    else {
        panic!()
    };
    std::fs::rename(t.root.join("home/w/src"), t.root.join("home/w/src2")).unwrap();
    std::os::unix::fs::symlink(t.root.join("out"), t.root.join("home/w/src")).unwrap();
    assert!(!step.run(&t.site, &t.seen, quiet()).await.ok);
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
    let o = step.run(&t.site, &t.seen, quiet()).await;
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
    let o = s.run(&wide, &t.seen, quiet()).await;
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
    assert!(o.detail["summary"].as_str().unwrap().starts_with("exit 3, 0."), "{o:?}");
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
    let job = tokio::spawn(async move { step.run(&site, &seen, tx).await });
    let heard = tokio::time::timeout(Duration::from_secs(20), rx.recv()).await.unwrap().unwrap();
    assert_eq!(heard, "up\n");
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

/// What is shown as it comes and what is added at the end are the whole of it, once.
#[tokio::test]
async fn what_is_shown_is_whole() {
    let t = t(Access::Work);
    for (c, whole) in [
        ("printf 'a\\nb'", "a\nb\n"),
        ("seq 1 30000", ""),
        ("echo x; exit 2", "x\n(exit 2)\n"),
        ("true", "(no output)\n"),
    ] {
        let (Plan::Go(step, _) | Plan::Ask(_, step, _)) = t.plan("shell", json!({ "command": c }))
        else {
            panic!("{c}")
        };
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(2);
        let listen = async {
            let mut all = String::new();
            while let Some(s) = rx.recv().await {
                all += &s;
            }
            all
        };
        let (o, mut all) = tokio::join!(step.run(&t.site, &t.seen, tx), listen);
        all += &o.rest.unwrap();
        if whole.is_empty() {
            assert_eq!(
                all.lines().filter(|l| l.parse::<u32>().is_ok()).count(),
                30_000,
                "nothing is left out"
            );
            assert!(o.text.len() < 60_000);
        } else {
            assert_eq!(all, whole, "{c}");
            assert_eq!(o.text, whole, "{c}");
        }
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
        if t.site.path.iter().any(|d| d.as_path().join(p).exists()) {
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
    assert_eq!(
        names,
        ["read", "write", "edit", "patch", "list", "glob", "search", "shell", "job", "fetch"]
    );
    let mut digests: Vec<String> = s.iter().map(schema_digest).collect();
    assert_eq!(digests, specs().iter().map(schema_digest).collect::<Vec<_>>());
    digests.sort();
    digests.dedup();
    assert_eq!(digests.len(), s.len());
    let size: usize = s.iter().map(|s| s.schema.to_string().len() + s.description.len()).sum();
    assert!(size < 5500, "what every request carries: {size} bytes");
}

#[tokio::test]
async fn jobs_go_on_and_are_read() {
    let t = t(Access::Work);
    let m = format!("7{:05}", std::process::id() % 100_000);
    let job = |v: Value| t.run("job", v);
    let o =
        job(json!({"do": "start", "command": format!("echo up && sleep {m} && echo late")})).await;
    assert!(o.ok && o.text.starts_with("job 1 runs"), "{o:?}");
    let o = job(json!({"do": "read", "id": 1, "wait": 20})).await;
    assert_eq!(o.text, "up\n(job 1: it runs)\n");
    let o = job(json!({"do": "read", "id": 1})).await;
    assert_eq!(o.text, "(job 1: it runs)\n", "nothing twice");
    // A person sees what it said last, also what the agent has read.
    let shown = t.seen.1.shown();
    let one = shown.first().unwrap();
    assert_eq!((one.job, one.last.as_str(), one.ended.is_none()), (1, "up\n", true));
    assert!(one.command.starts_with("echo up"));
    // What it runs is known while it runs.
    let mut runs = Vec::new();
    for _ in 0..100 {
        runs = t.seen.1.shown().first().unwrap().processes.clone();
        if runs.len() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(runs.len() >= 2, "the shell and its sleep: {runs:?}");
    assert!(job(json!({"do": "list"})).await.text.starts_with("1: echo up"));
    assert!(job(json!({"do": "stop", "id": 1})).await.ok);
    let mut last = String::new();
    for _ in 0..100 {
        last = job(json!({"do": "read", "id": 1})).await.text;
        if last.contains("ended") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(last.contains("ended"), "{last}");
    let runs = || {
        let o = std::process::Command::new("ps").args(["-axo", "args="]).output().unwrap();
        String::from_utf8_lossy(&o.stdout).lines().any(|l| l.ends_with(&format!("sleep {m}")))
    };
    assert!(!runs());
    // One that ends by itself, and one whose agent goes.
    job(json!({"do": "start", "command": "echo x && false"})).await;
    let mut all = String::new();
    for _ in 0..100 {
        all += &job(json!({"do": "read", "id": 2, "wait": 1})).await.text;
        if all.contains("exit 1") {
            break;
        }
    }
    assert!(all.starts_with("x\n") && all.contains("ended: exit 1"), "{all}");
    job(json!({"do": "start", "command": format!("sleep {m}")})).await;
    let now = Instant::now();
    while !runs() && now.elapsed() < Duration::from_secs(10) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    t.seen.1.end();
    let now = Instant::now();
    while runs() {
        assert!(now.elapsed() < Duration::from_secs(10));
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // Bounded, and what it says too.
    job(json!({"do": "start", "command": "seq 1 100000"})).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let o = job(json!({"do": "read", "id": 4})).await;
    assert!(
        o.text.len() < 80_000 && o.text.contains("not read in time") && o.text.contains("100000"),
        "{}",
        o.text.len()
    );
    let last = t.seen.1.shown().into_iter().find(|j| j.job == 4).unwrap().last;
    assert!(last.len() <= 4096 && last.ends_with("100000\n"), "{}", last.len());
    for _ in 0..8 {
        job(json!({"do": "start", "command": "sleep 30"})).await;
    }
    assert!(
        job(json!({"do": "start", "command": "sleep 30"})).await.text.contains("there are 8 jobs")
    );
    for bad in [
        json!({}),
        json!({"do": "x"}),
        json!({"do": "start"}),
        json!({"do": "read"}),
        json!({"do": "read", "id": 99}),
        json!({"do": "stop", "id": 99}),
    ] {
        assert!(!job(bad.clone()).await.ok, "{bad}");
    }
    // Judged as any command.
    let ask = self::t(Access::Ask);
    assert_eq!(ask.decision("job", json!({"do": "start", "command": "ls"})), Decision::Ask);
    assert_eq!(ask.decision("job", json!({"do": "list"})), Decision::Allow);
    let o = t.run("job", json!({"do": "start", "command": "touch ../out-of-it"})).await;
    assert!(!o.ok || !t.root.join("home/out-of-it").exists());
    t.seen.1.end();
}

#[tokio::test]
async fn what_was_read_in_part_is_not_written_anew() {
    let t = t(Access::Work);
    let text: String = (1..=30).map(|n| format!("line {n}\n")).collect();
    std::fs::write(t.root.join("home/w/long.txt"), &text).unwrap();
    let write = json!({"path": "long.txt", "content": "short\n"});
    let read =
        |from: usize, lines: usize| json!({"path": "long.txt", "from": from, "lines": lines});
    assert!(t.run("write", write.clone()).await.text.contains("read long.txt before"));
    t.run("read", read(1, 10)).await;
    t.run("read", read(21, 10)).await;
    let o = t.run("write", write.clone()).await;
    assert!(o.text.contains("only a part"), "{}", o.text);
    // What it says itself it may replace.
    let o = t.run("edit", json!({"path": "long.txt", "old": "line 5\n", "new": "LINE 5\n"})).await;
    assert!(o.ok, "{}", o.text);
    std::fs::write(t.root.join("home/w/long.txt"), &text).unwrap();
    // In pieces, in any order, is whole as well; after a change nothing of it counts.
    for (from, lines) in [(21, 10), (1, 15), (3, 2), (14, 7)] {
        assert!(!t.run("write", write.clone()).await.ok);
        t.run("read", read(from, lines)).await;
    }
    assert!(t.run("write", write.clone()).await.ok);
    assert_eq!(t.file("long.txt").as_deref(), Some("short\n"));
    // What it wrote it knows.
    assert!(t.run("write", json!({"path": "long.txt", "content": "again\n"})).await.ok);
    std::fs::write(t.root.join("home/w/none.txt"), "").unwrap();
    t.run("read", json!({"path": "none.txt"})).await;
    assert!(t.run("write", json!({"path": "none.txt", "content": "x"})).await.ok);
}

#[tokio::test]
async fn arguments_as_models_send_them() {
    let t = t(Access::Work);
    let o = t.run("read", json!({"path": "a.txt", "from": 2.0, "lines": 1.0})).await;
    assert!(o.ok && o.text.contains("two") && !o.text.contains("one"), "{}", o.text);
    let o = t
        .run("search", json!({"pattern": "two", "path": null, "only": null, "around": null}))
        .await;
    assert!(o.ok, "{}", o.text);
    assert!(t.run("job", json!({"do": "list", "id": null, "command": null})).await.ok);
    for bad in [
        json!({"path": null}),
        json!({"path": "a.txt", "from": 1.5}),
        json!({"path": "a.txt", "from": -1.0}),
    ] {
        assert!(!t.run("read", bad.clone()).await.ok, "{bad}");
    }
}

#[tokio::test]
async fn what_is_found_has_an_end() {
    let t = t(Access::Work);
    let line = "x".repeat(400) + "\n";
    for d in 0..3 {
        std::fs::write(t.root.join(format!("home/w/big{d}.txt")), line.repeat(4000)).unwrap();
    }
    let o = t.run("search", json!({"pattern": "x", "around": 20})).await;
    assert!(o.ok && o.text.len() < 96 * 1024 && o.text.contains("there may be more"));
    let many = t.root.join("home/w/many");
    std::fs::create_dir(&many).unwrap();
    for n in 0..450 {
        std::fs::write(many.join(format!("{n:03}-{}", "y".repeat(230))), "").unwrap();
    }
    for (tool, args) in
        [("list", json!({"path": "many"})), ("glob", json!({"pattern": "*", "path": "many"}))]
    {
        let o = t.run(tool, args).await;
        let shown = o.text.lines().filter(|l| !l.starts_with('(')).count();
        assert!(o.text.len() < 66 * 1024, "{tool}");
        assert!(o.text.contains(&format!("({} more)", 450 - shown)), "{tool}");
    }
}

#[tokio::test]
async fn a_patch_that_fails_leaves_no_folders() {
    let t = t(Access::Work);
    let p = "*** Begin Patch\n*** Add File: n1/n2/new\n+x\n*** Update File: a.txt\n-one\n+1\n*** End Patch";
    let (Plan::Go(step, _) | Plan::Ask(_, step, _)) = t.plan("patch", json!({"patch": p})) else {
        panic!()
    };
    std::fs::write(t.root.join("home/w/a.txt"), "other\n").unwrap();
    let o = step.run(&t.site, &t.seen, quiet()).await;
    assert!(!o.ok, "{}", o.text);
    assert!(!t.root.join("home/w/n1").exists());
    std::fs::write(t.root.join("home/w/a.txt"), "one\n").unwrap();
    assert!(t.run("patch", json!({"patch": p})).await.ok);
    assert_eq!(t.file("n1/n2/new").as_deref(), Some("x\n"));
    let many: String = (0..101).map(|n| format!("*** Add File: f{n}\n+x\n")).collect();
    let o = t.run("patch", json!({"patch": format!("*** Begin Patch\n{many}*** End Patch")})).await;
    assert!(o.text.contains("more than 100 files"), "{}", o.text);
}

/// The strictest of its files decides for a patch, in whatever order they are.
#[tokio::test]
async fn one_file_to_ask_about_is_enough() {
    let mut t = t(Access::Work);
    let rules = "version=1\n[[rule]]\nid='a'\ntool='write'\npaths=['src/**']\ndecision='ask'";
    t.site.policy = Policy::parse(rules, Source::Workspace).unwrap();
    let add = |f: &str| format!("*** Add File: {f}\n+x\n");
    for (files, want) in [
        (vec!["new", "other"], Decision::Allow),
        (vec!["new", "src/new"], Decision::Ask),
        (vec!["src/new", "new"], Decision::Ask),
        (vec!["n1", "src/n2", "n3"], Decision::Ask),
    ] {
        let body: String = files.iter().map(|f| add(f)).collect();
        let p = format!("*** Begin Patch\n{body}*** End Patch");
        assert_eq!(t.decision("patch", json!({"patch": p})), want, "{files:?}");
    }
}

/// A walk does not show what would have to be asked about.
#[tokio::test]
async fn a_walk_shows_only_what_may_be_read() {
    let mut t = t(Access::Work);
    let rules = "version=1\n[[rule]]\nid='a'\ntool='read'\npaths=['**/*.rs']\ndecision='ask'";
    t.site.policy = Policy::parse(rules, Source::Workspace).unwrap();
    for (tool, args) in [
        ("glob", json!({"pattern": "**"})),
        ("search", json!({"pattern": "main|one"})),
        ("list", json!({"path": "src"})),
    ] {
        let o = t.run(tool, args).await;
        assert!(o.ok && !o.text.contains("b.rs") && o.text.contains("not allowed"), "{}", o.text);
    }
}

/// A file is never in neither place.
#[tokio::test]
async fn a_moved_file_is_written_before_the_old_one_goes() {
    let t = t(Access::Work);
    let p = "*** Begin Patch\n*** Update File: a.txt\n*** Move to: src/b.rs.d/c.txt\n-one\n+1\n*** End Patch";
    let (Plan::Go(step, _) | Plan::Ask(_, step, _)) = t.plan("patch", json!({"patch": p})) else {
        panic!()
    };
    // The new place cannot be written any more.
    std::fs::write(t.root.join("home/w/src/b.rs.d"), "in the way").unwrap();
    assert!(!step.run(&t.site, &t.seen, quiet()).await.ok);
    assert_eq!(t.file("a.txt").as_deref(), Some("one\ntwo\nthree\n"));
}

#[tokio::test]
async fn only_the_last_outputs_are_kept() {
    let t = t(Access::Work);
    std::fs::write(t.root.join("tmp/output-mine.txt"), "").unwrap();
    for _ in 0..10 {
        t.run("shell", json!({"command": "seq 1 30000"})).await;
    }
    let names: Vec<String> = std::fs::read_dir(t.root.join("tmp"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.starts_with("output-"))
        .collect();
    assert_eq!(names.len(), 9, "{names:?}");
    assert!(names.contains(&"output-mine.txt".into()));
}

/// Somebody holds the output open who cannot be found any more: that is not waited for.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn who_holds_the_output_open_is_not_waited_for() {
    let t = t(Access::Work);
    let script = "perl -MPOSIX -e 'POSIX::close($_) for 3..255; if (!fork) { setsid(); if (!fork) { sleep 12 } exit } exit' ; echo done";
    let now = Instant::now();
    let o = t.run("shell", json!({ "command": script })).await;
    assert!(o.text.contains("done"), "{}", o.text);
    assert!(now.elapsed() < Duration::from_secs(8), "{:?}", now.elapsed());
}

/// SI-13: nothing is reached that was not judged, and who may not ask does not.
#[tokio::test]
async fn what_may_be_fetched() {
    use Decision::{Allow, Ask, Deny};
    let mut t = t(Access::Work);
    let url = |u: &str| json!({ "url": u });
    assert_eq!(t.decision("fetch", url("https://docs.rs/x")), Deny, "no web at all");
    t.site.caps.web = true;
    assert_eq!(t.decision("fetch", url("https://docs.rs/x")), Ask);
    let Plan::Ask(ask, step, _) = t.plan("fetch", url("https://DOCS.rs/a/../x?q=é#p")) else {
        panic!()
    };
    // What is asked about is what is sent, to the byte.
    assert_eq!(ask.subject, "https://docs.rs/x?q=%C3%A9");
    assert!(ask.scope.contains("docs.rs") && step.changes());
    for bad in ["http://docs.rs/", "https://127.0.0.1/", "https://localhost/", "https://a@b.io/"] {
        assert_eq!(t.decision("fetch", url(bad)), Deny, "{bad}");
    }
    t.site.caps.net = Net::of([axiom_policy::Host::parse("docs.rs").unwrap()]);
    assert_eq!(t.decision("fetch", url("https://docs.rs/x")), Allow);
    assert_eq!(t.decision("fetch", url("https://docs.rs:8443/x")), Ask);
    assert_eq!(t.decision("fetch", url("https://docs.rs.evil.io/x")), Ask);
    t.site.access = Access::Ask;
    assert_eq!(t.decision("fetch", url("https://docs.rs/x")), Ask);
    t.site.access = Access::Work;
    let rules = "version=1\n[[rule]]\nid='n'\ntool='fetch'\nhosts=['*.evil.io']\ndecision='deny'\n[[rule]]\nid='y'\ntool='fetch'\nhosts=['crates.io']\ndecision='allow'";
    t.site.policy = Policy::parse(rules, Source::Person).unwrap();
    assert_eq!(t.decision("fetch", url("https://docs.rs.evil.io/x")), Deny);
    assert_eq!(t.decision("fetch", url("https://crates.io/x")), Allow);
    assert_eq!(t.decision("fetch", url("https://static.crates.io/x")), Ask);
}
