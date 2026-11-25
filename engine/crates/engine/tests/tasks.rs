//! Five small tasks in a repository, the same for a model of this machine (by hand) and for one that
//! answers from a script in Anthropic's form. Each is judged by what is there afterwards.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use common::*;

use axiom_engine::Context;
use axiom_policy::Access;
use axiom_proto::{ItemKind, Outcome};

struct Task {
    name: &'static str,
    ask: &'static str,
    done: fn(&T) -> bool,
    script: fn() -> Vec<Reply>,
}

fn prepare(t: &T) {
    let w = t.root.join("w");
    std::fs::create_dir(w.join("src")).unwrap();
    for (f, c) in [
        ("src/math.py", "def add(a, b):\n    return a - b\n"),
        ("src/use.py", "from math_helpers import fetch_total\n\nprint(fetch_total([1, 2]))\n"),
        ("src/math_helpers.py", "def fetch_total(xs):\n    return sum(xs)\n"),
        ("notes.txt", NOTES),
    ] {
        std::fs::write(w.join(f), c).unwrap();
    }
}
const NOTES: &str = "the answer is kept in src\nMAGIC = 7319\n";

fn said(t: &T, what: &str) -> bool {
    t.stage.texts(ItemKind::Prose).iter().any(|s| s.contains(what))
}

const TASKS: &[Task] = &[
    Task {
        name: "make a file",
        ask: "Create a file named greeting.txt that contains exactly the line: hello world",
        done: |t| t.file("greeting.txt").is_some_and(|s| s.trim() == "hello world"),
        script: || {
            vec![
                anthropic(
                    None,
                    "",
                    &[("t1", "write", r#"{"path":"greeting.txt","content":"hello world\n"}"#)],
                ),
                anthropic(None, "Done.", &[]),
            ]
        },
    },
    Task {
        name: "fix a bug",
        ask: "The function add in src/math.py subtracts instead of adding. Fix it.",
        done: |t| {
            t.file("src/math.py")
                .is_some_and(|s| s.contains("return a + b") && s.contains("def add(a, b):"))
        },
        script: || {
            vec![
                anthropic(None, "", &[("t1", "read", r#"{"path":"src/math.py"}"#)]),
                anthropic(
                    None,
                    "",
                    &[("t2", "edit", r#"{"path":"src/math.py","old":"a - b","new":"a + b"}"#)],
                ),
                anthropic(None, "Fixed.", &[]),
            ]
        },
    },
    Task {
        name: "find something",
        ask: "Which file contains the word MAGIC, and what number does it have there? Answer with the file's name and the number.",
        done: |t| said(t, "notes.txt") && said(t, "7319"),
        script: || {
            vec![
                anthropic(None, "", &[("t1", "search", r#"{"pattern":"MAGIC"}"#)]),
                anthropic(None, "It is in notes.txt, with 7319.", &[]),
            ]
        },
    },
    Task {
        name: "rename across files",
        ask: "Rename the function fetch_total to total everywhere in src.",
        done: |t| {
            let (a, b) = (
                t.file("src/use.py").unwrap_or_default(),
                t.file("src/math_helpers.py").unwrap_or_default(),
            );
            !a.contains("fetch_total")
                && !b.contains("fetch_total")
                && b.contains("def total(")
                && a.contains("import total")
        },
        script: || {
            let patch = "*** Begin Patch\n*** Update File: src/math_helpers.py\n-def fetch_total(xs):\n+def total(xs):\n*** Update File: src/use.py\n-from math_helpers import fetch_total\n+from math_helpers import total\n@@\n-print(fetch_total([1, 2]))\n+print(total([1, 2]))\n*** End Patch";
            let args = serde_json::json!({ "patch": patch }).to_string();
            vec![
                anthropic(None, "", &[("t1", "search", r#"{"pattern":"fetch_total"}"#)]),
                anthropic(None, "", &[("t2", "patch", &args)]),
                anthropic(None, "Renamed.", &[]),
            ]
        },
    },
    Task {
        name: "run and tell",
        ask: "Run the command `python3 -c \"print(sum(open('notes.txt','rb').read()))\"` and tell me what it prints.",
        // Not to be known without running it.
        done: |t| {
            let sum: u32 = NOTES.bytes().map(u32::from).sum();
            t.stage.tools().iter().any(|(n, o)| n == "shell" && *o == Outcome::Completed)
                && said(t, &sum.to_string())
        },
        script: || {
            vec![
                anthropic(
                    None,
                    "",
                    &[(
                        "t1",
                        "shell",
                        r#"{"command":"python3 -c \"print(sum(open('notes.txt','rb').read()))\""}"#,
                    )],
                ),
                anthropic(
                    None,
                    &format!("It prints {}.", NOTES.bytes().map(u32::from).sum::<u32>()),
                    &[],
                ),
            ]
        },
    },
];

#[tokio::test]
async fn five_tasks_with_a_scripted_model() {
    for task in TASKS {
        let t = T::anthropic(Access::Work, (task.script)()).await;
        prepare(&t);
        let mut a = t.agent();
        a.model = "claude-test".into();
        let e = t.run(&mut a, task.ask).await;
        assert_eq!((e.outcome, e.error), (Outcome::Completed, None), "{}", task.name);
        assert!((task.done)(&t), "{}", task.name);
        // What was kept reads back to the byte.
        let kept = t.stage.turns();
        let c = Context::load(&kept).unwrap();
        let again: Vec<String> = kept
            .iter()
            .map(|k| {
                serde_json::to_string(&serde_json::from_str::<axiom_engine::Turn>(k).unwrap())
                    .unwrap()
            })
            .collect();
        assert_eq!(again, kept, "{}", task.name);
        assert_eq!(c.messages(), a.context.messages());
    }
}

/// By hand. A person says yes to whatever is asked.
#[tokio::test]
#[ignore = "needs LM Studio on this machine"]
async fn five_tasks_on_lm_studio() {
    let model = std::env::var("AXIOM_LIVE_MODEL").unwrap_or("qwen/qwen3-4b-2507".into());
    let mut good = 0;
    for task in TASKS {
        let t = T::live().await;
        prepare(&t);
        let mut a = t.agent();
        a.model.clone_from(&model);
        a.settings.max_output = Some(1200);
        a.limits.steps = 15;
        a.window = None;
        let now = std::time::Instant::now();
        let person = async {
            let mut answered = 0;
            loop {
                if t.stage.requests() > answered {
                    let (id, digest) = t.stage.request_number(answered);
                    answered += 1;
                    let _ = t.desk.answer(&id, axiom_policy::Answer::Once, &digest);
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        };
        let e = tokio::select! {
            e = t.run(&mut a, task.ask) => e,
            () = person => unreachable!(),
        };
        let ok = (task.done)(&t);
        good += usize::from(ok);
        println!(
            "{}: {} ({:?}, {:.1} s, {} calls) {:?} {:?}",
            task.name,
            if ok { "done" } else { "NOT DONE" },
            e.outcome,
            now.elapsed().as_secs_f64(),
            t.stage.tools().len(),
            t.stage.tools(),
            e.error
        );
        for s in t.stage.said_by_tools().iter().take(12) {
            println!("    tool: {}", s.chars().take(160).collect::<String>().replace('\n', " / "));
        }
        if !ok {
            println!("    said: {:?}", t.stage.texts(ItemKind::Prose).last());
        }
    }
    println!("{good} of {} with {model}", TASKS.len());
}
