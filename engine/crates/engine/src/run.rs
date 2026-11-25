//! One run: from what a person said until the agent has nothing more to do.

use std::{
    pin::Pin,
    time::{Duration, Instant},
};

use axiom_model::{
    Accumulator, Args, Block, CallError, Class, Client, Content, Delta, Event, Message, Reply,
    Request, Retry, Role, Settings, Start, Stop, Tool,
};
use axiom_policy::{Ask, Verdict};
use axiom_proto::{
    AgentId, AgentState, Answerable, Body, ItemId, ItemKind, Outcome, Provenance, RequestId, RunId,
    Usage,
};
use axiom_tools::{Output, Plan, Seen, Site, Spec, Step};
use serde_json::{Value, json};
use tokio::sync::{mpsc, watch};

use crate::{
    Deed, Desk, Refused, Say, Stage,
    context::{Context, Turn},
};

/// How far a run may go.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Answers of the model.
    pub steps: u32,
    pub time: Duration,
    /// Of all answers together, as the provider counts them.
    pub output_tokens: Option<u64>,
}
impl Default for Limits {
    fn default() -> Self {
        Self { steps: 200, time: Duration::from_secs(4 * 3600), output_tokens: None }
    }
}

/// How a failed request is tried again.
#[derive(Debug, Clone, Copy)]
pub struct Retries {
    pub most: u32,
    pub first: Duration,
    /// Longer than this is not waited, also if the provider asks for it.
    pub longest: Duration,
}
impl Default for Retries {
    fn default() -> Self {
        Self { most: 6, first: Duration::from_secs(1), longest: Duration::from_secs(120) }
    }
}

/// An agent, ready to run.
pub struct Agent<S: Stage> {
    pub id: AgentId,
    pub stage: S,
    pub desk: Desk,
    pub client: Client,
    pub model: String,
    pub settings: Settings,
    /// Tokens the model takes, if they are known.
    pub window: Option<u64>,
    pub site: Site,
    pub seen: Seen,
    pub context: Context,
    pub specs: Vec<Spec>,
    /// Said by the person of this agent's work, beside what Axiom says.
    pub instructions: String,
    pub limits: Limits,
    pub retries: Retries,
    /// What a person says meanwhile.
    pub inbox: mpsc::Receiver<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ended {
    pub outcome: Outcome,
    pub error: Option<String>,
}

/// Why a run stops before the agent is done.
enum Halt {
    Cancelled,
    Failed(String),
    /// Nobody to say anything to.
    Gone,
}
impl From<Refused> for Halt {
    fn from(r: Refused) -> Self {
        match r {
            Refused::Closed => Halt::Gone,
            Refused::This(why) => Halt::Failed(why),
        }
    }
}

type Cancel = watch::Receiver<bool>;

async fn stopped(c: &mut Cancel) {
    if c.wait_for(|c| *c).await.is_err() {
        // Nobody can cancel it any more.
        std::future::pending::<()>().await;
    }
}

impl<S: Stage> Agent<S> {
    /// Runs until the agent is done, cancelled, or cannot go on. How it ended is in the journal when
    /// this returns. `run.started` is said by who starts it.
    pub async fn run(&mut self, run: RunId, prompt: String, mut cancel: Cancel) -> Ended {
        let r = self.go(&run, prompt, &mut cancel).await;
        let (outcome, error) = match r {
            Ok(()) => (Outcome::Completed, None),
            Err(Halt::Cancelled) => (Outcome::Cancelled, None),
            Err(Halt::Failed(why)) => (Outcome::Failed, Some(why)),
            Err(Halt::Gone) => return Ended { outcome: Outcome::Interrupted, error: None },
        };
        let state = match outcome {
            Outcome::Completed => AgentState::Done,
            Outcome::Failed => AgentState::Failed,
            _ => AgentState::Paused,
        };
        let say = vec![
            Say::Event(Body::RunEnded { run, outcome, error: error.clone() }),
            Say::Event(Body::AgentState { agent: self.id.clone(), state }),
        ];
        if self.stage.say(say).await.is_ok() {
            let _ = self.stage.sync().await;
        }
        Ended { outcome, error }
    }

