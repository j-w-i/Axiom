//! HTTP against a server on this machine that behaves badly (SI-3, SI-4; T25, T44). No test here needs a
//! machine to be fast: what is waited for is either certain to come or never does.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::{sync::Arc, time::Duration};

use axiom_model::{
    Accumulator, Answer, Auth, CallError, Class, Client, Content, Credential, EncodeError,
    Endpoint, Error, Event, Message, Options, Profile, Reply, Request, Retry, Secret, Settings,
    SetupError, Stop,
};
use common::*;
use tokio::{sync::Notify, time::timeout};

const KEY: &str = "sk-Canary-0123456789-abcdef";

fn endpoint(url: &str) -> Endpoint {
    Endpoint::parse(url).unwrap()
}
fn key(url: &str, auth: &Auth) -> Credential {
    Credential::new(endpoint(url).origin(), auth, Secret::new(KEY.into())).unwrap()
}
fn client_with(profile: Profile, url: &str, options: Options) -> Client {
    let credential = key(url, &profile.auth);
    Client::new(profile, endpoint(url), Some(credential), options).unwrap()
}
fn client(url: &str) -> Client {
    client_with(Profile::openai_compatible("p", url), url, Options::default())
}
fn anthropic(url: &str) -> Client {
    client_with(Profile::anthropic(), url, Options::default())
}
fn request() -> Request {
    Request {
        model: "m".into(),
        messages: vec![Message::user("hi")],
        settings: Settings { max_output: Some(9), ..Settings::default() },
        ..Request::default()
    }
}
/// The provider's error, which never has the credential in it, however it is printed.
fn provider(e: CallError) -> Error {
    let CallError::Provider(e) = e else { panic!("{e:?}") };
    clean(&e)
}
fn clean(e: &Error) -> Error {
    let all = format!("{e} {e:?} {e:#?} {:?}", std::error::Error::source(e).map(|s| s.to_string()));
    assert!(!all.to_lowercase().contains("canary") && !all.contains("0123456789"), "{all}");
    e.clone()
}
async fn fails(client: &Client) -> Error {
    provider(timeout(LONG, client.stream(&request())).await.unwrap().unwrap_err())
}
async fn begins(client: &Client) -> Answer {
    timeout(LONG, client.stream(&request())).await.unwrap().unwrap()
}
/// All of the answer, and the error it ended in.
async fn all(mut answer: Answer) -> (Reply, Option<Error>) {
    let mut a = Accumulator::new(answer.scope().clone());
    let mut error = None;
    while let Some(next) = timeout(LONG, answer.next()).await.unwrap() {
        assert!(error.is_none(), "something came after the error");
        match next {
            Ok(event) => a.push(&event).unwrap(),
            Err(e) => error = Some(clean(&e)),
        }
    }
    assert!(timeout(LONG, answer.next()).await.unwrap().is_none());
    (a.finish(), error)
}
fn text(reply: &Reply) -> String {
    let texts = reply.message.blocks.iter().filter_map(|b| match &b.content {
        Content::Text { text } => Some(text.as_str()),
        _ => None,
    });
    texts.collect()
}

const HI: &str =
    "data: {\"id\":\"c\",\"model\":\"m\",\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n";
const CALL: &str = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"a\",\"function\":{\"name\":\"f\",\"arguments\":\"{}\"}}]}}]}\n\n";
const STOP: &str = "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n";
const DONE: &str = "data: [DONE]\n\n";

// What is sent.

#[tokio::test]
async fn a_request_and_its_answer() {
    let s = Server::new(vec![send(STREAM), send([HI, STOP, DONE].concat())]).await;
    let answer = begins(&client(&s.url())).await;
    assert_eq!((answer.request_id(), answer.lost().len()), (Some("req_head"), 0));
    assert_eq!(answer.scope().model, "m");
    let (r, e) = all(answer).await;
    assert_eq!((e, text(&r).as_str(), r.stop, r.complete), (None, "hi", Some(Stop::EndTurn), true));

    let [asked] = s.asked().try_into().unwrap();
    assert!(asked.head.starts_with("POST /v1/chat/completions HTTP/1.1\r\n"));
    assert_eq!(asked.header("authorization"), Some(format!("Bearer {KEY}").as_str()));
    assert_eq!(asked.header("accept"), Some("text/event-stream"));
    assert_eq!(asked.header("content-type"), Some("application/json"));
    assert_eq!(asked.header("user-agent"), Some("axiom/0.0.1"));
    for none in ["accept-encoding", "referer", "cookie", "x-api-key"] {
        assert_eq!(asked.header(none), None);
    }
    let sent =
        axiom_model::codec::encode(&request(), &Profile::openai_compatible("p", "x")).unwrap();
    assert_eq!(asked.body, sent.body);
    assert_eq!(String::from_utf8_lossy(&asked.body).matches("Canary").count(), 0);
}

#[tokio::test]
async fn anthropic_is_asked_its_own_way() {
    let body = fixture("messages-from-documentation/tool.sse");
    let s = Server::new(vec![send(STREAM), Step::Send(body)]).await;
    let (r, e) = all(begins(&anthropic(&s.url())).await).await;
    assert_eq!((e, r.message.calls().count()), (None, 1));
    let [asked] = s.asked().try_into().unwrap();
    assert!(asked.head.starts_with("POST /v1/messages HTTP/1.1\r\n"));
    assert_eq!(asked.header("x-api-key"), Some(KEY));
    assert_eq!(asked.header("anthropic-version"), Some("2026-10-21"));
    assert_eq!(asked.header("authorization"), None);
}

