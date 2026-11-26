//! Claude Code's own list of things to do, as its tools make it (code.claude.com/docs/en/agent-sdk/
//! todo-tracking, read 2026-11-26): `TodoWrite`, which says all of it, or `TaskCreate` and
//! `TaskUpdate`, which say a step of it. A call counts when its result says that it was done. What the
//! model writes may call an id by another name, which Claude Code repairs and the stream does not.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::{
    event::{Plan, Progress},
    talk::{list, owned, text},
};

/// Calls that wait for their result, and steps.
const CALLS: usize = 64;
const STEPS: usize = 200;

#[derive(Debug, Default)]
pub(crate) struct Tasks {
    calls: BTreeMap<String, (String, Value)>,
    steps: Vec<(String, String, Progress)>,
}

fn progress(s: Option<&str>) -> Progress {
    match s {
        Some("pending") => Progress::Todo,
        Some("in_progress") => Progress::Doing,
        Some("completed") => Progress::Done,
        _ => Progress::Unknown,
    }
}

impl Tasks {
    pub fn called(&mut self, id: &str, name: &str, input: Option<&Value>) {
        if matches!(name, "TodoWrite" | "TaskCreate" | "TaskUpdate")
            && self.calls.len() < CALLS
            && let Some(i) = input
        {
            self.calls.insert(id.to_owned(), (name.to_owned(), i.clone()));
        }
    }

    /// The list, if it is another now. `result`: what the call came to, as it is told apart from its
    /// words.
    pub fn ended(&mut self, id: &str, well: bool, result: &Value) -> Option<Plan> {
        let (name, input) = self.calls.remove(id)?;
        if !well {
            return None;
        }
        match name.as_str() {
            "TodoWrite" => {
                self.steps = list(&input, "/todos")
                    .iter()
                    .take(STEPS)
                    .map(|t| {
                        let words = owned(t, "/content").unwrap_or_default();
                        (String::new(), words, progress(text(t, "/status")))
                    })
                    .collect();
            }
            "TaskCreate" => {
                let words = owned(result, "/task/subject").or_else(|| owned(&input, "/subject"))?;
                let id = owned(result, "/task/id")?;
                if self.steps.len() >= STEPS {
                    return None;
                }
                self.steps.push((id, words, Progress::Todo));
            }
            _ => {
                let id = ["/taskId", "/task_id", "/id"].iter().find_map(|k| text(&input, k))?;
                let at = self.steps.iter().position(|s| s.0 == id)?;
                if text(&input, "/status") == Some("deleted") {
                    self.steps.remove(at);
                } else {
                    let s = &mut self.steps[at];
                    if let Some(w) = owned(&input, "/subject") {
                        s.1 = w;
                    }
                    if let Some(p) = text(&input, "/status") {
                        s.2 = progress(Some(p));
                    }
                }
            }
        }
        let steps = self.steps.iter().map(|s| (s.1.clone(), s.2)).collect();
        Some(Plan { id: "tasks".into(), steps, text: None })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use serde_json::json;

    fn steps(p: Option<Plan>) -> Vec<(String, Progress)> {
        p.unwrap().steps
    }
    fn s(t: &str, p: Progress) -> (String, Progress) {
        (t.into(), p)
    }

    #[test]
    fn all_of_it_at_once() {
        let mut t = Tasks::default();
        let todo = |c: &str, s: &str| json!({"content": c, "status": s, "activeForm": "x"});
        let all =
            json!({"todos": [todo("a", "completed"), todo("b", "in_progress"), todo("c", "odd")]});
        t.called("1", "TodoWrite", Some(&all));
        assert_eq!(
            steps(t.ended("1", true, &Value::Null)),
            [s("a", Progress::Done), s("b", Progress::Doing), s("c", Progress::Unknown)]
        );
        // What was refused changes nothing; what is not about it is nothing.
        t.called("2", "TodoWrite", Some(&json!({"todos": []})));
        assert_eq!(t.ended("2", false, &Value::Null), None);
        t.called("3", "Bash", Some(&json!({"todos": []})));
        assert_eq!(t.ended("3", true, &Value::Null), None);
        t.called("4", "TodoWrite", Some(&json!({"todos": []})));
        assert_eq!(steps(t.ended("4", true, &Value::Null)), []);
    }

    #[test]
    fn step_by_step() {
        let mut t = Tasks::default();
        for (call, id, words) in [("1", "7", "Look"), ("2", "8", "Do")] {
            let input = json!({"subject": words, "description": "d"});
            t.called(call, "TaskCreate", Some(&input));
            t.ended(call, true, &json!({"task": {"id": id, "subject": words}})).unwrap();
        }
        // One without an id is no step.
        t.called("3", "TaskCreate", Some(&json!({"subject": "x"})));
        assert_eq!(t.ended("3", true, &Value::Null), None);
        let mut update = |input: Value| {
            t.called("4", "TaskUpdate", Some(&input));
            t.ended("4", true, &json!({"success": true}))
        };
        assert_eq!(
            steps(update(json!({"taskId": "7", "status": "completed"}))),
            [s("Look", Progress::Done), s("Do", Progress::Todo)]
        );
        assert_eq!(
            steps(update(json!({"task_id": "8", "status": "in_progress", "subject": "Do it"}))),
            [s("Look", Progress::Done), s("Do it", Progress::Doing)]
        );
        assert_eq!(update(json!({"id": "9", "status": "completed"})), None);
        assert_eq!(update(json!({"status": "completed"})), None);
        assert_eq!(
            steps(update(json!({"id": "7", "status": "deleted"}))),
            [s("Do it", Progress::Doing)]
        );
    }
}
