//! Events folded into the message they tell.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::types::*;

/// An answer, whole or as far as it came.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub id: Option<String>,
    pub model: Option<String>,
    pub message: Message,
    pub stop: Option<Stop>,
    pub usage: Usage,
    /// The provider said it was done. If not, no call in the message can be run.
    pub complete: bool,
}

/// The events contradict each other: a decoder's mistake, not a provider's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the stream's events are out of order")]
pub struct OutOfOrder;

/// Holds what the decoder passed on, which is bounded there.
#[derive(Debug, Clone)]
pub struct Accumulator {
    scope: Scope,
    id: Option<String>,
    model: Option<String>,
    blocks: Vec<Open>,
    stop: Option<Stop>,
    usage: Usage,
    ended: bool,
}

#[derive(Debug, Clone)]
struct Open {
    start: Start,
    text: String,
    extra: Vec<Json>,
    native: Option<Json>,
    stopped: bool,
    broken: Option<String>,
}

impl Accumulator {
    pub fn new(scope: Scope) -> Self {
        Self {
            scope,
            id: None,
            model: None,
            blocks: Vec::new(),
            stop: None,
            usage: Usage::default(),
            ended: false,
        }
    }

    pub fn push(&mut self, event: &Event) -> Result<(), OutOfOrder> {
        if self.ended {
            return Err(OutOfOrder);
        }
        match event {
            Event::Started { id, model } => (self.id, self.model) = (id.clone(), model.clone()),
            Event::BlockStart { block, start } => {
                if *block != self.blocks.len() {
                    return Err(OutOfOrder);
                }
                self.blocks.push(Open {
                    start: start.clone(),
                    text: String::new(),
                    extra: Vec::new(),
                    native: None,
                    stopped: false,
                    broken: None,
                });
            }
            Event::Delta { block, delta } => {
                let b = self.open(*block)?;
                match (delta, &b.start) {
                    (Delta::Text(t), Start::Text)
                    | (Delta::Reasoning(t), Start::Reasoning)
                    | (Delta::Args(t), Start::ToolCall { .. } | Start::Unknown { .. }) => {
                        b.text.push_str(t);
                    }
                    (Delta::Other(json), _) => b.extra.push(json.clone()),
                    _ => return Err(OutOfOrder),
                }
            }
            Event::BlockStop { block, native, broken } => {
                let b = self.open(*block)?;
                (b.native, b.broken, b.stopped) = (native.clone(), broken.clone(), true);
                // It has them all.
                if matches!(b.start, Start::Unknown { .. }) && b.native.is_some() {
                    b.extra.clear();
                }
            }
            Event::Usage(usage) => self.usage = usage.clone(),
            Event::Stop(stop) => self.stop = Some(stop.clone()),
            Event::End => self.ended = true,
        }
        Ok(())
    }

    fn open(&mut self, block: usize) -> Result<&mut Open, OutOfOrder> {
        self.blocks.get_mut(block).filter(|b| !b.stopped).ok_or(OutOfOrder)
    }

    /// The answer as it stands. Until [`Event::End`] nothing in it can be run.
    pub fn finish(self) -> Reply {
        let runs = self.ended && self.stop.as_ref().is_some_and(Stop::runs_calls);
        let mut seen: Vec<String> = Vec::new();
        let mut blocks = Vec::with_capacity(self.blocks.len());
        for b in self.blocks {
            let cut = !b.stopped;
            let content = match b.start {
                Start::Text => Content::Text { text: b.text },
                Start::Reasoning => Content::Reasoning { text: b.text },
                Start::Unknown { name } => Content::Unknown { name },
                Start::ToolCall { id, name } => {
                    let args = if cut || !runs {
                        Args::Cut
                    } else if let Some(why) = b.broken {
                        Args::Broken(why)
                    } else if id.is_empty() || name.is_empty() {
                        Args::Broken("the call has no id or no name".into())
                    } else if seen.contains(&id) {
                        Args::Broken("another call has the same id".into())
                    } else {
                        parse(&b.text)
                    };
                    seen.push(id.clone());
                    Content::ToolCall(ToolCall { id, name, raw: b.text, args })
                }
            };
            let native = b.native.map(|json| Native { scope: self.scope.clone(), json });
            blocks.push(Block { content, native, extra: b.extra, cut });
        }
        Reply {
            id: self.id,
            model: self.model,
            message: Message { role: Role::Assistant, blocks },
            stop: self.stop,
            usage: self.usage,
            complete: self.ended,
        }
    }
}

fn parse(raw: &str) -> Args {
    match serde_json::from_str::<Map<String, Value>>(raw) {
        Ok(args) => Args::Ready(args),
        Err(e) => Args::Broken(format!("the arguments are not a JSON object: {e}")),
    }
}