    async fn go(&mut self, run: &RunId, prompt: String, cancel: &mut Cancel) -> Result<(), Halt> {
        let began = Instant::now();
        let mut output = 0;
        self.heard(run, prompt).await?;
        for _ in 0..self.limits.steps {
            if *cancel.borrow() {
                return Err(Halt::Cancelled);
            }
            let mut more = Vec::new();
            while let Ok(m) = self.inbox.try_recv() {
                more.push(m);
            }
            for m in more {
                self.heard(run, m).await?;
            }
            if began.elapsed() > self.limits.time {
                return Err(Halt::Failed("the run has taken as long as it may".into()));
            }
            if self.limits.output_tokens.is_some_and(|most| output >= most) {
                return Err(Halt::Failed("the run has used as many tokens as it may".into()));
            }
            let reply = self.answer(run, cancel).await?;
            output += reply.usage.output.unwrap_or(0);
            self.usage(&reply).await?;
            let stop = reply.stop.clone();
            let message = reply.message;
            self.remember(Turn::Message(message.clone())).await?;
            let has_calls =
                message.blocks.iter().any(|b| matches!(b.content, Content::ToolCall(_)));
            if has_calls {
                self.calls(run, &message, cancel).await?;
                continue;
            }
            match stop {
                Some(Stop::EndTurn | Stop::Sequence(_) | Stop::ToolUse) | None => {
                    // What was said while it answered is heard before it is done.
                    match self.inbox.try_recv() {
                        Ok(m) => self.heard(run, m).await?,
                        Err(_) => return Ok(()),
                    }
                }
                Some(Stop::Pause) => {}
                Some(Stop::MaxOutput) => {
                    self.notice(run, "The answer reached its longest length; the agent goes on.")
                        .await?;
                    self.remember(Turn::Message(Message::user("Go on."))).await?;
                }
                Some(Stop::ContextFull) => {
                    return Err(Halt::Failed("the model's context is full".into()));
                }
                Some(Stop::Refusal(why)) => {
                    let why = why.map(|w| format!(" ({w})")).unwrap_or_default();
                    return Err(Halt::Failed(format!("the provider declined to answer{why}")));
                }
                Some(Stop::Other(why)) => {
                    return Err(Halt::Failed(format!("the model stopped: {why}")));
                }
            }
        }
        Err(Halt::Failed(format!(
            "the run has taken {} steps, as many as it may",
            self.limits.steps
        )))
    }

    /// Takes in what a person said.
    async fn heard(&mut self, run: &RunId, text: String) -> Result<(), Halt> {
        let item = ItemId::new();
        self.stage
            .say(vec![
                Say::Event(self.item(run, &item, ItemKind::Prompt, "", None)),
                Say::Text(item.clone(), text.clone()),
                Say::End(item, Outcome::Completed, None),
            ])
            .await?;
        self.remember(Turn::Message(Message::user(text))).await
    }

    async fn notice(&self, run: &RunId, text: &str) -> Result<(), Halt> {
        let item = ItemId::new();
        let say = vec![
            Say::Event(self.item(run, &item, ItemKind::Notice, "", None)),
            Say::Text(item.clone(), text.into()),
            Say::End(item, Outcome::Completed, None),
        ];
        Ok(self.stage.say(say).await?)
    }

    async fn state(&self, state: AgentState) -> Result<(), Halt> {
        let b = Body::AgentState { agent: self.id.clone(), state };
        Ok(self.stage.say(vec![Say::Event(b)]).await?)
    }

    fn item(
        &self,
        run: &RunId,
        item: &ItemId,
        kind: ItemKind,
        name: &str,
        args: Option<Value>,
    ) -> Body {
        Body::ItemStarted {
            item: item.clone(),
            run: run.clone(),
            agent: self.id.clone(),
            kind,
            name: name.into(),
            args,
            args_cut: 0,
        }
    }

    /// On the drive before anything follows from it.
    async fn remember(&mut self, t: Turn) -> Result<(), Halt> {
        let text = serde_json::to_string(&t).map_err(|e| Halt::Failed(e.to_string()))?;
        self.stage.remember(self.context.turns(), text).await?;
        self.context.push(&t);
        Ok(())
    }

