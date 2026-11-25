//! Agents that work apart, through the real socket: their copy of the folder is made, worked in, and
//! brought in as a person says. The model is a server that answers from a script.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use axiom_proto::*;
use serde_json::{Value, json};

mod common;
use common::*;
#[path = "../../engine/tests/common/server.rs"]
mod server;
use server::*;
#[path = "common/apart.rs"]
mod apart;
use apart::*;

#[tokio::test]
async fn an_agent_works_apart_and_a_person_brings_it_in() {
    let mut t = T::new(vec![]).await;
    let w = t.f.dir.path().join("w");
    git(&w, &["init", "-q", "-b", "trunk"]);
    t.write(".gitignore", "/target\n");
    t.write("a.txt", "one\n");
    t.write("b.txt", "two\n");
    t.write("target/tracked", "t");
    git(&w, &["add", "-A"]);
    git(&w, &["add", "-f", "target/tracked"]);
    git(&w, &["commit", "-q", "-m", "first"]);
    t.write("a.txt", "one and not committed\n");
    t.write("target/big", "x");
    let a = t.agent("Builder", Access::Work, true).await;
    // Nothing before it is spoken to.
    assert_eq!(t.changes(&a).await, CopyChanges::default());
    assert!(!t.copy_of(&a).exists());
    let (o, e) = t
        .does(
            &a,
            "pwd > where; echo agent > a.txt; mkdir -p new && echo n > new/n.txt; rm b.txt; \
             git log --oneline > log; ls target > seen",
        )
        .await;
    assert_eq!((o, e), (Outcome::Completed, None));
    let copy = std::fs::canonicalize(t.copy_of(&a)).unwrap();
    assert_eq!(t.copied(&a, "where").unwrap().trim(), copy.to_str().unwrap());
    assert!(t.copied(&a, "log").unwrap().contains("first"), "git says what there was");
    // What git ignores is not there, but for what it tracks all the same.
    assert_eq!(t.copied(&a, "seen").as_deref(), Some("tracked\n"));
    assert!(t.server.requests()[0].to_string().contains("a copy of the project's folder"));
    // None of it is in the folder.
    assert_eq!(t.file("a.txt").as_deref(), Some("one and not committed\n"));
    assert!(t.file("b.txt").is_some() && t.file("where").is_none());
    let c = t.changes(&a).await;
    assert!(c.made && c.whole && c.back.is_none());
    assert_eq!(
        paths(&c),
        [
            ("a.txt", "changed"),
            ("b.txt", "removed"),
            ("log", "new"),
            ("new/n.txt", "new"),
            ("seen", "new"),
            ("where", "new"),
        ]
    );
    let diff = file(&c, "a.txt").diff.clone().unwrap();
    assert!(diff.contains("-one and not committed\n+agent\n"), "{diff}");
    assert!(c.files.iter().all(|f| !f.clash && f.never.is_none() && f.others.is_empty()));
    // It cannot reach the folder by itself.
    let there = t.path("evil");
    t.does(
        &a,
        &format!("echo x > {0}; cat {1} > stolen", there.display(), t.path("a.txt").display()),
    )
    .await;
    assert!(!there.exists());
    assert_eq!(t.copied(&a, "stolen").unwrap_or_default(), "");

    let r = t.bring(&a, &["a.txt", "b.txt", "new/n.txt"]).await;
    assert_eq!(r, json!({"brought": ["a.txt", "b.txt", "new/n.txt"], "left": []}));
    assert_eq!(t.file("a.txt").as_deref(), Some("agent\n"));
    assert_eq!(t.file("new/n.txt").as_deref(), Some("n\n"));
    assert_eq!((t.file("b.txt"), t.file("log")), (None, None));
    let c = t.changes(&a).await;
    let back = c.back.clone().unwrap();
    assert_eq!(back.files, 3);
    assert!(c.files.iter().all(|f| f.path != "a.txt" && f.path != "b.txt"));
    let record = t.record().await;
    assert!(record[0].starts_with("brought in: a.txt (") && record[0].contains("b.txt (removed)"));

    let (session, id) = (t.s.clone(), back.id);
    // Put back, but not over what has been changed since.
    t.write("a.txt", "mine\n");
    let back = |id: &str| Call::CopyBack {
        session: SessionId(session.0.clone()),
        agent: a.clone(),
        back: id.into(),
    };
    assert!(t.c.call(back("q_x")).await.is_err(), "not another than was shown");
    let call = back(&id);
    let r = t.c.call(call.clone()).await.unwrap();
    assert_eq!(r["back"], json!(["b.txt", "new/n.txt"]));
    assert_eq!(r["left"], json!([{"path": "a.txt", "why": "it has changed since"}]));
    assert_eq!((t.file("b.txt").as_deref(), t.file("new/n.txt")), (Some("two\n"), None));
    t.write("a.txt", "agent\n");
    assert_eq!(t.c.call(call).await.unwrap()["back"], json!(["a.txt"]));
    assert_eq!(t.file("a.txt").as_deref(), Some("one and not committed\n"));
    assert!(paths(&t.changes(&a).await).contains(&("a.txt", "changed")));

    // It is there after the host has been away.
    t.restart().await;
    assert!(paths(&t.changes(&a).await).contains(&("a.txt", "changed")));
    t.does(&a, "cat a.txt >> again").await;
    assert_eq!(t.copied(&a, "again").as_deref(), Some("agent\n"));

    // Thrown away only with a word to it, and then there is another.
    let e = t.remove(&a, false).await.unwrap_err();
    assert_eq!(e.code, ErrorCode::Busy);
    assert!(e.message.contains("are in it only"), "{}", e.message);
    assert!(t.copy_of(&a).exists());
    t.remove(&a, true).await.unwrap();
    assert!(!t.copy_of(&a).exists());
    assert!(!t.changes(&a).await.made);
    // It can be set up as another, as any agent.
    let (session, access) = (t.s.clone(), Some(Access::Ask));
    let set = Call::AgentSet { session, agent: a.clone(), execution: None, access, plan: None };
    t.c.call(set).await.unwrap();
    t.does(&a, "true").await;
    assert_eq!(t.copied(&a, "a.txt").as_deref(), Some("one and not committed\n"));
    assert_eq!(t.changes(&a).await.files, []);
    t.remove(&a, false).await.unwrap();
    assert!(t.record().await.len() >= 5);
}