#[tokio::test]
async fn openai_is_asked_its_own_way() {
    let body = fixture("responses-from-documentation/tool.sse");
    let s = Server::new(vec![send(STREAM), Step::Send(body)]).await;
    let c = client_with(Profile::openai(), &s.url(), Options::default());
    let (r, e) = all(begins(&c).await).await;
    assert_eq!((e, r.message.calls().count(), r.complete), (None, 1, true));
    let [asked] = s.asked().try_into().unwrap();
    assert!(asked.head.starts_with("POST /v1/responses HTTP/1.1\r\n"));
    assert_eq!(asked.header("authorization"), Some(format!("Bearer {KEY}").as_str()));
    assert!(asked.body.starts_with(br#"{"model":"m","store":false,"stream":true,"#));
}

#[tokio::test]
async fn gemini_is_asked_its_own_way() {
    let body = fixture("gemini-from-documentation/parallel.sse");
    let s = Server::new(vec![send(STREAM), Step::Send(body)]).await;
    let c = client_with(Profile::gemini(), &s.url(), Options::default());
    let mut r = request();
    r.model = "gemini-4_x.0-Hydrogen".into();
    let answer = timeout(LONG, c.stream(&r)).await.unwrap().unwrap();
    // It ends with the connection, and that is an end since it had said why it stopped.
    let (reply, e) = all(answer).await;
    assert_eq!((e, reply.message.calls().count(), reply.complete), (None, 2, true));
    let [asked] = s.asked().try_into().unwrap();
    assert!(asked.head.starts_with(
        "POST /v1/models/gemini-4_x.0-Hydrogen:streamGenerateContent?alt=sse HTTP/1.1\r\n"
    ));
    // The key is in its header and nowhere else.
    assert_eq!(asked.header("x-goog-api-key"), Some(KEY));
    assert_eq!(asked.header("authorization"), None);
    assert_eq!(asked.head.matches(KEY).count(), 1);
}

#[tokio::test]
async fn gemini_that_hangs_up_before_it_stopped_is_cut_short() {
    let half =
        r#"data: {"candidates":[{"content":{"parts":[{"functionCall":{"name":"f","args":{}}}]}}]}"#;
    let s = Server::new(vec![send(STREAM), send(format!("{half}\n\n")), Step::Drop]).await;
    let c = client_with(Profile::gemini(), &s.url(), Options::default());
    let (r, e) = all(begins(&c).await).await;
    assert_eq!((e.unwrap().class, r.complete), (Class::CutShort, false));
    assert_eq!(r.message.calls().count(), 0);
    // After it did, how the connection ends does not matter.
    let head =
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n";
    let stop = format!("{half}\n\ndata: {{\"candidates\":[{{\"finishReason\":\"STOP\"}}]}}\n\n");
    let s =
        Server::new(vec![send(head), send(format!("{:x}\r\n{stop}\r\n", stop.len())), Step::Drop])
            .await;
    let c = client_with(Profile::gemini(), &s.url(), Options::default());
    let (r, e) = all(begins(&c).await).await;
    assert_eq!((e, r.complete, r.message.calls().count()), (None, true, 1));
}

#[tokio::test]
async fn a_models_name_cannot_change_where_a_request_goes() {
    let s = Server::new(vec![]).await;
    let c = client_with(Profile::gemini(), &s.url(), Options::default());
    for bad in [
        "",
        ".",
        "..",
        "...",
        "a/b",
        "../x",
        "a:b",
        "a?b",
        "a#b",
        "a%2fb",
        "a b",
        "a\n",
        "//evil",
        "é",
        "a\\b",
        "a&b",
        "models/m",
        "m:generateContent?x=",
        "a@b",
        "a\0",
    ] {
        let mut r = request();
        r.model = bad.into();
        let e = c.stream(&r).await.unwrap_err();
        assert_eq!(e, CallError::Encode(EncodeError::BadModel(bad.into())), "{bad:?}");
    }
    assert!(s.asked().is_empty());
}

#[tokio::test]
async fn openrouter_is_asked_its_own_way() {
    let body = fixture("openrouter-from-documentation/reasoning.sse");
    let s = Server::new(vec![send(STREAM), Step::Send(body)]).await;
    let c = client_with(Profile::openrouter(), &s.url(), Options::default());
    let (r, e) = all(begins(&c).await).await;
    assert_eq!((e, r.complete), (None, true));
    let [asked] = s.asked().try_into().unwrap();
    assert!(asked.head.starts_with("POST /v1/chat/completions HTTP/1.1\r\n"));
    assert_eq!(asked.header("authorization"), Some(format!("Bearer {KEY}").as_str()));
    for none in ["http-referer", "x-title", "x-openrouter-title"] {
        assert_eq!(asked.header(none), None);
    }
}

#[tokio::test]
async fn googles_errors() {
    let page = fixture("gemini-from-documentation/error.json");
    let answer = |status: &str, body: Vec<u8>| {
        vec![send(head(status, "content-type: application/json\r\n")), Step::Send(body)]
    };
    // A wrong key is a bad request there.
    let s = Server::new(answer("400 Bad Request", page)).await;
    let c = client_with(Profile::gemini(), &s.url(), Options::default());
    let e = fails(&c).await;
    assert_eq!((e.class, e.retry, e.status), (Class::Auth, Retry::Never, Some(400)));
    assert_eq!(e.code.as_deref(), Some("API_KEY_INVALID"));
    for (status, name, class, retry) in [
        (400, "INVALID_ARGUMENT", Class::BadRequest, Retry::Never),
        (402, "RESOURCE_EXHAUSTED", Class::Billing, Retry::Never),
        (403, "PERMISSION_DENIED", Class::Auth, Retry::Never),
        (429, "RESOURCE_EXHAUSTED", Class::RateLimit, Retry::Safe),
        (500, "INTERNAL", Class::Server, Retry::Restart),
        (503, "UNAVAILABLE", Class::Overloaded, Retry::Safe),
        (504, "DEADLINE_EXCEEDED", Class::Server, Retry::Restart),
    ] {
        let body = format!(r#"{{"error":{{"code":{status},"message":"m","status":"{name}"}}}}"#);
        let s = Server::new(answer(&format!("{status} x"), body.into_bytes())).await;
        let c = client_with(Profile::gemini(), &s.url(), Options::default());
        let e = fails(&c).await;
        assert_eq!((e.class, e.retry, e.code.as_deref()), (class, retry, Some(name)));
    }
}

#[tokio::test]
async fn without_a_credential_none_is_sent() {
    let s = Server::new(vec![send(STREAM), send([STOP, DONE].concat())]).await;
    let url = format!("http://localhost:{}/v1", s.addr.port());
    let c = Client::new(Profile::lm_studio(), endpoint(&url), None, Options::default()).unwrap();
    let (r, e) = all(begins(&c).await).await;
    assert!(r.complete && e.is_none());
    assert_eq!(s.asked()[0].header("authorization"), None);
}

#[tokio::test]
async fn a_request_that_cannot_be_written_is_not_sent() {
    let s = Server::new(vec![]).await;
    let mut r = request();
    r.settings.effort = Some("high".into());
    let e = client(&s.url()).stream(&r).await.unwrap_err();
    assert_eq!(e, CallError::Encode(EncodeError::Unsupported("effort")));
    assert!(s.asked().is_empty());
}

// Where the credential goes.

#[tokio::test]
async fn redirects_are_not_followed_and_leak_nothing() {
    let canary = Server::new(vec![send(STREAM), send([HI, STOP, DONE].concat())]).await;
    for status in
        ["301 Moved", "302 Found", "303 See Other", "307 Temporary", "308 Permanent", "300 Choices"]
    {
        let elsewhere = format!("http://localhost:{}/v1/{KEY}", canary.addr.port());
        for to in [elsewhere.as_str(), "/v1/other", "//127.0.0.1/"] {
            let s = Server::new(vec![send(head(status, &format!("location: {to}\r\n")))]).await;
            let e = fails(&client(&s.url())).await;
            assert_eq!((e.class, e.retry), (Class::Protocol, Retry::Never));
            assert_eq!(e.status, status[..3].parse().ok());
            assert!(e.message.contains("redirect, which is never followed"));
            assert_eq!(s.asked().len(), 1);
        }
    }
    assert!(canary.asked().is_empty());
}

#[tokio::test]
async fn a_credential_of_another_origin_makes_no_client() {
    let auth = Auth::Bearer;
    for (made_for, used_at) in [
        ("https://api.example.com/v1", "https://api.example.org/v1"),
        ("https://api.example.com/v1", "https://api.example.com:8443/v1"),
        ("https://localhost/v1", "http://localhost/v1"),
        ("http://localhost:1/v1", "http://localhost:2/v1"),
        ("http://localhost:1/v1", "http://127.0.0.1:1/v1"),
    ] {
        let p = Profile::openai_compatible("p", used_at);
        let e = Client::new(p, endpoint(used_at), Some(key(made_for, &auth)), Options::default());
        let e = e.unwrap_err();
        assert!(matches!(e, SetupError::OtherOrigin { .. }));
        assert!(!format!("{e} {e:?}").contains("Canary"));
    }
    let mut p = Profile::openai_compatible("p", "https://example.com");
    p.headers.insert("authorization".into(), "x".into());
    let e = Client::new(p, endpoint("https://example.com"), None, Options::default()).unwrap_err();
    assert_eq!(e, SetupError::BadHeader("authorization".into()));
}

#[tokio::test]
async fn nothing_prints_the_credential() {
    let s = Server::new(vec![
        send(format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nx-echo: {KEY}\r\nrequest-id: {KEY}\r\nconnection: close\r\n\r\n")),
        send([HI, STOP, DONE].concat()),
    ])
    .await;
    let c = client(&s.url());
    let answer = begins(&c).await;
    assert_eq!(answer.request_id(), Some("[secret]"));
    for shown in
        [format!("{c:?}"), format!("{c:#?}"), format!("{answer:?}"), format!("{answer:#?}")]
    {
        assert!(!shown.contains("Canary"), "{shown}");
    }
}

/// Every way an answer can fail, from a server that repeats what it was sent wherever it can.
#[tokio::test]
async fn a_server_that_repeats_the_credential_does_not_get_it_into_an_error() {
    let json = |status: &str, body: String| {
        vec![
            send(head(
                status,
                &format!(
                    "content-type: application/json\r\nrequest-id: {KEY}\r\nretry-after: {KEY}\r\n"
                ),
            )),
            send(body),
        ]
    };
    let error = format!(
        r#"{{"type":"{KEY}","error":{{"type":"{KEY}","code":"{KEY}","message":"bad key {KEY}!","details":{{"error_code":"{KEY}"}}}},"request_id":"{KEY}"}}"#
    );
    let escaped = error.replace("sk-Canary", r"sk\u002dC\u0061nary");
    let in_stream = |data: &str| vec![send(STREAM), send(HI), send(format!("data: {data}\n\n"))];
    let mut ways = vec![
        json("200 OK", error.clone()),
        json("200 OK", format!("[\"{KEY}\"]")),
        json("200 OK", KEY.into()),
        vec![send(head("200 OK", &format!("content-type: {KEY}\r\n")))],
        vec![send(head(
            "200 OK",
            &format!("content-type: text/event-stream\r\ncontent-encoding: {KEY}\r\n"),
        ))],
        vec![send(head(&format!("500 {KEY}"), ""))],
        vec![send(format!("HTTP/1.1 {KEY}\r\n\r\n"))],
        vec![send(format!("{KEY}\r\n\r\n"))],
        vec![send(format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n{KEY}\r\n"
        ))],
        vec![send(format!("HTTP/1.1 200 OK\r\ncontent-length: {KEY}\r\n\r\n"))],
        in_stream(&error),
        in_stream(&escaped),
        in_stream(KEY),
        in_stream(&format!(r#"{{"choices":"{KEY}"}}"#)),
        in_stream(&format!(r#"{{"error":"{KEY}"}}"#)),
        vec![send(STREAM), send(format!("event: {KEY}\nevent: error\ndata: {KEY}\n\n"))],
        vec![send(STREAM), send(format!("data: {KEY}\n\n")), Step::Drop],
        vec![send(STREAM), send(format!("data: {KEY}\u{0}")), send(b"\xff\n")],
        vec![Step::Drop],
        vec![send("HTTP/1.1 200 OK\r\n"), Step::Drop],
    ];
    for status in [
        "300 x", "400 x", "401 x", "402 x", "403 x", "404 x", "413 x", "418 x", "429 x", "500 x",
        "503 x", "529 x",
    ] {
        ways.push(json(status, error.clone()));
        ways.push(json(status, escaped.clone()));
        ways.push(json(status, format!("<html>{KEY}</html>")));
        ways.push(json(status, format!("{}{KEY}", "x".repeat(2040))));
        ways.push(json(status, format!("{}{KEY}", "x".repeat(64 * 1024 - 10))));
    }
    for (n, way) in ways.into_iter().enumerate() {
        for codec in [
            Profile::openai_compatible("p", "x"),
            Profile::anthropic(),
            Profile::openai(),
            Profile::gemini(),
        ] {
            let s = Server::new(way.clone()).await;
            let c = client_with(codec, &s.url(), Options::default());
            // `fails`, `all` and `clean` look for the credential.
            let failed = match timeout(LONG, c.stream(&request())).await.unwrap() {
                Err(e) => Some(provider(e)),
                Ok(answer) => all(answer).await.1,
            };
            assert!(failed.is_some(), "{n}");
            if c.profile().codec != axiom_model::Codec::Messages {
                provider(timeout(LONG, c.list_models()).await.unwrap().unwrap_err());
            }
        }
    }
}

#[tokio::test]
async fn the_credential_is_not_shown_when_nobody_answers() {
    let closed = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap()
    };
    let e = fails(&client(&format!("http://{closed}/v1"))).await;
    assert_eq!((e.class, e.retry, e.status), (Class::Network, Retry::Safe, None));
    assert!(e.message.starts_with("no connection to the provider: "), "{}", e.message);
    // Not TLS, though it was expected.
    let s = Server::new(vec![send(format!("HTTP/1.1 400 {KEY}\r\n\r\n"))]).await;
    let e = fails(&client(&format!("https://{}/v1", s.addr))).await;
    assert_eq!((e.class, e.retry), (Class::Network, Retry::Safe));
    assert!(!s.asked()[0].head.contains("Canary"));
    // A name that cannot be (RFC 6761), whether or not there is a network.
    let e = fails(&client("https://nobody.invalid/v1")).await;
    assert_eq!((e.class, e.retry), (Class::Network, Retry::Safe));
}

// Proxies.

#[tokio::test]
async fn a_named_proxy_is_told_where_to_and_nothing_else() {
    let proxy = Server::new(vec![send(head("502 Bad Gateway", ""))]).await;
    let options = Options { proxy: Some(format!("http://{}", proxy.addr)), ..Options::default() };
    let c = client_with(Profile::anthropic(), "https://api.example.invalid/v1", options);
    let e = fails(&c).await;
    assert_eq!((e.class, e.retry), (Class::Network, Retry::Safe));
    let [asked] = proxy.asked().try_into().unwrap();
    assert!(asked.head.starts_with("CONNECT api.example.invalid:443 "), "{}", asked.head);
    assert!(!asked.head.contains("Canary") && asked.body.is_empty());
}

#[test]
fn some_proxies_are_refused() {
    let with = |url: &str, proxy: &str| {
        let options = Options { proxy: Some(proxy.into()), ..Options::default() };
        Client::new(Profile::openai_compatible("p", url), endpoint(url), None, options).err()
    };
    let bad = Some(SetupError::BadProxy);
    assert_eq!(with("http://localhost:1/v1", "http://127.0.0.1:2"), bad);
    for proxy in [
        "",
        "127.0.0.1:2",
        "http://u:p@127.0.0.1:2",
        "http://u@127.0.0.1",
        "ftp://127.0.0.1",
        "socks5://127.0.0.1",
    ] {
        assert_eq!(with("https://example.com/v1", proxy), bad, "{proxy}");
    }
    assert_eq!(with("https://example.com/v1", "http://127.0.0.1:2"), None);
}

/// The environment can only be set for another process: this same test program, told to run the one
/// test below.
#[tokio::test]
async fn the_environments_proxies_are_ignored() {
    let proxy = Server::new(vec![send(head("502 Bad Gateway", ""))]).await;
    let s = Server::new(vec![send(STREAM), send([HI, STOP, DONE].concat())]).await;
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child
        .args(["asks_the_server_it_is_told", "--exact", "--ignored"])
        .env("AXIOM_TEST_URL", s.url());
    for name in ["http_proxy", "HTTP_PROXY", "https_proxy", "HTTPS_PROXY", "all_proxy", "ALL_PROXY"]
    {
        child.env(name, format!("http://{}", proxy.addr));
    }
    for name in ["no_proxy", "NO_PROXY", "REQUEST_METHOD"] {
        child.env_remove(name);
    }
    let out = tokio::task::spawn_blocking(move || child.output());
    let out = timeout(LONG, out).await.unwrap().unwrap().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
    assert_eq!(s.asked().len(), 1);
    assert!(proxy.asked().is_empty());
}

#[tokio::test]
#[ignore = "run by the_environments_proxies_are_ignored, in another process"]
async fn asks_the_server_it_is_told() {
    let Ok(url) = std::env::var("AXIOM_TEST_URL") else { return };
    let (r, e) = all(begins(&client(&url)).await).await;
    assert!(r.complete && e.is_none());
}

// What the provider says went wrong.

#[tokio::test]
async fn statuses() {
    let anthropic_error = |kind: &str, message: &str| {
        format!(
            r#"{{"type":"error","error":{{"type":"{kind}","message":"{message}"}},"request_id":"req_body"}}"#
        )
    };
    for (status, kind, class, retry) in [
        ("401 Unauthorized", "authentication_error", Class::Auth, Retry::Never),
        ("402 Payment Required", "billing_error", Class::Billing, Retry::Never),
        ("429 Too Many Requests", "rate_limit_error", Class::RateLimit, Retry::Safe),
        ("529 ", "overloaded_error", Class::Overloaded, Retry::Safe),
        ("500 Internal", "api_error", Class::Server, Retry::Restart),
        ("400 Bad Request", "invalid_request_error", Class::BadRequest, Retry::Never),
    ] {
        let s = Server::new(vec![
            send(head(
                status,
                "content-type: application/json\r\nrequest-id: req_head\r\nretry-after: 7\r\n",
            )),
            send(anthropic_error(kind, "words")),
        ])
        .await;
        let e = fails(&anthropic(&s.url())).await;
        assert_eq!((e.class, e.retry), (class, retry), "{status}");
        assert_eq!(e.status, status[..3].parse().ok());
        assert_eq!((e.message.as_str(), e.code.as_deref()), ("words", Some(kind)));
        assert_eq!(e.request_id.as_deref(), Some("req_head"));
        assert_eq!(e.retry_after, Some(Duration::from_secs(7)));
        // Nothing is retried here.
        assert_eq!(s.asked().len(), 1);
    }
    // The id from the body or under its other name; a date, long gone; a head with neither.
    let one = |headers: &'static str, body: String| async move {
        let s = Server::new(vec![send(head("429 x", headers)), send(body)]).await;
        fails(&client(&s.url())).await
    };
    let e = one("", anthropic_error("rate_limit_error", "m")).await;
    assert_eq!((e.request_id.as_deref(), e.retry_after), (Some("req_body"), None));
    let e =
        one("x-request-id: other\r\nretry-after: Sun, 06 Nov 1994 08:49:37 GMT\r\n", String::new())
            .await;
    assert_eq!((e.request_id.as_deref(), e.retry_after), (Some("other"), Some(Duration::ZERO)));
    let e = one("retry-after: soon\r\n", String::new()).await;
    assert_eq!((e.request_id, e.retry_after), (None, None));
    let spent = r#"{"error":{"type":"rate_limit_error","message":"m","details":{"error_code":"enforced_spend_limit_reached"}}}"#;
    assert_eq!(one("", spent.into()).await.class, Class::Billing);
    // Centuries away.
    let e = one("retry-after: Fri, 31 Dec 9999 23:59:59 GMT\r\n", String::new()).await;
    assert!(e.retry_after.unwrap() > Duration::from_secs(365 * 24 * 3600));
}

#[tokio::test]
async fn success_that_is_none() {
    let one = |headers: &'static str, body: &'static str| async move {
        let s = Server::new(vec![send(head("200 OK", headers)), send(body)]).await;
        fails(&client(&s.url())).await
    };
    let json = "content-type: application/json\r\nrequest-id: req_head\r\nretry-after: 3\r\n";
    let e = one(json, r#"{"error":{"message":"maximum context length is 8","code":"x"}}"#).await;
    assert_eq!((e.class, e.status), (Class::ContextTooLong, Some(200)));
    assert_eq!(
        (e.request_id.as_deref(), e.retry_after),
        (Some("req_head"), Some(Duration::from_secs(3)))
    );
    assert_eq!(
        one(json, r#"{"error":"Unexpected endpoint or method."}"#).await.class,
        Class::Server
    );
    assert_eq!(one(json, r#"{"choices":[]}"#).await.class, Class::Protocol);
    let e = one("content-type: text/html\r\n", "<html>").await;
    assert_eq!(
        (e.class, e.message.as_str()),
        (Class::Protocol, "the answer is not a stream but a web page")
    );
    let e = one("content-type: text/event-stream\r\ncontent-encoding: gzip\r\n", "\x1f").await;
    assert_eq!((e.class, e.retry), (Class::Protocol, Retry::Never));
    // Said in capitals, with a character set, or not said at all: a stream.
    for fine in
        ["content-type: Text/Event-Stream; charset=utf-8\r\n", "", "content-encoding: identity\r\n"]
    {
        let s =
            Server::new(vec![send(head("200 OK", fine)), send([HI, STOP, DONE].concat())]).await;
        let (r, e) = all(begins(&client(&s.url())).await).await;
        assert!(r.complete && e.is_none(), "{fine}");
    }
}

// Answers that break off, stall, or do not end.

#[tokio::test]
async fn an_answer_that_breaks_off_is_kept_and_cannot_run() {
    let chunked = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\nrequest-id: req_head\r\n\r\n";
    let chunk = |text: &str| format!("{:x}\r\n{text}\r\n", text.len());
    let long = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: 99999\r\nrequest-id: req_head\r\n\r\n";
    let seen = Arc::new(Notify::new());
    for (head, first) in [(chunked, chunk(&[HI, CALL].concat())), (long, [HI, CALL].concat())] {
        let s =
            Server::new(vec![send(head), send(first), Step::Wait(seen.clone()), Step::Drop]).await;
        let mut answer = begins(&client(&s.url())).await;
        // The connection goes away only after the call has been read.
        let mut a = Accumulator::new(answer.scope().clone());
        for _ in 0..6 {
            let event = timeout(LONG, answer.next()).await.unwrap().unwrap().unwrap();
            a.push(&event).unwrap();
        }
        seen.notify_one();
        let e = clean(&timeout(LONG, answer.next()).await.unwrap().unwrap().unwrap_err());
        assert_eq!((e.class, e.retry), (Class::CutShort, Retry::Restart));
        assert_eq!(e.request_id.as_deref(), Some("req_head"));
        assert!(answer.next().await.is_none());
        let r = a.finish();
        assert_eq!((text(&r).as_str(), r.complete, r.message.calls().count()), ("hi", false, 0));
        assert_eq!(r.message.blocks.len(), 2);
    }
    // Closed in order, but without an end.
    let s = Server::new(vec![send(STREAM), send([HI, CALL].concat())]).await;
    let (r, e) = all(begins(&client(&s.url())).await).await;
    let e = e.unwrap();
    assert_eq!((e.class, e.retry, r.message.calls().count()), (Class::CutShort, Retry::Restart, 0));
    // A server that says why it stopped, and then nothing: an end.
    let s = Server::new(vec![send(STREAM), send([HI, CALL, STOP].concat())]).await;
    let (r, e) = all(begins(&client(&s.url())).await).await;
    assert_eq!((r.complete, r.message.calls().count(), e), (true, 1, None));
    // The same, rudely.
    let s = Server::new(vec![send(chunked), send(chunk(&[HI, CALL, STOP].concat()))]).await;
    let (r, e) = all(begins(&client(&s.url())).await).await;
    assert_eq!((r.complete, r.message.calls().count(), e), (true, 1, None));
}

#[tokio::test]
async fn no_answer_at_all() {
    let s = Server::new(vec![Step::Drop]).await;
    let e = fails(&client(&s.url())).await;
    // It had the request, and may have worked on it.
    assert_eq!((e.class, e.retry), (Class::Network, Retry::Restart));
    assert_eq!(s.asked().len(), 1);
}

#[tokio::test]
async fn a_server_that_stalls_is_left() {
    let never = || Step::Wait(Arc::new(Notify::new()));
    // A server that may be slow, but answers, has all the time it needs: only what is being waited
    // for in vain is short.
    let no_head = || Options { connect: LONG, first_byte: SHORT, idle: LONG, proxy: None };
    let no_more = || Options { connect: LONG, first_byte: LONG, idle: SHORT, proxy: None };
    let stalled = |options, steps| async move {
        let s = Server::new(steps).await;
        let c = client_with(Profile::openai_compatible("p", "x"), &s.url(), options);
        match timeout(LONG, c.stream(&request())).await.unwrap() {
            Err(e) => provider(e),
            Ok(answer) => all(answer).await.1.unwrap(),
        }
    };
    let e = stalled(no_head(), vec![never()]).await;
    assert_eq!((e.class, e.retry), (Class::Network, Retry::Restart));
    assert_eq!(e.message, "the provider did not answer for 0.25 s");
    let e = stalled(no_head(), vec![send("HTTP/1.1 200 OK\r\n"), never()]).await;
    assert_eq!(e.class, Class::Network);
    let e = stalled(no_more(), vec![send(STREAM), never()]).await;
    assert_eq!((e.class, e.retry), (Class::CutShort, Retry::Restart));
    assert_eq!(e.message, "nothing more came for 0.25 s");
    let e = stalled(no_more(), vec![send(STREAM), send(HI), never()]).await;
    assert_eq!((e.class, e.retry), (Class::CutShort, Retry::Restart));
    // The status is enough when the words do not come.
    let slow = send(head("429 x", "retry-after: 5\r\n"));
    let e = stalled(no_more(), vec![slow, send("{"), never()]).await;
    assert_eq!((e.class, e.retry_after), (Class::RateLimit, Some(Duration::from_secs(5))));
    let slow = send(head("200 OK", "content-type: application/json\r\n"));
    let e = stalled(no_more(), vec![slow, never()]).await;
    assert_eq!((e.class, e.retry), (Class::CutShort, Retry::Restart));
    // Said why it stopped, and then nothing: an end. All of it is sent at once.
    let s = Server::new(vec![send([STREAM, HI, STOP].concat()), never()]).await;
    let c = client_with(Profile::openai_compatible("p", "x"), &s.url(), no_more());
    let (r, e) = all(begins(&c).await).await;
    assert!(r.complete && e.is_none());
    // Said it was done, and holds on to the connection: nobody waits for it.
    let s = Server::new(vec![send(STREAM), send([HI, STOP, DONE].concat()), never()]).await;
    let (r, e) = all(begins(&client(&s.url())).await).await;
    assert!(r.complete && e.is_none());
}

#[tokio::test]
async fn a_connection_that_is_taken_and_never_spoken_on_is_left() {
    // The system takes the connection; nobody answers the TLS hello.
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("https://{}/v1", l.local_addr().unwrap());
    let options = Options { connect: SHORT, ..Options::default() };
    let e = fails(&client_with(Profile::anthropic(), &url, options)).await;
    assert_eq!((e.class, e.retry), (Class::Network, Retry::Safe));
    assert_eq!(e.message, "no connection to the provider for 0.25 s");
}

#[tokio::test]
async fn what_does_not_end_is_not_kept() {
    let forever = |start: &str, again: &str| {
        vec![send(start), Step::Forever(again.repeat(64 * 1024 / again.len()).into_bytes())]
    };
    // A line; an event of many lines.
    for (start, again) in
        [("data: ", "x"), ("data: x\n", "data: x\n"), (": ", "x"), ("event: ", "x")]
    {
        let s = Server::new(forever(&format!("{STREAM}{start}"), again)).await;
        let (r, e) = all(begins(&client(&s.url())).await).await;
        let e = e.unwrap();
        assert_eq!((e.class, e.retry, r.message.blocks.len()), (Class::Protocol, Retry::Never, 0));
        assert!(e.message.contains("larger than 8388608 bytes"), "{}", e.message);
    }
    // An error's words, an answer that is no stream, a list.
    let s = Server::new(forever(&head("500 x", ""), "x")).await;
    let e = fails(&client(&s.url())).await;
    assert_eq!(e.class, Class::Server);
    assert!(e.message.starts_with("xxx") && e.message.len() < 2100);
    let s = Server::new(forever(&head("200 OK", "content-type: application/json\r\n"), "x")).await;
    assert_eq!(fails(&client(&s.url())).await.class, Class::Protocol);
    let s = Server::new(forever(&head("200 OK", ""), " ")).await;
    let e = provider(timeout(LONG, client(&s.url()).list_models()).await.unwrap().unwrap_err());
    assert_eq!(
        (e.class, e.message.as_str()),
        (Class::Protocol, "the list of models is larger than 4194304 bytes")
    );
    // A head.
    let s = Server::new(forever("HTTP/1.1 200 OK\r\n", "x: y\r\n")).await;
    assert_eq!(fails(&client(&s.url())).await.class, Class::Network);
}

#[tokio::test]
async fn an_odd_stream_is_still_a_stream() {
    let body = [HI, STOP, DONE].concat();
    let mut pieces = String::new();
    for (n, b) in body.bytes().enumerate() {
        let size = if n % 2 == 0 { "1;ext=\"v\"" } else { "0001" };
        pieces.push_str(&format!("{size}\r\n{}\r\n", b as char));
    }
    pieces.push_str("0\r\nx-trailer: y\r\n\r\n");
    let head =
        "HTTP/1.1 200 fine\r\nCONTENT-TYPE:text/event-stream\r\nTransfer-Encoding: Chunked\r\n\r\n";
    let s = Server::new(vec![send(format!("{head}{pieces}"))]).await;
    let (r, e) = all(begins(&client(&s.url())).await).await;
    assert_eq!((text(&r).as_str(), r.complete, e), ("hi", true, None));
    // Chunks that are not.
    for bad in ["zz\r\nx\r\n", "-1\r\n", "ffffffffffffffffff\r\n"] {
        let s = Server::new(vec![send(format!("{head}{bad}"))]).await;
        let (r, e) = all(begins(&client(&s.url())).await).await;
        assert_eq!((r.complete, e.unwrap().class), (false, Class::CutShort), "{bad}");
    }
    // What was read before the mistake is still told, and before the error.
    let s = Server::new(vec![send(STREAM), send(format!("{HI}data: \u{1}\n\n{HI}"))]).await;
    let (r, e) = all(begins(&client(&s.url())).await).await;
    assert_eq!((text(&r).as_str(), e.unwrap().class), ("hi", Class::Protocol));
    let s =
        Server::new(vec![send(STREAM), send([HI.as_bytes(), b"data: \xff\n\n"].concat())]).await;
    let (r, e) = all(begins(&client(&s.url())).await).await;
    assert_eq!((text(&r).as_str(), e.unwrap().message.as_str()), ("hi", "the stream is not UTF-8"));
    // And nothing after the end matters, whatever it is.
    let s = Server::new(vec![
        send(STREAM),
        send([HI.as_bytes(), STOP.as_bytes(), DONE.as_bytes(), b"\xff\n\n"].concat()),
    ])
    .await;
    let (r, e) = all(begins(&client(&s.url())).await).await;
    assert_eq!((r.complete, e), (true, None));
}

#[tokio::test]
async fn an_error_in_the_stream_has_the_heads_id_and_is_cleaned() {
    let error = r#"data: {"error":{"message":"a\u001b[31mb","code":"overloaded_error"}}"#;
    let s = Server::new(vec![send(STREAM), send(format!("{HI}{error}\n\n"))]).await;
    let (r, e) = all(begins(&client(&s.url())).await).await;
    let e = e.unwrap();
    assert_eq!((e.class, e.retry), (Class::Overloaded, Retry::Restart));
    assert_eq!((e.message.as_str(), e.request_id.as_deref()), ("a [31mb", Some("req_head")));
    assert_eq!(text(&r), "hi");
}

/// The end of a body often comes just after the end of the answer. Waiting for it, for a moment, keeps
/// the connection, and with it a TLS handshake per turn. When it is free again is the machine's to
/// say, so this asks until it was.
#[tokio::test]
async fn a_finished_answer_leaves_its_connection_to_the_next() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let taken = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = taken.clone();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            tokio::spawn(async move {
                let mut buf = [0; 4096];
                while matches!(socket.read(&mut buf).await, Ok(n) if n > 0) {
                    let body = [STOP, DONE].concat();
                    let head = "HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n";
                    let first = format!("{head}{:x}\r\n{body}\r\n", body.len());
                    let _ = socket.write_all(first.as_bytes()).await;
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    let _ = socket.write_all(b"0\r\n\r\n").await;
                }
            });
        }
    });
    let c = client(&url);
    for n in 1..=50 {
        let (r, e) = all(begins(&c).await).await;
        assert!(r.complete && e.is_none());
        if taken.load(std::sync::atomic::Ordering::SeqCst) < n {
            return;
        }
        tokio::time::sleep(Duration::from_millis(n as u64)).await;
    }
    panic!("no connection was used twice");
}

// Cancelling.

#[tokio::test]
async fn dropping_an_answer_closes_the_connection() {
    let closed = Arc::new(Notify::new());
    let s = Server::new(vec![send(STREAM), send(HI), Step::UntilClosed(closed.clone())]).await;
    let mut answer = begins(&client(&s.url())).await;
    assert!(matches!(answer.next().await, Some(Ok(Event::Started { .. }))));
    drop(answer);
    timeout(LONG, closed.notified()).await.unwrap();
}

#[tokio::test]
async fn dropping_a_request_closes_the_connection() {
    let closed = Arc::new(Notify::new());
    let s = Server::new(vec![Step::UntilClosed(closed.clone())]).await;
    let c = client(&s.url());
    let task = tokio::spawn(async move { c.stream(&request()).await.is_ok() });
    while s.asked().is_empty() {
        tokio::task::yield_now().await;
    }
    task.abort();
    timeout(LONG, closed.notified()).await.unwrap();
}

#[tokio::test]
async fn an_error_lets_go_of_the_connection() {
    let closed = Arc::new(Notify::new());
    let s = Server::new(vec![send(STREAM), send("data: x\n\n"), Step::UntilClosed(closed.clone())])
        .await;
    let mut answer = begins(&client(&s.url())).await;
    assert!(answer.next().await.unwrap().is_err());
    timeout(LONG, closed.notified()).await.unwrap();
    assert!(answer.next().await.is_none());
}

#[tokio::test]
async fn giving_up_on_one_event_loses_none() {
    let go = Arc::new(Notify::new());
    let s =
        Server::new(vec![send(STREAM), Step::Wait(go.clone()), send([HI, STOP, DONE].concat())])
            .await;
    let mut answer = begins(&client(&s.url())).await;
    for _ in 0..3 {
        assert!(timeout(Duration::from_millis(5), answer.next()).await.is_err());
    }
    go.notify_one();
    let (r, e) = all(answer).await;
    assert_eq!((text(&r).as_str(), r.complete, e), ("hi", true, None));
}

// The list of models.

#[tokio::test]
async fn models() {
    let list = |head: String, body: Vec<u8>| async move {
        let s = Server::new(vec![Step::Send(head.into_bytes()), Step::Send(body)]).await;
        let c = client(&s.url());
        let got = timeout(LONG, c.list_models()).await.unwrap();
        let [asked] = s.asked().try_into().unwrap();
        assert!(asked.head.starts_with("GET /v1/models HTTP/1.1\r\n"));
        assert_eq!(asked.header("authorization"), Some(format!("Bearer {KEY}").as_str()));
        assert_eq!(asked.header("accept"), Some("application/json"));
        got.map_err(provider)
    };
    let ok = || head("200 OK", "content-type: application/json\r\n");
    let (recorded_head, body) = recorded("models");
    let got = list(format!("{recorded_head}\r\n\r\n"), body).await.unwrap();
    let ids: Vec<&str> = got.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids.len(), 4);
    assert!(ids.contains(&"qwen2.5-0.5b-instruct-mlx"));
    assert!(got[0].native.get().contains(r#""owned_by": "organization_owner""#));
    assert!(list(ok(), br#"{"data":[]}"#.to_vec()).await.unwrap().is_empty());

    let class = |r: Result<_, Error>| r.map(|_: Vec<_>| ()).unwrap_err().class;
    for bad in [&b"[]"[..], b"", b"{\"data\":[{}]}", b"{\"data\":[{\"id\":7}]}", b"<html>"] {
        assert_eq!(class(list(ok(), bad.to_vec()).await), Class::Protocol);
    }
    let many =
        |n: usize| format!(r#"{{"data":[{}]}}"#, vec![r#"{"id":"m"}"#; n].join(",")).into_bytes();
    assert_eq!(list(ok(), many(10_000)).await.unwrap().len(), 10_000);
    assert_eq!(class(list(ok(), many(10_001)).await), Class::Protocol);
    let unauthorised = head("401 x", "");
    assert_eq!(class(list(unauthorised, b"{}".to_vec()).await), Class::Auth);
    // As LM Studio answers an address it does not have.
    let e = list(ok(), br#"{"error":"Unexpected endpoint or method. (GET /v1/nope)"}"#.to_vec())
        .await
        .unwrap_err();
    assert_eq!(
        (e.class, e.message.as_str()),
        (Class::Server, "Unexpected endpoint or method. (GET /v1/nope)")
    );

    let e = anthropic("http://127.0.0.1:1/v1").list_models().await.unwrap_err();
    assert_eq!(e, CallError::Encode(EncodeError::Unsupported("the list of models")));
}

#[tokio::test]
async fn openais_models() {
    let s = Server::new(vec![
        send(head("200 OK", "content-type: application/json\r\n")),
        Step::Send(fixture("responses-from-documentation/models.json")),
    ])
    .await;
    let c = client_with(Profile::openai(), &s.url(), Options::default());
    let got = timeout(LONG, c.list_models()).await.unwrap().unwrap();
    let ids: Vec<&str> = got.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["model-id-0", "model-id-1", "model-id-2"]);
    assert!(got[2].native.get().contains(r#""shutdown_date": "2026-10-23""#));
    assert!(s.asked()[0].head.starts_with("GET /v1/models HTTP/1.1\r\n"));
}

/// Pages, one for each connection.
async fn paged(pages: Vec<String>) -> (Result<Vec<axiom_model::ModelInfo>, Error>, Vec<String>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1beta", listener.local_addr().unwrap());
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = asked.clone();
    let server = tokio::spawn(async move {
        for page in pages {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut raw = Vec::new();
            while !raw.ends_with(b"\r\n\r\n") {
                let mut buf = [0; 4096];
                let n = socket.read(&mut buf).await.unwrap();
                raw.extend_from_slice(&buf[..n]);
            }
            seen.lock().unwrap().push(String::from_utf8(raw).unwrap());
            let head = head("200 OK", "content-type: application/json\r\n");
            let _ = socket.write_all(head.as_bytes()).await;
            let _ = socket.write_all(page.as_bytes()).await;
        }
    });
    let c = client_with(Profile::gemini(), &url, Options::default());
    let got = timeout(LONG, c.list_models()).await.unwrap().map_err(provider);
    server.abort();
    let asked = asked.lock().unwrap().clone();
    (got, asked)
}

#[tokio::test]
async fn geminis_models_come_in_pages() {
    let first = r#"{"models":[{"name":"models/a","supportedGenerationMethods":["countTokens","generateContent"],"inputTokenLimit":1},{"name":"models/embeds","supportedGenerationMethods":["embedContent"]},{"name":"models/none","supportedGenerationMethods":[]}],"nextPageToken":"p&2 /=é"}"#;
    let second = r#"{"models":[{"name":"b"},{"name":"tunedModels/c"}],"nextPageToken":""}"#;
    let (got, asked) = paged(vec![first.into(), second.into(), "{}".into()]).await;
    let got = got.unwrap();
    // Those that can be asked as the codec asks; and what says nothing of it.
    let ids: Vec<&str> = got.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["a", "b", "tunedModels/c"]);
    assert!(got[0].native.get().contains(r#""inputTokenLimit":1"#));
    // The server's token is a value and nothing more.
    let lines: Vec<&str> = asked.iter().map(|a| a.lines().next().unwrap()).collect();
    assert_eq!(
        lines,
        [
            "GET /v1beta/models?pageSize=1000 HTTP/1.1",
            "GET /v1beta/models?pageSize=1000&pageToken=p%262+%2F%3D%C3%A9 HTTP/1.1",
        ]
    );
    assert!(asked.iter().all(|a| a.contains(&format!("x-goog-api-key: {KEY}\r\n"))));
    // No list is an empty one.
    assert_eq!(paged(vec!["{}".into()]).await.0.unwrap(), []);
}

#[tokio::test]
async fn geminis_models_are_bounded() {
    let class = |r: Result<Vec<_>, Error>| r.unwrap_err();
    // A list without an end.
    let page = |n: usize| {
        format!(r#"{{"models":[{}],"nextPageToken":"t"}}"#, vec![r#"{"name":"m"}"#; n].join(","))
    };
    let (got, asked) = paged(vec![page(1); 12]).await;
    let e = class(got);
    assert_eq!((e.class, asked.len()), (Class::Protocol, 10));
    assert_eq!(e.message, "the list of models has more than 10 pages");
    // As many as there may be, and one more.
    let mut pages = vec![page(1_000); 9];
    pages.push(format!(r#"{{"models":[{}]}}"#, vec![r#"{"name":"m"}"#; 1_000].join(",")));
    assert_eq!(paged(pages.clone()).await.0.unwrap().len(), 10_000);
    pages[0] = page(1_001);
    assert!(class(paged(pages).await.0).message.contains("more than 10000 models"));
    // What cannot be asked is counted too: it has to be read.
    let none = r#"{"name":"m","supportedGenerationMethods":[]}"#;
    let many = format!(r#"{{"models":[{}]}}"#, vec![none; 10_001].join(","));
    assert_eq!(class(paged(vec![many]).await.0).class, Class::Protocol);
    // All pages together.
    let big = format!(r#"{{"models":[],"x":"{}","nextPageToken":"t"}}"#, "x".repeat(2 << 20));
    let (got, asked) = paged(vec![big; 4]).await;
    assert_eq!(class(got).message, "the list of models is larger than 4194304 bytes");
    assert_eq!(asked.len(), 2);
    for bad in
        ["[]", "", "<html>", r#"{"models":[{}]}"#, r#"{"models":[{"name":7}]}"#, r#"{"models":7}"#]
    {
        assert_eq!(class(paged(vec![bad.into()]).await.0).class, Class::Protocol, "{bad}");
    }
    let e = class(
        paged(vec![r#"{"error":{"code":403,"message":"m","status":"PERMISSION_DENIED"}}"#.into()])
            .await
            .0,
    );
    // It has no list, and is not read as an empty one.
    assert_eq!((e.class, e.message.as_str()), (Class::Auth, "m"));
}

/// Every recorded answer, through the whole of it, as the server framed it.
#[tokio::test]
async fn recorded_answers_through_the_transport() {
    for (name, calls, class) in [
        ("text", 0, None),
        ("tool", 1, None),
        ("tool-parallel", 2, None),
        ("tool-cut", 0, None),
        ("too-long", 0, Some(Class::ContextTooLong)),
        ("error-request", 0, Some(Class::BadRequest)),
    ] {
        let raw = fixture(&format!("lmstudio-recorded/{name}.http"));
        let s = Server::new(vec![Step::Send(raw)]).await;
        let got = match timeout(LONG, client(&s.url()).stream(&request())).await.unwrap() {
            Ok(answer) => {
                let (r, e) = all(answer).await;
                (r.message.calls().count(), e.map(|e| e.class))
            }
            Err(e) => (0, Some(provider(e).class)),
        };
        assert_eq!(got, (calls, class), "{name}");
    }
}
