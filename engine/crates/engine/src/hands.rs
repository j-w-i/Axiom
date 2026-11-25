//! What any loop's agent does its work with: a call is put on record, judged, asked about and done in
//! one way, whoever asks for it.

use std::{
    pin::Pin,
    time::{Duration, Instant},
};

use axiom_policy::{Ask, Verdict, rules::Decision};
use axiom_proto::{
    AgentId, AgentState, Answerable, Body, ItemId, ItemKind, Outcome, RequestId, RunId,
};
use axiom_tools::{Output, Plan, Seen, Site, Spec, Step};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::{
    Deed, Desk, Other, Refused, Say, Stage,
    run::{ASKED, Cancel, Halt, stopped},
};

/// Of one agent, as it is now.
pub struct Hands<'a, S> {
    pub id: &'a AgentId,
    pub stage: &'a S,
    pub desk: &'a Desk,
    pub site: &'a Site,
    pub seen: &'a Seen,
    pub specs: &'a [Spec],
}

/// What a call came to, as the model is told, and whether its run may go on.
pub(crate) struct Done {
    pub text: String,
    pub failed: bool,
    pub halt: Option<Halt>,
}

pub(crate) fn begins(
    agent: &AgentId,
    run: &RunId,
    item: &ItemId,
    kind: ItemKind,
    name: &str,
    args: Option<Value>,
) -> Body {
    Body::ItemStarted {
        item: item.clone(),
        run: run.clone(),
        agent: agent.clone(),
        kind,
        name: name.into(),
        args,
        args_cut: 0,
    }
}

/// A call that may be done, and why.
struct Ready {
    step: Step,
    verdict: Verdict,
    /// The question a person answered, and the digest of what they saw.
    answered: Option<(RequestId, String)>,
}