    fn request(&self) -> Result<Request, Halt> {
        let tools = self
            .specs
            .iter()
            .filter(|s| self.site.caps.tools.has(s.name))
            .map(|s| {
                let schema =
                    axiom_model::Json::of(&s.schema).map_err(|e| Halt::Failed(e.to_string()))?;
                Ok(Tool { name: s.name.into(), description: s.description.into(), schema })
            })
            .collect::<Result<_, Halt>>()?;
        Ok(Request {
            model: self.model.clone(),
            system: crate::context::system(&self.site, &self.instructions),
            tools,
            messages: self.context.messages().to_vec(),
            settings: self.settings.clone(),
        })
    }

    /// Asks the model, again if that may help. What came of a try that failed is marked as such.
    async fn answer(&mut self, run: &RunId, cancel: &mut Cancel) -> Result<Reply, Halt> {
        let request = self.request()?;
        let mut tries = 0;
        loop {
            let (retry, class, words, after) = match self.one(run, &request, cancel).await? {
                Ok(r) => return Ok(r),
                Err(CallError::Encode(e)) => return Err(Halt::Failed(e.to_string())),
                Err(CallError::Provider(e)) => (e.retry, e.class, e.to_string(), e.retry_after),
            };
            tries += 1;
            if retry == Retry::Never || tries > self.retries.most {
                return Err(Halt::Failed(words));
            }
            let wait = after.unwrap_or_else(|| self.retries.first * (1 << (tries - 1).min(10)));
            if wait > self.retries.longest {
                return Err(Halt::Failed(format!(
                    "{words} (it asks to be left alone for {} s)",
                    wait.as_secs()
                )));
            }
            let what = match class {
                Class::RateLimit => "The provider limits how often it is asked",
                Class::Overloaded => "The provider has too much to do",
                Class::Network => "The provider was not reached",
                Class::CutShort => "The answer broke off",
                _ => "The provider failed",
            };
            self.notice(
                run,
                &format!(
                    "{what}. Trying again in {:.0} s ({tries} of {}).",
                    wait.as_secs_f64().ceil(),
                    self.retries.most
                ),
            )
            .await?;
            tokio::select! {
                () = tokio::time::sleep(wait) => {}
                () = stopped(cancel) => return Err(Halt::Cancelled),
            }
        }
    }

    /// One request and its answer, said as it comes.
    async fn one(
        &mut self,
        run: &RunId,
        request: &Request,
        cancel: &mut Cancel,
    ) -> Result<Result<Reply, CallError>, Halt> {
        let mut answer = tokio::select! {
            biased;
            () = stopped(cancel) => return Err(Halt::Cancelled),
            a = self.client.stream(request) => match a {
                Ok(a) => a,
                Err(e) => return Ok(Err(e)),
            },
        };
        let mut fold = Accumulator::new(answer.scope().clone());
        // By block: the item it is shown as, and whether it has ended.
        let mut items: Vec<Option<ItemId>> = Vec::new();
        let failed = loop {
            let e = tokio::select! {
                biased;
                () = stopped(cancel) => {
                    // The connection closes first.
                    drop(answer);
                    self.end_all(&mut items, Outcome::Cancelled).await?;
                    return self.keep(fold.finish()).await;
                }
                e = answer.next() => e,
            };
            let e = match e {
                None => break None,
                Some(Err(e)) => break Some(e),
                Some(Ok(e)) => e,
            };
            if fold.push(&e).is_err() {
                return Err(Halt::Failed("the answer cannot be read".into()));
            }
            self.show(run, &e, &mut items).await?;
        };
        if let Some(e) = failed {
            self.end_all(&mut items, Outcome::Failed).await?;
            return Ok(Err(CallError::Provider(e)));
        }
        self.end_all(&mut items, Outcome::Completed).await?;
        Ok(Ok(fold.finish()))
    }

    /// What a cancelled answer leaves: the words it had said, so that the agent knows them too.
    async fn keep(&mut self, r: Reply) -> Result<Result<Reply, CallError>, Halt> {
        let text: Vec<Block> = r
            .message
            .blocks
            .into_iter()
            .filter_map(|b| match b.content {
                Content::Text { text } if !text.trim().is_empty() => Some(Block::text(text)),
                _ => None,
            })
            .collect();
        if !text.is_empty() {
            self.remember(Turn::Message(Message { role: Role::Assistant, blocks: text })).await?;
        }
        Err(Halt::Cancelled)
    }