#[tokio::test]
async fn what_is_shown_is_what_is_brought_in() {
    let mut t = T::new(vec![]).await;
    t.write("a.txt", "one\n");
    t.write("b.txt", "two\n");
    let a = t.agent("Builder", Access::Work, true).await;
    t.does(&a, "echo agent > a.txt; echo agent > b.txt; echo x > .envrc").await;
    let own = t.s.clone();
    let bring =
        |files: Vec<CopySeen>| Call::CopyBring { session: own.clone(), agent: a.clone(), files };
    let seen = |path: &str, digest: Option<&str>| CopySeen {
        path: path.into(),
        digest: digest.map(Into::into),
        run: false,
        there: None,
    };
    let c: CopyChanges = {
        let call = Call::CopyChanges { session: t.s.clone(), agent: a.clone() };
        serde_json::from_value(t.c.call(call).await.unwrap()).unwrap()
    };
    let digest = c.files.iter().find(|f| f.path == "a.txt").unwrap().digest.clone().unwrap();
    // Changed after it was shown; another digest; none; a name that is not below the folder.
    let call = bring(vec![
        seen("a.txt", Some("other")),
        seen("b.txt", None),
        seen("../w/a.txt", None),
        seen("../evil", Some(&digest)),
        seen("/etc/x", Some(&digest)),
    ]);
    let r = t.c.call(call).await.unwrap();
    assert_eq!(r["brought"].as_array().map(Vec::len), Some(0), "{r}");
    assert_eq!(r["left"].as_array().map(Vec::len), Some(5), "{r}");
    // The folder's own has changed, whether that was shown or not.
    let there = file(&c, "a.txt").there.clone();
    std::fs::write(t.path("a.txt"), "mine\n").unwrap();
    let mut mine = t.changes(&a).await;
    let now = mine.files.remove(mine.files.iter().position(|f| f.path == "a.txt").unwrap()).there;
    for there in [there, now, None] {
        let shown = CopySeen { there, ..seen("a.txt", Some(&digest)) };
        let call = bring(vec![shown, seen(".envrc", Some(&digest))]);
        let r = t.c.call(call).await.unwrap();
        assert_eq!(r["brought"], json!([]), "{r}");
    }
    assert_eq!(t.file("a.txt").as_deref(), Some("mine\n"));
    assert_eq!(t.file("b.txt").as_deref(), Some("two\n"));
    assert!(t.c.call(bring(vec![])).await.is_err());
    let c = t.changes(&a).await;
    assert!(c.files.iter().find(|f| f.path == "a.txt").unwrap().clash);
    // Nothing was brought in, so nothing is put back.
    assert_eq!(c.back, None);
}

