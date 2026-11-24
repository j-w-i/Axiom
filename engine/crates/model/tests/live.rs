//! Against the LM Studio on this machine, by hand:
//! `cargo test -p axiom-model --test live -- --ignored --nocapture --test-threads 1`.
//! Few and small requests; it starts, stops and changes nothing, but LM Studio loads a model that is
//! asked for.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use axiom_model::{
    Accumulator, Answer, Block, CallError, Class, Client, Content, Delta, Endpoint, Event, Json,
    Message, Options, Profile, Reply, Request, Role, Settings, Stop, Tool,
};
use tokio::time::timeout;

const SMALL: &str = "qwen2.5-0.5b-instruct-mlx";
const CALLS: &str = "qwen/qwen3-4b-2507";

fn client() -> Client {
    let profile = Profile::lm_studio();
    let endpoint = Endpoint::parse(profile.base_url.as_deref().unwrap()).unwrap();
    Client::new(profile, endpoint, None, Options::default()).unwrap()
}
fn ask(model: &str, text: &str, max: u32) -> Request {
    Request {
        model: model.into(),
        messages: vec![Message::user(text)],
        settings: Settings { max_output: Some(max), ..Settings::default() },
        ..Request::default()
    }
}
async fn all(mut answer: Answer) -> Reply {
    let mut a = Accumulator::new(answer.scope().clone());
    while let Some(event) = answer.next().await {
        a.push(&event.unwrap()).unwrap();
    }
    a.finish()
}

#[tokio::test]
#[ignore = "needs the LM Studio on the owner's Mac; run by hand"]
async fn a_conversation_with_a_tool() {
    let c = client();
    let models = c.list_models().await.unwrap();
    println!("models: {:?}", models.iter().map(|m| &m.id).collect::<Vec<_>>());
    assert!(models.iter().any(|m| m.id == CALLS));

    let mut r = ask(CALLS, "What is the weather in Paris?", 120);
    r.system = "Use the tools.".into();
    r.tools = vec![Tool {
        name: "get_weather".into(),
        description: "The weather now at a place.".into(),
        schema: Json::parse(
            r#"{"type":"object","properties":{"city":{"type":"string"}},"required":["city"]}"#,
        )
        .unwrap(),
    }];
    let first = all(c.stream(&r).await.unwrap()).await;
    println!("call: {first:?}");
    assert_eq!((first.complete, first.stop.clone()), (true, Some(Stop::ToolUse)));
    let calls: Vec<_> = first.message.calls().collect();
    assert_eq!((calls.len(), calls[0].name()), (1, "get_weather"));
    let result = Block::result(calls[0].id(), "18 degrees, rain", false);
    r.messages.push(first.message.clone());
    r.messages.push(Message { role: Role::User, blocks: vec![result] });
    let second = all(c.stream(&r).await.unwrap()).await;
    println!("after it: {second:?}");
    assert_eq!(second.stop, Some(Stop::EndTurn));
    let Content::Text { text } = &second.message.blocks[0].content else { panic!() };
    assert!(text.contains("18"));
}

#[tokio::test]
#[ignore = "needs the LM Studio on the owner's Mac; run by hand"]
async fn cancelling_and_limits() {
    let c = client();
    // Dropped after a few pieces; the next answer comes at once, so the server has let go.
    let mut answer = c.stream(&ask(SMALL, "Count from 1 to 500.", 2000)).await.unwrap();
    let mut pieces = 0;
    while pieces < 5 {
        if let Event::Delta { delta: Delta::Text(_), .. } = answer.next().await.unwrap().unwrap() {
            pieces += 1;
        }
    }
    drop(answer);
    let start = std::time::Instant::now();
    let next = timeout(Duration::from_secs(60), c.stream(&ask(SMALL, "Say hi.", 4))).await;
    let reply = all(next.unwrap().unwrap()).await;
    println!("after cancelling, {:?}: {reply:?}", start.elapsed());
    assert_eq!((reply.complete, reply.stop), (true, Some(Stop::MaxOutput)));

    let long = all_or_error(&c, &ask(SMALL, &"word ".repeat(40_000), 4)).await;
    println!("too long: {long:?}");
    assert_eq!(long.unwrap_err().class, Class::ContextTooLong);

    let empty = Request { messages: Vec::new(), ..ask(SMALL, "", 4) };
    let empty = all_or_error(&c, &empty).await.unwrap_err();
    println!("no messages: {empty:?}");
    assert_eq!((empty.class, empty.status), (Class::BadRequest, Some(400)));

    // It answers with whatever it has loaded, and says so only in the stream.
    let other = all_or_error(&c, &ask("no-such-model", "Hi", 4)).await.unwrap();
    println!("unknown model answered by {:?}", other.model);
    assert_ne!(other.model.as_deref(), Some("no-such-model"));
}

async fn all_or_error(c: &Client, r: &Request) -> Result<Reply, axiom_model::Error> {
    let mut answer = match c.stream(r).await {
        Ok(answer) => answer,
        Err(CallError::Provider(e)) => return Err(e),
        Err(e) => panic!("{e}"),
    };
    let mut a = Accumulator::new(answer.scope().clone());
    while let Some(event) = answer.next().await {
        a.push(&event?).unwrap();
    }
    Ok(a.finish())
}