    async fn end_all(&self, items: &mut [Option<ItemId>], outcome: Outcome) -> Result<(), Halt> {
        let say: Vec<Say> =
            items.iter_mut().filter_map(Option::take).map(|i| Say::End(i, outcome, None)).collect();
        if !say.is_empty() {
            self.stage.say(say).await?;
        }
        Ok(())
    }

    async fn show(
        &self,
        run: &RunId,
        e: &Event,
        items: &mut Vec<Option<ItemId>>,
    ) -> Result<(), Halt> {
        let say = match e {
            Event::BlockStart { start, .. } => {
                // A call is shown when it is judged, whole.
                let kind = match start {
                    Start::Text => Some(ItemKind::Prose),
                    Start::Reasoning => Some(ItemKind::Reasoning),
                    Start::ToolCall { .. } | Start::Unknown { .. } => None,
                };
                let item = kind.map(|_| ItemId::new());
                items.push(item.clone());
                match (kind, item) {
                    (Some(k), Some(i)) => vec![Say::Event(self.item(run, &i, k, "", None))],
                    _ => return Ok(()),
                }
            }
            Event::Delta { block, delta: Delta::Text(t) | Delta::Reasoning(t) } => {
                match items.get(*block).and_then(Option::as_ref) {
                    Some(i) if !t.is_empty() => vec![Say::Text(i.clone(), t.clone())],
                    _ => return Ok(()),
                }
            }
            Event::BlockStop { block, .. } => match items.get_mut(*block).and_then(Option::take) {
                Some(i) => vec![Say::End(i, Outcome::Completed, None)],
                None => return Ok(()),
            },
            _ => return Ok(()),
        };
        Ok(self.stage.say(say).await?)
    }

    async fn usage(&self, r: &Reply) -> Result<(), Halt> {
        let u = &r.usage;
        if *u == axiom_model::Usage::default() {
            return Ok(());
        }
        let full = u.prompt.zip(u.output).map(|(p, o)| p + o);
        let context = full
            .zip(self.window)
            .filter(|(_, w)| *w > 0)
            .map(|(f, w)| (f as f64 / w as f64).min(1.0));
        let usage = Usage {
            input: u.uncached.or(u.prompt),
            cache_read: u.cache_read,
            cache_write: u.cache_write,
            output: u.output,
            reasoning: u.reasoning,
            context,
        };
        let b = Body::Usage { agent: self.id.clone(), usage, provenance: Provenance::Reported };
        Ok(self.stage.say(vec![Say::Event(b)]).await?)
    }

    /// Does what the answer asks for, in its order. Every call gets a result, whatever happens.
    async fn calls(&mut self, run: &RunId, m: &Message, cancel: &mut Cancel) -> Result<(), Halt> {
        let mut halt = None;
        for b in &m.blocks {
            let Content::ToolCall(c) = &b.content else { continue };
            if halt.is_some() {
                let not = "Not done: the run was stopped before it.";
                self.result(&c.id, not, true).await?;
                continue;
            }
            let item = ItemId::new();
            let args = match &c.args {
                Args::Ready(a) => Some(Value::Object(a.clone())),
                _ => Some(Value::String(c.raw.clone())),
            };
            // Intent before effect: on the drive before it is looked at any further.
            self.stage
                .say(vec![Say::Event(self.item(run, &item, ItemKind::Tool, &c.name, args))])
                .await?;
            let out = match c.ready() {
                None => {
                    let why = match &c.args {
                        Args::Broken(why) => why.clone(),
                        _ => "the answer ended inside it".into(),
                    };
                    Ok(Output { ok: false, text: format!("Not done: {why}."), detail: Value::Null })
                }
                Some(call) => {
                    let args = Value::Object(call.args().clone());
                    self.call(&item, call.name(), &args, cancel).await
                }
            };
            let (text, failed) = match out {
                Ok(o) => {
                    let outcome = if o.ok { Outcome::Completed } else { Outcome::Failed };
                    let mut result = o.detail;
                    if !result.is_object() {
                        result = json!({});
                    }
                    result["ok"] = o.ok.into();
                    self.stage
                        .say(vec![
                            Say::Text(item.clone(), o.text.clone()),
                            Say::End(item, outcome, Some(result)),
                        ])
                        .await?;
                    (o.text, !o.ok)
                }
                Err(Halt::Cancelled) => {
                    self.stage.say(vec![Say::End(item, Outcome::Cancelled, None)]).await?;
                    halt = Some(Halt::Cancelled);
                    ("Stopped by a person while it was being done. It may have been done in part.".into(), true)
                }
                Err(Halt::Gone) => return Err(Halt::Gone),
                Err(h @ Halt::Failed(_)) => {
                    self.stage.say(vec![Say::End(item, Outcome::Failed, None)]).await?;
                    halt = Some(h);
                    ("Not done: Axiom could not go on.".into(), true)
                }
            };
            self.result(&c.id, &text, failed).await?;
        }
        halt.map_or(Ok(()), Err)
    }