#[tokio::test]
async fn two_who_change_the_same_are_told_of() {
    let mut t = T::new(vec![]).await;
    t.write("same.txt", "one\n");
    t.write("x.txt", "x\n");
    let a = t.agent("Ada", Access::Work, true).await;
    let b = t.agent("Ben", Access::Work, true).await;
    t.does(&a, "echo ada > same.txt; echo ada > x.txt").await;
    t.does(&b, "cat same.txt > was; echo ben > same.txt").await;
    assert_eq!(t.copied(&b, "was").as_deref(), Some("one\n"), "they do not see each other's");
    let c = t.changes(&a).await;
    let others =
        |c: &CopyChanges, p: &str| c.files.iter().find(|f| f.path == p).unwrap().others.clone();
    assert_eq!((others(&c, "same.txt"), others(&c, "x.txt")), (vec!["Ben".to_owned()], vec![]));
    t.bring(&a, &["same.txt"]).await;
    // Now it is a clash for the other, who is brought up to date and settles it.
    let c = t.changes(&b).await;
    assert!(c.files.iter().find(|f| f.path == "same.txt").unwrap().clash);
    let call = Call::CopyRenew { session: t.s.clone(), agent: b.clone() };
    // Told of until it is settled.
    t.c.call(call.clone()).await.unwrap();
    let c = t.changes(&b).await;
    assert!(file(&c, "same.txt").unsettled && !file(&c, "same.txt").clash);
    let r = t.c.call(call).await.unwrap();
    assert_eq!((&r["clashes"], &r["taken"]), (&json!(["same.txt"]), &json!(0)));
    let say = r["say"].as_str().unwrap();
    assert!(say.contains("same.txt"), "{say}");
    let place = say.split("What the project has now is in ").nth(1).unwrap();
    let place = place.split(", under").next().unwrap().to_owned();
    t.does(&b, &format!("cat {place}/same.txt same.txt > both && mv both same.txt")).await;
    assert_eq!(t.copied(&b, "same.txt").as_deref(), Some("ada\nben\n"));
    let c = t.changes(&b).await;
    assert!(!c.files.iter().find(|f| f.path == "same.txt").unwrap().clash);
    let f = file(&c, "same.txt");
    assert!(!f.unsettled && f.diff.as_ref().unwrap().contains(" ada\n+ben\n"), "{f:?}");
    let call = Call::CopyRenew { session: t.s.clone(), agent: b.clone() };
    assert_eq!(t.c.call(call).await.unwrap()["clashes"], json!([]), "it is settled");
    t.bring(&b, &["same.txt"]).await;
    assert_eq!(t.file("same.txt").as_deref(), Some("ada\nben\n"));
    // What the folder has and only it changed is taken over.
    let r = t.c.call(Call::CopyRenew { session: t.s.clone(), agent: a.clone() }).await.unwrap();
    assert_eq!((&r["taken"], &r["say"]), (&json!(1), &Value::Null));
    assert_eq!(t.copied(&a, "same.txt").as_deref(), Some("ada\nben\n"));
}