impl<S: Stage> Hands<'_, S> {
    /// For a loop that is not Axiom's. What the model is told, and whether it was done.
    pub async fn tool(
        &self,
        run: &RunId,
        name: &str,
        args: &Value,
        cancel: &mut Cancel,
    ) -> Result<(String, bool), Refused> {
        let ready = match args {
            Value::Object(_) => Ok(args.clone()),
            _ => Err("the arguments are not an object".to_owned()),
        };
        let mut waited = Duration::ZERO;
        match self.one(run, name, args.clone(), ready, cancel, &mut waited).await {
            Ok(d) => Ok((d.text, !d.failed)),
            Err(Halt::Failed(why) | Halt::Full(why)) => Err(Refused::This(why)),
            Err(Halt::Gone | Halt::Cancelled | Halt::Unbegun) => Err(Refused::Closed),
        }
    }

    /// One call, from its first word on the record to its last. `args`: or why they cannot be read.
    pub(crate) async fn one(
        &self,
        run: &RunId,
        name: &str,
        shown: Value,
        args: Result<Value, String>,
        cancel: &mut Cancel,
        waited: &mut Duration,
    ) -> Result<Done, Halt> {
        let item = ItemId::new();
        // Intent before effect: on the drive before it is looked at any further.
        let begin = begins(self.id, run, &item, ItemKind::Tool, name, Some(shown));
        self.stage.say(vec![Say::Event(begin)]).await?;
        let out = match args {
            Err(why) => Ok(Output {
                ok: false,
                text: format!("Not done: {why}."),
                rest: None,
                detail: Value::Null,
            }),
            Ok(args) => self.call(&item, name, &args, cancel, waited).await,
        };
        let (text, failed, halt) = match out {
            Ok(o) => {
                let outcome = if o.ok { Outcome::Completed } else { Outcome::Failed };
                let made: Vec<String> = o.detail["made"]
                    .as_array()
                    .map(|m| m.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
                    .unwrap_or_default();
                let mut result = o.detail;
                if !result.is_object() {
                    result = json!({});
                }
                result["ok"] = o.ok.into();
                self.stage
                    .say(vec![
                        Say::Text(item.clone(), o.rest.unwrap_or_else(|| o.text.clone())),
                        Say::End(item, outcome, Some(result)),
                    ])
                    .await?;
                if !made.is_empty() {
                    let words = format!(
                        "This command made {}. Other programs take orders from such files: look \
                         at them before you open the folder elsewhere.",
                        made.join(", ")
                    );
                    let notice = ItemId::new();
                    let begin = begins(self.id, run, &notice, ItemKind::Notice, "", None);
                    self.stage
                        .say(vec![
                            Say::Event(begin),
                            Say::Text(notice.clone(), words),
                            Say::End(notice, Outcome::Completed, None),
                        ])
                        .await?;
                }
                (o.text, !o.ok, None)
            }
            Err(Halt::Unbegun) => {
                self.stage.say(vec![Say::End(item, Outcome::Cancelled, None)]).await?;
                (ASKED.into(), true, Some(Halt::Cancelled))
            }
            Err(Halt::Cancelled) => {
                self.stage.say(vec![Say::End(item, Outcome::Cancelled, None)]).await?;
                let words =
                    "Stopped by a person while it was being done. It may have been done in part.";
                (words.into(), true, Some(Halt::Cancelled))
            }
            Err(Halt::Gone) => return Err(Halt::Gone),
            Err(h @ (Halt::Failed(_) | Halt::Full(_))) => {
                self.stage.say(vec![Say::End(item, Outcome::Failed, None)]).await?;
                ("Not done: Axiom could not go on.".into(), true, Some(h))
            }
        };
        Ok(Done { text, failed, halt })
    }

    async fn call(
        &self,
        item: &ItemId,
        tool: &str,
        args: &Value,
        cancel: &mut Cancel,
        waited: &mut Duration,
    ) -> Result<Output, Halt> {
        if let Some(o) = self.other(tool, args, cancel).await? {
            return self.others(item, tool, args, o, cancel, waited).await;
        }
        let (site, seen) = (self.site.clone(), self.seen.clone());
        let (t, a) = (tool.to_owned(), args.clone());
        // Files are looked at off the runtime's own threads.
        let plan = tokio::task::spawn_blocking(move || axiom_tools::plan(&t, &a, &site, &seen));
        // Planning changes nothing: whoever stops does not wait for it.
        let plan = tokio::select! {
            biased;
            () = stopped(cancel) => return Err(Halt::Unbegun),
            () = self.stage.closed() => return Err(Halt::Gone),
            p = plan => p.map_err(|_| Halt::Failed("the tool failed".into()))?,
        };
        let (step, verdict, answered) = match plan {
            Plan::No(why) => {
                return Ok(Output {
                    ok: false,
                    text: format!("Not done: {why}."),
                    rest: None,
                    detail: Value::Null,
                });
            }
            Plan::Go(step, verdict) => (step, verdict, None),
            Plan::Ask(ask, step, verdict) => {
                if self.desk.allowed(&ask) {
                    (step, verdict, None)
                } else {
                    let what = match tool {
                        "shell" | "job" => "asks to run a command",
                        "write" | "edit" | "patch" => "asks to change files",
                        "read" | "list" | "glob" | "search" => "asks to look at files",
                        "fetch" => "asks to read from the web",
                        _ => "asks to use a tool",
                    };
                    match self.ask_person(item, what, &ask, cancel, waited).await? {
                        Ok(id) => (step, verdict, Some((id, ask.digest()))),
                        Err(why) => {
                            return Ok(Output {
                                ok: false,
                                text: why,
                                rest: None,
                                detail: Value::Null,
                            });
                        }
                    }
                }
            }
        };
        self.perform(item, tool, args, Ready { step, verdict, answered }, cancel).await
    }

    async fn other(
        &self,
        tool: &str,
        args: &Value,
        cancel: &mut Cancel,
    ) -> Result<Option<Other>, Halt> {
        tokio::select! {
            biased;
            () = stopped(cancel) => Err(Halt::Unbegun),
            () = self.stage.closed() => Err(Halt::Gone),
            o = self.stage.judge(tool, args) => Ok(o),
        }
    }

    /// A tool of the host's own.
    async fn others(
        &self,
        item: &ItemId,
        tool: &str,
        args: &Value,
        o: Other,
        cancel: &mut Cancel,
        waited: &mut Duration,
    ) -> Result<Output, Halt> {
        let no = |text: String| Output { ok: false, text, rest: None, detail: Value::Null };
        let mut answered = None;
        match o.decision {
            Decision::Deny => return Ok(no(format!("Not done: {}.", o.why))),
            Decision::Allow => {}
            Decision::Ask => {
                let schema = self.specs.iter().find(|s| s.name == tool);
                let ask = Ask {
                    chain: self.site.chain.clone(),
                    workspace: self.site.workspace.to_string(),
                    tool: tool.into(),
                    schema: schema.map(axiom_tools::schema_digest).unwrap_or_default(),
                    subject: o.subject.clone(),
                    scope: format!("{}. Asked because {}.", o.scope, o.why),
                    unsandboxed: false,
                };
                if !self.desk.allowed(&ask) {
                    match self.ask_person(item, &o.what, &ask, cancel, waited).await? {
                        Ok(id) => answered = Some((id, ask.digest())),
                        Err(why) => return Ok(no(why)),
                    }
                }
            }
        }
        if o.changes {
            self.stage
                .audit(Deed {
                    chain: self.site.chain.clone(),
                    workspace: self.site.workspace.to_string(),
                    tool: tool.into(),
                    subject: o.subject,
                    decision: o.decision,
                    rules: vec![],
                    answered,
                })
                .await?;
        }
        self.stage.sync().await?;
        let began = Instant::now();
        let out = tokio::select! {
            biased;
            () = stopped(cancel) => return Err(Halt::Cancelled),
            () = self.stage.closed() => return Err(Halt::Gone),
            out = self.stage.perform(tool, args) => out,
        };
        // Others' work is not this run's own time.
        *waited += began.elapsed();
        Ok(out)
    }

    /// A person's yes, or the words for the model.
    async fn ask_person(
        &self,
        item: &ItemId,
        what: &str,
        ask: &Ask,
        cancel: &mut Cancel,
        waited: &mut Duration,
    ) -> Result<Result<RequestId, String>, Halt> {
        let mut open = match self.desk.open(ask) {
            Ok(o) => o,
            Err(e) => return Ok(Err(format!("Not done: {e}."))),
        };
        let id = open.id.clone();
        let opened = self
            .stage
            .say(vec![
                Say::Event(Body::RequestOpened {
                    request: id.clone(),
                    agent: self.id.clone(),
                    what: what.into(),
                    subject: ask.subject.clone(),
                    scope: ask.scope.clone(),
                    answerable: Answerable::Live,
                    digest: ask.digest(),
                    item: Some(item.clone()),
                    session: !ask.unsandboxed,
                }),
                Say::Event(Body::AgentState { agent: self.id.clone(), state: AgentState::Asking }),
            ])
            .await;
        match opened {
            Ok(()) => {}
            Err(Refused::Closed) => return Err(Halt::Gone),
            // Longer than a person can be shown whole.
            Err(Refused::This(why)) => {
                return Ok(Err(format!("Not done: {why}. Make it smaller, in several steps.")));
            }
        }
        self.stage.sync().await?;
        let asked = Instant::now();
        let answer = tokio::select! {
            biased;
            () = stopped(cancel) => None,
            () = self.stage.closed() => return Err(Halt::Gone),
            a = open.wait() => Some(a),
        };
        *waited += asked.elapsed();
        let cancelled = answer.is_none();
        let approval = answer.flatten();
        let words = if cancelled {
            "expired"
        } else if approval.is_some() {
            "allowed"
        } else {
            "deny"
        };
        self.stage
            .say(vec![Say::Event(Body::RequestClosed {
                request: id.clone(),
                answer: words.into(),
            })])
            .await?;
        if cancelled {
            return Err(Halt::Unbegun);
        }
        let b = Body::AgentState { agent: self.id.clone(), state: AgentState::Running };
        self.stage.say(vec![Say::Event(b)]).await?;
        if approval.is_some_and(|a| self.desk.spend(a, ask)) {
            return Ok(Ok(id));
        }
        Ok(Err("Not done: a person said no. Do not try the same by another way; ask what they want instead.".into()))
    }

    async fn perform(
        &self,
        item: &ItemId,
        tool: &str,
        args: &Value,
        Ready { step, verdict, answered }: Ready,
        cancel: &mut Cancel,
    ) -> Result<Output, Halt> {
        if step.changes() {
            self.stage
                .audit(Deed {
                    chain: self.site.chain.clone(),
                    workspace: self.site.workspace.to_string(),
                    tool: tool.into(),
                    subject: args.to_string(),
                    decision: verdict.decision,
                    rules: verdict.rules,
                    answered,
                })
                .await?;
        }
        self.stage.sync().await?;
        let (site, seen) = (self.site.clone(), self.seen.clone());
        let (heard, mut hears) = mpsc::channel(16);
        let mut work: Pin<Box<dyn Future<Output = Output> + Send>> =
            Box::pin(async move { step.run(&site, &seen, heard).await });
        // What comes after the work has ended is still said before its end.
        let mut done = None;
        let mut open = true;
        loop {
            tokio::select! {
                biased;
                () = self.stage.closed(), if done.is_none() => {
                    drop(work);
                    return Err(Halt::Gone);
                }
                () = stopped(cancel), if done.is_none() => {
                    // Ends what it started.
                    drop(work);
                    return Err(Halt::Cancelled);
                }
                more = hears.recv(), if open => match more {
                    Some(t) => self.stage.say(vec![Say::Text(item.clone(), t)]).await?,
                    None => open = false,
                },
                o = &mut work, if done.is_none() => done = Some(o),
                else => break,
            }
        }
        done.ok_or_else(|| Halt::Failed("the tool failed".into()))
    }
}