    /// By itself, at once: what was done is known though the host is killed before the next call.
    async fn result(&mut self, id: &str, text: &str, failed: bool) -> Result<(), Halt> {
        let blocks = vec![Block::result(id, text, failed)];
        self.remember(Turn::Message(Message { role: Role::User, blocks })).await
    }

    async fn call(
        &mut self,
        item: &ItemId,
        tool: &str,
        args: &Value,
        cancel: &mut Cancel,
    ) -> Result<Output, Halt> {
        let (site, seen) = (self.site.clone(), self.seen.clone());
        let (t, a) = (tool.to_owned(), args.clone());
        // Files are looked at off the runtime's own threads.
        let plan = tokio::task::spawn_blocking(move || axiom_tools::plan(&t, &a, &site, &seen))
            .await
            .map_err(|_| Halt::Failed("the tool failed".into()))?;
        let (step, verdict, answered) = match plan {
            Plan::No(why) => {
                return Ok(Output {
                    ok: false,
                    text: format!("Not done: {why}."),
                    detail: Value::Null,
                });
            }
            Plan::Go(step, verdict) => (step, verdict, None),
            Plan::Ask(ask, step, verdict) => {
                if self.desk.allowed(&ask) {
                    (step, verdict, None)
                } else {
                    match self.ask(item, tool, &ask, &verdict, cancel).await? {
                        Ok(id) => (step, verdict, Some((id, ask.digest()))),
                        Err(why) => {
                            return Ok(Output { ok: false, text: why, detail: Value::Null });
                        }
                    }
                }
            }
        };
        self.perform(tool, args, step, verdict, answered, cancel).await
    }

    /// A person's yes, or the words for the model.
    async fn ask(
        &self,
        item: &ItemId,
        tool: &str,
        ask: &Ask,
        verdict: &Verdict,
        cancel: &mut Cancel,
    ) -> Result<Result<RequestId, String>, Halt> {
        let mut open = match self.desk.open(ask) {
            Ok(o) => o,
            Err(e) => return Ok(Err(format!("Not done: {e}."))),
        };
        let id = open.id.clone();
        let what = match tool {
            "shell" => "Run a command",
            "write" | "edit" | "patch" => "Change files",
            "read" | "list" | "glob" | "search" => "Look at files",
            _ => "Use a tool",
        };
        let opened = self
            .stage
            .say(vec![
                Say::Event(Body::RequestOpened {
                    request: id.clone(),
                    agent: self.id.clone(),
                    what: format!("{what} ({})", verdict.why),
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
        let answer = tokio::select! {
            biased;
            () = stopped(cancel) => None,
            a = open.wait() => Some(a),
        };
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
            return Err(Halt::Cancelled);
        }
        self.state(AgentState::Running).await?;
        if approval.is_some_and(|a| self.desk.spend(a, ask)) {
            return Ok(Ok(id));
        }
        Ok(Err("Not done: a person said no. Do not try the same by another way; ask what they want instead.".into()))
    }

    async fn perform(
        &mut self,
        tool: &str,
        args: &Value,
        step: Step,
        verdict: Verdict,
        answered: Option<(RequestId, String)>,
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
        let mut work: Pin<Box<dyn Future<Output = Output> + Send>> =
            Box::pin(async move { step.run(&site, &seen, |_| {}).await });
        tokio::select! {
            biased;
            () = stopped(cancel) => {
                // Ends what it started.
                drop(work);
                Err(Halt::Cancelled)
            }
            o = &mut work => Ok(o),
        }
    }
}