#[tokio::test]
async fn the_folders_rules_hold_in_a_copy() {
    let mut t = T::new(vec![]).await;
    t.write("secret.txt", "SECRET");
    t.write("a.txt", "one");
    t.write(
        ".axiom/policy.toml",
        "version = 1\n[[rule]]\nid = 's'\ntool = 'read'\npaths = ['secret.txt']\ndecision = 'deny'\n\
         [[rule]]\nid = 'w'\ntool = 'write'\npaths = ['a.txt']\ndecision = 'deny'\n",
    );
    t.write(".gitignore", ".axiom\n");
    let a = t.agent("Builder", Access::Work, true).await;
    t.write("b.txt", "two");
    let write = |p: &str| json!({"path": p, "content": "agent"}).to_string();
    t.server.add(calls(&[
        ("c1", "read", r#"{"path":"a.txt"}"#),
        ("c2", "read", r#"{"path":"b.txt"}"#),
    ]));
    t.server.add(calls(&[
        ("c3", "write", &write("a.txt")),
        ("c4", "write", &write("b.txt")),
        ("c5", "shell", r#"{"command":"cat secret.txt > got; true"}"#),
    ]));
    t.server.add(text(&["done"]));
    t.tell(&a).await.unwrap();
    assert_eq!(t.ended().await.0, Outcome::Completed);
    // Ignored, so it is not in the copy; it is the folder's that counts.
    assert_eq!(t.copied(&a, ".axiom/policy.toml"), None);
    assert_eq!(t.copied(&a, "secret.txt"), None, "what may not be read is not copied");
    assert_eq!(t.copied(&a, "got").as_deref(), Some(""));
    assert_eq!(t.copied(&a, "a.txt").as_deref(), Some("one"));
    assert_eq!(t.copied(&a, "b.txt").as_deref(), Some("agent"));
}

#[tokio::test]
async fn what_stands_in_the_way_of_a_copy() {
    let mut t = T::new(vec![]).await;
    t.write("a.txt", "one");
    let a = t.agent("Builder", Access::Work, true).await;
    // One that was begun and not ended.
    let half = t.copy_of(&a);
    std::fs::create_dir_all(&half).unwrap();
    std::fs::write(half.join("half"), "x").unwrap();
    t.does(&a, "true").await;
    assert!(t.copied(&a, "half").is_none() && t.copied(&a, "a.txt").is_some());
    // Its place is a link to elsewhere.
    let b = t.agent("Other", Access::Work, true).await;
    let elsewhere = t.f.dir.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    std::fs::write(elsewhere.join("precious"), "x").unwrap();
    std::os::unix::fs::symlink(&elsewhere, t.copy_of(&b)).unwrap();
    let e = refused(t.tell(&b).await);
    assert!(!e.message.is_empty());
    assert!(elsewhere.join("precious").exists() && !elsewhere.join("a.txt").exists());
    std::fs::remove_file(t.copy_of(&b)).unwrap();
    t.does(&b, "true").await;
    assert!(t.copied(&b, "a.txt").is_some());
    // It became a link after it was made.
    std::fs::rename(t.copy_of(&b), t.f.dir.path().join("moved")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, t.copy_of(&b)).unwrap();
    let call = Call::CopyChanges { session: t.s.clone(), agent: b.clone() };
    assert!(refused(t.c.call(call).await).message.contains("leads elsewhere"));
    assert!(t.tell(&b).await.is_err());
    assert!(t.remove(&b, true).await.is_err());
    assert!(elsewhere.join("precious").exists());
    // One that works in the folder has none.
    let c = t.agent("In it", Access::Work, false).await;
    let call = Call::CopyChanges { session: t.s.clone(), agent: c.clone() };
    assert!(refused(t.c.call(call).await).message.contains("in the folder itself"));
    // Thrown away by hand: the next time there is another.
    std::fs::remove_dir_all(t.copy_of(&a)).unwrap();
    assert!(!t.changes(&a).await.made);
    t.does(&a, "true").await;
    assert!(t.copied(&a, "a.txt").is_some());
}

/// While it works nothing is taken from under it.
#[tokio::test]
async fn a_copy_that_is_worked_in_stays() {
    let mut t = T::new(vec![]).await;
    t.write("a.txt", "one");
    let a = t.agent("Builder", Access::Work, true).await;
    t.server.add(text(&["thinking"]).hangs());
    let run = RunId(t.tell(&a).await.unwrap()["run"].as_str().unwrap().into());
    assert_eq!(t.remove(&a, true).await.unwrap_err().code, ErrorCode::Busy);
    let call = Call::CopyRenew { session: t.s.clone(), agent: a.clone() };
    assert_eq!(refused(t.c.call(call).await).code, ErrorCode::Busy);
    assert!(t.copy_of(&a).exists());
    t.c.call(Call::Cancel { session: t.s.clone(), run }).await.unwrap();
    t.ended().await;
}

/// Who works in a copy does not get the folder changed by telling one who does not.
#[tokio::test]
async fn telling_one_that_works_in_the_folder_is_asked_about() {
    let mut t = T::new(vec![]).await;
    t.write("a.txt", "one");
    let a = t.agent("Apart", Access::Work, true).await;
    t.agent("Inside", Access::Work, false).await;
    t.agent("Looks", Access::Read, false).await;
    t.agent("Also", Access::Work, true).await;
    for (name, asks) in [("Inside", true), ("Looks", false), ("Also", false)] {
        let args = json!({"do": "tell", "name": name, "text": "do it"}).to_string();
        t.server.add(calls(&[("c1", "agent", &args)]));
        for _ in 0..4 {
            t.server.add(text(&["done"]));
        }
        t.tell(&a).await.unwrap();
        let mut asked = false;
        let (mut runs, mut begun) = (0, false);
        loop {
            let m = t.c.next_within(LONG).await.unwrap().expect("more");
            let Message::Events { events, .. } = m else { continue };
            for e in events {
                match e.body {
                    Body::RequestOpened { request, digest, scope, .. } => {
                        asked = true;
                        assert!(scope.contains(name), "{scope}");
                        let session = t.s.clone();
                        let answer = Answer::Deny;
                        t.c.call(Call::Answer { session, request, answer, digest, words: None })
                            .await
                            .unwrap();
                    }
                    Body::RunStarted { .. } => (runs, begun) = (runs + 1, true),
                    Body::RunEnded { .. } => runs -= 1,
                    _ => {}
                }
            }
            if begun && runs == 0 {
                break;
            }
        }
        assert_eq!(asked, asks, "{name}");
        quiet(&mut t.c).await;
        t.server.clear();
    }
}

/// Axiom's own folder of what is known of copies, to be set back to what it was.
fn records(t: &T) -> PathBuf {
    t.f.paths.data.join("copies")
}
fn saved(d: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
        match e.file_type().unwrap().is_dir() {
            true => out.extend(saved(&e.path())),
            false => out.push((e.path(), std::fs::read(e.path()).unwrap())),
        }
    }
    out
}
fn restore(t: &T, all: &[(PathBuf, Vec<u8>)]) {
    std::fs::remove_dir_all(records(t)).unwrap();
    for (p, b) in all {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b).unwrap();
    }
}
fn record_of(all: &[(PathBuf, Vec<u8>)]) -> (usize, Value) {
    let n = all.iter().position(|f| f.0.extension().is_some_and(|e| e == "json")).unwrap();
    (n, serde_json::from_slice(&all[n].1).unwrap())
}

/// What ends half way is made good by what is known of it: nobody's work is taken for the folder's.
#[tokio::test]
async fn what_ended_half_way_is_recognised() {
    let mut t = T::new(vec![]).await;
    for n in ["a.txt", "b.txt"] {
        t.write(n, "old\n");
    }
    let a = t.agent("Builder", Access::Work, true).await;
    t.does(&a, "echo agent > a.txt; echo agent > b.txt; echo n > n.txt").await;
    let first = saved(&records(&t));
    t.bring(&a, &["a.txt", "n.txt"]).await;
    let brought = saved(&records(&t));
    let back =
        |t: &T, id: String| Call::CopyBack { session: t.s.clone(), agent: a.clone(), back: id };

    // Put back, and the host is gone before it has said so.
    let id = t.changes(&a).await.back.unwrap().id;
    t.c.call(back(&t, id.clone())).await.unwrap();
    assert_eq!(t.file("a.txt").as_deref(), Some("old\n"));
    restore(&t, &brought);
    let c = t.changes(&a).await;
    assert_eq!(paths(&c), [("a.txt", "changed"), ("b.txt", "changed"), ("n.txt", "new")]);
    assert!(c.files.iter().all(|f| !f.clash) && c.back.is_none());
    assert_eq!(t.remove(&a, false).await.unwrap_err().code, ErrorCode::Busy);
    let call = Call::CopyRenew { session: t.s.clone(), agent: a.clone() };
    let r = t.c.call(call.clone()).await.unwrap();
    assert_eq!((&r["taken"], &r["clashes"]), (&json!(0), &json!([])), "{r}");
    assert_eq!(t.copied(&a, "a.txt").as_deref(), Some("agent\n"));
    assert_eq!(t.copied(&a, "n.txt").as_deref(), Some("n\n"));
    // Half of it, and then again.
    restore(&t, &brought);
    t.write("n.txt", "n\n");
    let r = t.c.call(back(&t, id)).await.unwrap();
    assert_eq!((&r["back"], &r["left"]), (&json!(["a.txt", "n.txt"]), &json!([])), "{r}");
    assert_eq!(t.file("n.txt"), None);

    // Brought in, and the host is gone before it has said so: as if it had, and it can be put back.
    t.bring(&a, &["a.txt", "n.txt"]).await;
    let mut half = saved(&records(&t));
    let (n, mut v) = record_of(&half);
    v["files"] = record_of(&first).1["files"].clone();
    half[n].1 = v.to_string().into_bytes();
    restore(&t, &half);
    let c = t.changes(&a).await;
    assert_eq!(paths(&c), [("b.txt", "changed")]);
    assert_eq!(c.back.as_ref().unwrap().files, 2);
    t.does(&a, "echo more >> a.txt").await;
    assert!(!file(&t.changes(&a).await, "a.txt").clash, "what was brought in is no clash");
    let id = c.back.unwrap().id;
    assert_eq!(t.c.call(back(&t, id)).await.unwrap()["left"], json!([]));
    assert_eq!((t.file("a.txt").as_deref(), t.file("n.txt")), (Some("old\n"), None));

    // On record, and nothing done yet.
    t.bring(&a, &["b.txt"]).await;
    let mut none = saved(&records(&t));
    let (n, mut v) = record_of(&none);
    v["files"] = record_of(&first).1["files"].clone();
    none[n].1 = v.to_string().into_bytes();
    t.write("b.txt", "old\n");
    restore(&t, &none);
    let c = t.changes(&a).await;
    assert!(!file(&c, "b.txt").clash && c.back.is_none(), "{c:?}");
    assert_eq!(t.bring(&a, &["b.txt"]).await["brought"], json!(["b.txt"]));
    // What is kept is of the last only.
    let dirs =
        std::fs::read_dir(records(&t)).unwrap().flatten().find(|e| e.path().is_dir()).unwrap();
    assert_eq!(std::fs::read_dir(dirs.path()).unwrap().count(), 1);
}

/// What was brought in is known of only as long as nothing else has happened to it.
#[tokio::test]
async fn what_changed_after_it_was_brought_in_is_a_persons() {
    let mut t = T::new(vec![]).await;
    t.write("a.txt", "old\n");
    let a = t.agent("Builder", Access::Work, true).await;
    t.does(&a, "echo agent > a.txt").await;
    t.bring(&a, &["a.txt"]).await;
    // Neither what it was nor what it became: the person's.
    t.write("a.txt", "mine\n");
    t.does(&a, "echo more >> a.txt").await;
    let c = t.changes(&a).await;
    assert!(file(&c, "a.txt").clash);
    assert_eq!(t.bring(&a, &["a.txt"]).await["brought"], json!([]));
    assert_eq!(t.file("a.txt").as_deref(), Some("mine\n"));
    // Taken over since, and then as it was at first: still the person's, not the agent's.
    t.does(&a, "echo agent > a.txt").await;
    let renew = Call::CopyRenew { session: t.s.clone(), agent: a.clone() };
    t.c.call(renew.clone()).await.unwrap();
    assert_eq!(t.copied(&a, "a.txt").as_deref(), Some("mine\n"));
    t.write("a.txt", "old\n");
    assert_eq!(t.changes(&a).await.files, []);
    t.c.call(renew).await.unwrap();
    assert_eq!(t.copied(&a, "a.txt").as_deref(), Some("old\n"));
}

/// The same for bringing a copy up to date: what was taken over is not taken for the agent's.
#[tokio::test]
async fn what_was_taken_over_is_the_folders() {
    let mut t = T::new(vec![]).await;
    for n in ["a.txt", "b.txt", "gone.txt"] {
        t.write(n, "one\n");
    }
    let a = t.agent("Builder", Access::Work, true).await;
    t.does(&a, "true").await;
    let first = saved(&records(&t));
    t.write("a.txt", "two\n");
    t.write("b.txt", "two\n");
    std::fs::remove_file(t.path("gone.txt")).unwrap();
    let call = Call::CopyRenew { session: t.s.clone(), agent: a.clone() };
    t.c.call(call.clone()).await.unwrap();
    // As it is while that goes on: what is to be taken over is on record, and one was not yet.
    let (_, done) = record_of(&saved(&records(&t)));
    let mut half = first.clone();
    let (n, mut v) = record_of(&half);
    v["taking"] =
        json!({"a.txt": done["files"]["a.txt"], "b.txt": done["files"]["b.txt"], "gone.txt": null});
    half[n].1 = v.to_string().into_bytes();
    std::fs::write(t.copy_of(&a).join("b.txt"), "one\n").unwrap();
    restore(&t, &half);
    // The person goes on meanwhile.
    t.write("a.txt", "three\n");
    t.write("gone.txt", "again\n");
    let c = t.changes(&a).await;
    assert_eq!(c.files, [], "nothing of it is the agent's");
    let r = t.c.call(call).await.unwrap();
    assert_eq!((&r["taken"], &r["clashes"], &r["left"]), (&json!(3), &json!([]), &json!([])));
    for (f, text) in [("a.txt", "three\n"), ("b.txt", "two\n"), ("gone.txt", "again\n")] {
        assert_eq!(t.copied(&a, f).as_deref(), Some(text), "{f}");
    }
}

/// What only it has is said, also where git does not look.
#[tokio::test]
async fn nothing_is_thrown_away_unsaid() {
    let mut t = T::new(vec![]).await;
    t.write(".gitignore", "out/\n");
    t.write("a.txt", "one\n");
    let a = t.agent("Builder", Access::Work, true).await;
    t.does(&a, "mkdir out && echo work > out/notes").await;
    let c = t.changes(&a).await;
    assert_eq!((c.files.len(), c.ignored, c.whole), (0, 1, true));
    let e = t.remove(&a, false).await.unwrap_err();
    assert!(e.message.contains("are in it only"), "{}", e.message);
    assert!(t.copied(&a, "out/notes").is_some());
    t.remove(&a, true).await.unwrap();
    // Nor is the home folder copied into itself.
    let home = t.f.dir.path().join("home").to_str().unwrap().to_owned();
    let call = Call::SessionCreate { title: "H".into(), workspace: Some(home) };
    let h = SessionId(t.c.call(call).await.unwrap()["session"].as_str().unwrap().into());
    let was = std::mem::replace(&mut t.s, h);
    let execution = Execution {
        harness: "axiom".into(),
        provider: None,
        model: Some("m".into()),
        connection: Some(t.connection.clone()),
        effort: None,
        environment: None,
    };
    let call = Call::AgentAdd {
        session: t.s.clone(),
        name: "Home".into(),
        execution,
        access: Access::Work,
        trust: false,
        instructions: vec![],
        apart: true,
        plan: false,
    };
    assert!(t.c.call(call).await.is_err());
    t.s = was;
}

/// Nothing of the agent writes any more when its copy is brought up to date or thrown away.
#[tokio::test]
async fn what_it_began_has_ended_first() {
    let mut t = T::new(vec![]).await;
    t.write("a.txt", "one\n");
    let a = t.agent("Builder", Access::Work, true).await;
    let command = "trap '' TERM; while true; do date +%s%N > tick; sleep 0.05; done";
    let args = json!({"do": "start", "command": command}).to_string();
    t.server.add(calls(&[("c1", "job", &args)]));
    t.server.add(text(&["done"]));
    t.tell(&a).await.unwrap();
    t.ended().await;
    for _ in 0..200 {
        if t.copied(&a, "tick").is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(t.copied(&a, "tick").is_some());
    let call = Call::CopyRenew { session: t.s.clone(), agent: a.clone() };
    t.c.call(call).await.unwrap();
    let was = t.copied(&a, "tick");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(t.copied(&a, "tick"), was, "it writes no more");
}

/// A host that is asked to stop does not do so in the middle of it.
#[tokio::test]
async fn it_is_not_stopped_in_the_middle() {
    let mut t = T::new(vec![]).await;
    t.write("a.txt", "old\n");
    let a = t.agent("Builder", Access::Work, true).await;
    t.does(&a, "echo agent > a.txt").await;
    let files: Vec<CopySeen> = t.changes(&a).await.files.into_iter().map(seen).collect();
    // What it puts on record first has to wait.
    let db = rusqlite::Connection::open(t.f.paths.journal()).unwrap();
    db.execute_batch("begin immediate").unwrap();
    let mut other = t.f.client().await;
    let (session, agent) = (t.s.clone(), a.clone());
    let bring = tokio::spawn(async move {
        let call = Call::CopyBring { session, agent, files };
        other.call(call).await
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!bring.is_finished());
    let e = refused(t.c.call(Call::Shutdown { force: false }).await);
    assert_eq!(e.code, ErrorCode::Busy);
    // Told to stop all the same, it ends this first.
    t.c.call(Call::Shutdown { force: true }).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(t.f.paths.token().exists(), "it is there still");
    db.execute_batch("rollback").unwrap();
    for _ in 0..500 {
        if !t.f.paths.token().exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(!t.f.paths.token().exists());
    assert_eq!(t.file("a.txt").as_deref(), Some("agent\n"));
    let _ = bring.await;
    let (_, v) = record_of(&saved(&records(&t)));
    assert_eq!(v["last"]["items"][0]["path"], "a.txt");
}
