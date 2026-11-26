//! Signing in to a ChatGPT plan, renewing, using and leaving it, against a server on this machine
//! that does as OpenAI documents. No browser is opened: the tests are the browser.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axiom_host::{
    Config,
    client::{Client, ClientError},
    connections::{Connections, MemoryVault, Text, Vault},
};
use axiom_model::chatgpt::Places;
use axiom_proto::*;
use ring::{
    rand::SystemRandom,
    signature::{KeyPair, RSA_PKCS1_SHA256, RsaKeyPair, RsaPublicKeyComponents},
};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Notify,
};

mod common;
use common::*;

const KEY: &[u8] = include_bytes!("../../model/fixtures/chatgpt/test-only.pk8");
const CLIENT: &str = "oaiapp_test1";

fn base64url(b: &[u8]) -> String {
    const SIGNS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for c in b.chunks(3) {
        let n = c.iter().fold(0u32, |n, b| n << 8 | u32::from(*b)) << (8 * (3 - c.len()));
        for i in 0..=c.len() {
            out.push(char::from(SIGNS[(n >> (18 - 6 * i) & 63) as usize]));
        }
    }
    out
}
fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}
fn identity(nonce: &str, subject: &str, email: &str) -> String {
    let pair = RsaKeyPair::from_pkcs8(KEY).unwrap();
    let text = format!(
        "{}.{}",
        base64url(br#"{"alg":"RS256","kid":"k1"}"#),
        base64url(
            json!({"iss": "https://auth.openai.com", "aud": CLIENT, "sub": subject,
                "email": email, "iat": now(), "exp": now() + 600, "nonce": nonce})
            .to_string()
            .as_bytes()
        )
    );
    let mut sign = vec![0; pair.public().modulus_len()];
    pair.sign(&RSA_PKCS1_SHA256, &SystemRandom::new(), text.as_bytes(), &mut sign).unwrap();
    format!("{text}.{}", base64url(&sign))
}

/// What the other side does, and what was asked of it.
struct Other {
    /// The next sign-in's.
    nonce: String,
    subject: String,
    email: String,
    scope: String,
    expires_in: u64,
    /// Counts what is given out.
    n: usize,
    /// What renews, until it has.
    refresh: String,
    /// Said instead, once each.
    instead: VecDeque<(u16, Value)>,
    revoke: u16,
    /// The plan's limit is reached.
    spent: bool,
    wait: Option<Arc<Notify>>,
    asked: Vec<(String, String, String)>,
}
#[derive(Clone)]
struct OpenAi(Arc<Mutex<Other>>, String);

impl OpenAi {
    async fn start() -> Self {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let at = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
        let other = Other {
            nonce: String::new(),
            subject: "user-1".into(),
            email: "me@example.com".into(),
            scope: "openid email offline_access resource.invoke chatgpt.tokens.use.direct".into(),
            expires_in: 3600,
            n: 0,
            refresh: String::new(),
            instead: VecDeque::new(),
            revoke: 200,
            spent: false,
            wait: None,
            asked: Vec::new(),
        };
        let this = Self(Arc::new(Mutex::new(other)), at);
        let it = this.clone();
        tokio::spawn(async move {
            while let Ok((s, _)) = l.accept().await {
                let it = it.clone();
                tokio::spawn(async move { it.serve(s).await });
            }
        });
        this
    }
    fn places(&self) -> Places {
        let at = &self.1;
        Places {
            authorize: format!("{at}/authorize"),
            token: format!("{at}/token"),
            revoke: format!("{at}/revoke"),
            keys: format!("{at}/keys"),
            resource: format!("{at}/v1"),
            ..Places::default()
        }
    }
    fn with<T>(&self, f: impl FnOnce(&mut Other) -> T) -> T {
        f(&mut self.0.lock().unwrap())
    }
    /// What was asked at a path: the head and the body of each.
    fn at(&self, path: &str) -> Vec<(String, BTreeMap<String, String>)> {
        self.with(|o| {
            let of = o.asked.iter().filter(|a| a.0 == path);
            of.map(|a| (a.1.clone(), pairs(&a.2))).collect()
        })
    }
    fn renewals(&self) -> usize {
        self.at("/token").iter().filter(|a| a.1["grant_type"] == "refresh_token").count()
    }
    async fn serve(self, mut s: TcpStream) {
        let mut b = Vec::new();
        let mut buf = [0; 8192];
        let (head, body) = loop {
            let n = s.read(&mut buf).await.unwrap_or(0);
            b.extend_from_slice(&buf[..n]);
            let t = String::from_utf8_lossy(&b).into_owned();
            if let Some((h, body)) = t.split_once("\r\n\r\n") {
                let length = h.lines().find_map(|l| {
                    l.to_lowercase().strip_prefix("content-length: ")?.parse::<usize>().ok()
                });
                if body.len() >= length.unwrap_or(0) {
                    break (h.to_owned(), body.to_owned());
                }
            }
            if n == 0 {
                return;
            }
        };
        let path = head.split(' ').nth(1).unwrap().to_owned();
        let (status, answer, wait) = self.with(|o| {
            o.asked.push((path.clone(), head, body.clone()));
            let wait = o.wait.clone().filter(|_| path == "/token");
            let (s, a) = o.answer(&path, &pairs(&body));
            (s, a, wait)
        });
        if let Some(w) = wait {
            w.notified().await;
        }
        let kind = if path == "/v1/responses" { "text/event-stream" } else { "application/json" };
        let head = format!(
            "HTTP/1.1 {status} X\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            answer.len()
        );
        let _ = s.write_all(head.as_bytes()).await;
        let _ = s.write_all(answer.as_bytes()).await;
        let _ = s.shutdown().await;
    }
}
impl Other {
    fn answer(&mut self, path: &str, form: &BTreeMap<String, String>) -> (u16, String) {
        let give = |o: &mut Self| {
            o.n += 1;
            o.refresh = format!("rt-canary-{}", o.n);
            json!({"access_token": format!("at-canary-{}", o.n), "refresh_token": o.refresh,
                "token_type": "Bearer", "expires_in": o.expires_in, "scope": o.scope})
        };
        match path {
            "/token" => {
                if let Some((s, v)) = self.instead.pop_front() {
                    return (s, v.to_string());
                }
                if form["grant_type"] == "authorization_code" {
                    let mut v = give(self);
                    v["id_token"] = json!(identity(&self.nonce, &self.subject, &self.email));
                    return (200, v.to_string());
                }
                if form["refresh_token"] != self.refresh {
                    return (400, json!({"error": "refresh_token_reused"}).to_string());
                }
                (200, give(self).to_string())
            }
            "/keys" => {
                let pair = RsaKeyPair::from_pkcs8(KEY).unwrap();
                let p = RsaPublicKeyComponents::<Vec<u8>>::from(pair.public_key());
                let k =
                    json!({"kty": "RSA", "kid": "k1", "n": base64url(&p.n), "e": base64url(&p.e)});
                (200, json!({ "keys": [k] }).to_string())
            }
            "/revoke" => (self.revoke, String::new()),
            "/v1/models" => (
                200,
                json!({"models": [{"slug": "gpt-a", "display_name": "GPT A", "visibility": "list"},
                    {"slug": "gpt-b", "display_name": "GPT B", "visibility": "list",
                        "supported_reasoning_levels": [{"effort": "low"}, {"effort": "max"}]},
                    {"slug": "x", "display_name": "X", "visibility": "hide"}]})
                .to_string(),
            ),
            "/v1/responses" if self.spent => (
                429,
                json!({"error": {"message": "the plan is used up", "type": "x",
                    "code": "subscription_sharing_usage_limit_exceeded"}})
                .to_string(),
            ),
            "/v1/responses" => (
                200,
                include_str!("../../model/fixtures/responses-from-documentation/text.sse").into(),
            ),
            _ => (404, String::new()),
        }
    }
}

fn pairs(form: &str) -> BTreeMap<String, String> {
    let plain = |s: &str| {
        let (mut out, mut b) = (Vec::new(), s.bytes());
        while let Some(c) = b.next() {
            out.push(match c {
                b'+' => b' ',
                b'%' => {
                    let h = [b.next().unwrap(), b.next().unwrap()];
                    u8::from_str_radix(std::str::from_utf8(&h).unwrap(), 16).unwrap()
                }
                c => c,
            });
        }
        String::from_utf8(out).unwrap()
    };
    let all = form.split('&').filter_map(|p| p.split_once('='));
    all.map(|(k, v)| (plain(k), plain(v))).collect()
}
fn query(url: &str) -> BTreeMap<String, String> {
    pairs(url.split_once('?').unwrap().1)
}

struct T {
    f: Fixture,
    o: OpenAi,
    vault: Arc<MemoryVault>,
    c: Client,
}
/// One sign-in, as the browser has it.
struct Going {
    id: String,
    asked: BTreeMap<String, String>,
    port: u16,
}

impl T {
    async fn new() -> Self {
        let (f, o, vault) = (fixture(), OpenAi::start().await, Arc::default());
        f.serve_with(config(&f, &o, &vault));
        let c = f.client().await;
        Self { f, o, vault, c }
    }
    async fn restart(&mut self) {
        self.c.call(Call::Shutdown { force: true }).await.unwrap();
        while self.f.paths.token().exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        self.f.serve_with(config(&self.f, &self.o, &self.vault));
        self.c = self.f.client().await;
    }
    async fn try_begin(&mut self, again: Option<&str>) -> Result<Going, ClientError> {
        let call = Call::SigninBegin { connection: again.map(Into::into) };
        let v = self.c.call(call).await?;
        let url = v["url"].as_str().unwrap();
        assert!(url.starts_with(&format!("{}/authorize?", self.o.1)), "{url}");
        let asked = query(url);
        let back = asked["redirect_uri"].strip_prefix("http://127.0.0.1:").unwrap();
        let port = back.strip_suffix("/auth/callback").unwrap().parse().unwrap();
        self.o.with(|o| o.nonce = asked["nonce"].clone());
        Ok(Going { id: v["signin"].as_str().unwrap().into(), asked, port })
    }
    async fn begin(&mut self) -> Going {
        self.try_begin(None).await.unwrap()
    }
    async fn state(&mut self, g: &Going) -> Value {
        self.c.call(Call::SigninState { signin: g.id.clone() }).await.unwrap()
    }
    async fn end(&mut self, g: &Going) -> Value {
        for _ in 0..500 {
            let s = self.state(g).await;
            if s["state"] != "waiting" && s["state"] != "finishing" {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("it does not end")
    }
    async fn sign_in(&mut self) -> String {
        let g = self.begin().await;
        assert!(g.come("code=c-canary&client_id=oaiapp_test1").await.contains("You are signed in"));
        let s = self.end(&g).await;
        s["connection"].as_str().unwrap_or_else(|| panic!("{s}")).to_owned()
    }
    async fn list(&mut self) -> Vec<Connection> {
        serde_json::from_value(self.c.call(Call::Connections {}).await.unwrap()).unwrap()
    }
    async fn models(&mut self, id: &str) -> Result<Value, ClientError> {
        self.c.call(Call::Models { connection: id.into() }).await
    }
    async fn problem(&mut self, g: &Going) -> String {
        let s = self.end(g).await;
        assert_eq!(s["state"], "failed", "{s}");
        s["problem"].as_str().unwrap().into()
    }
}
impl Going {
    /// The browser comes back with the right state and this.
    async fn come(&self, rest: &str) -> String {
        let state = &self.asked["state"];
        self.ask(&format!("GET /auth/callback?state={state}&{rest} HTTP/1.1")).await
    }
    async fn ask(&self, line: &str) -> String {
        self.say(&format!("{line}\r\nHost: 127.0.0.1:{}\r\n\r\n", self.port)).await
    }
    async fn say(&self, all: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", self.port)).await.unwrap();
        // It may be hung up on before it has said everything.
        let _ = s.write_all(all.as_bytes()).await;
        let mut out = String::new();
        let _ = tokio::time::timeout(LONG, s.read_to_string(&mut out)).await.unwrap();
        out
    }
    async fn open(&self) -> bool {
        TcpStream::connect(("127.0.0.1", self.port)).await.is_ok()
    }
}
fn config(f: &Fixture, o: &OpenAi, vault: &Arc<MemoryVault>) -> Config {
    let mut c = Config::new(f.paths.clone());
    c.vault = vault.clone();
    c.chatgpt = o.places();
    c.home = Some(f.dir.path().join("home"));
    c
}
fn no_canary(d: &Path) -> usize {
    let mut n = 0;
    for e in std::fs::read_dir(d).unwrap().flatten() {
        if e.file_type().unwrap().is_dir() {
            n += no_canary(&e.path());
        } else if let Ok(b) = std::fs::read(e.path()) {
            n += 1;
            assert!(!b.windows(6).any(|w| w == b"canary"), "{:?}", e.path());
        }
    }
    n
}
fn vaulted(v: &MemoryVault, id: &str) -> Option<String> {
    v.get(id).unwrap().map(|t| t.to_string())
}

#[tokio::test]
async fn a_person_signs_in_and_their_plan_is_used() {
    let mut t = T::new().await;
    let g = t.begin().await;
    assert_eq!(t.state(&g).await, json!({"state": "waiting"}));
    assert_eq!(g.asked["client_id"], "dynamic_agent_client");
    // What is not the one that is waited for learns nothing and ends nothing.
    let state = g.asked["state"].clone();
    let right = format!("/auth/callback?state={state}&code=c&client_id=oaiapp_test1");
    for line in [
        "GET / HTTP/1.1".to_owned(),
        "GET /auth/callback HTTP/1.1".into(),
        "GET /auth/callback?state=x&code=c&client_id=oaiapp_test1 HTTP/1.1".into(),
        "GET /auth/callback?state=x&error=access_denied HTTP/1.1".into(),
        format!("POST {right} HTTP/1.1"),
        format!("HEAD {right} HTTP/1.1"),
        format!("GET {right} HTTP/1.0"),
        format!("GET http://evil.example{right} HTTP/1.1"),
    ] {
        let a = g.ask(&line).await;
        assert!(a.starts_with("HTTP/1.1 404 ") && a.ends_with("\r\n\r\n"), "{line}: {a}");
    }
    let a = g.say(&format!("GET {right} HTTP/1.1\r\nHost: evil.example:{}\r\n\r\n", g.port)).await;
    assert!(a.starts_with("HTTP/1.1 404 "), "{a}");
    // Right in everything, and too long.
    let host = format!("Host: 127.0.0.1:{}", g.port);
    let long = format!("GET {right} HTTP/1.1\r\n{host}\r\nX: {}\r\n\r\n", "x".repeat(60_000));
    assert_eq!(g.say(&long).await, "");
    assert_eq!(t.state(&g).await, json!({"state": "waiting"}));
    // Connections that say nothing are not in the way.
    let mut idle = Vec::new();
    for _ in 0..10 {
        idle.push(TcpStream::connect(("127.0.0.1", g.port)).await.unwrap());
    }
    let page = g.come("code=c-canary&client_id=oaiapp_test1").await;
    assert!(page.contains("You are signed in") && !page.contains("canary"), "{page}");
    for has in [
        "Content-Security-Policy: default-src 'none'",
        "Cache-Control: no-store",
        "Referrer-Policy: no-referrer",
    ] {
        assert!(page.contains(has), "{page}");
    }
    // Known as soon as the page says it, whoever else still holds a connection.
    let s = t.state(&g).await;
    assert_eq!(s["state"], "done");
    let id = s["connection"].as_str().unwrap().to_owned();
    assert!(!g.open().await, "nothing listens when it is done");

    let [c] = t.list().await.try_into().unwrap();
    assert_eq!(
        (c.id.as_str(), c.name.as_str(), c.provider.as_str()),
        (id.as_str(), "me@example.com", "chatgpt")
    );
    assert_eq!((c.secret, c.lasts, c.problem, c.url), (true, false, None, None));
    assert_eq!(
        t.models(&id).await.unwrap(),
        // Of one nothing is said: what every model of OpenAI's takes. Of the other, what the list says.
        json!([
            {"id": "gpt-a", "name": "GPT A", "effort": true, "efforts": ["low", "medium", "high", "xhigh"]},
            {"id": "gpt-b", "name": "GPT B", "effort": true, "efforts": ["low", "max"]}
        ])
    );
    assert!(t.o.at("/v1/models")[0].0.contains("Bearer at-canary-1"));
    let asked = &t.o.at("/token")[0].1;
    assert_eq!((asked["code"].as_str(), asked["client_id"].as_str()), ("c-canary", CLIENT));
    // What renews it is in the vault; nothing of it is in a file or an answer (SI-3).
    assert_eq!(vaulted(&t.vault, &id).as_deref(), Some("rt-canary-1"));
    assert!(no_canary(t.f.dir.path()) > 3);
    let all = t.c.call(Call::Connections {}).await.unwrap().to_string();
    assert!(!all.contains("canary"));
    drop(idle);
}

#[tokio::test]
async fn several_accounts_and_the_same_installation() {
    let mut t = T::new().await;
    let (a, b) = (t.sign_in().await, t.sign_in().await);
    assert!(a != b && t.list().await.len() == 2);
    assert_eq!(connections(&t).lane(&a), Lane::Plan);
    assert_eq!(vaulted(&t.vault, &a).as_deref(), Some("rt-canary-1"));
    assert_eq!(vaulted(&t.vault, &b).as_deref(), Some("rt-canary-2"));
    let one = t.begin().await.asked["ext_agent_host_id"].clone();
    t.restart().await;
    let two = t.begin().await.asked["ext_agent_host_id"].clone();
    assert!(one == two && one.starts_with("urn:uuid:") && one.len() == 45, "{one} {two}");
    assert_eq!((&one[23..24], &one[..9]), ("4", "urn:uuid:"));
    // Another host has no access token yet: it renews, once, and keeps what it renews by next.
    t.models(&b).await.unwrap();
    assert_eq!((t.o.renewals(), vaulted(&t.vault, &b).as_deref()), (1, Some("rt-canary-3")));
    assert_eq!(vaulted(&t.vault, &a).as_deref(), Some("rt-canary-1"));
    // It cannot be added in another way.
    let call = Call::ConnectionAdd {
        name: "x".into(),
        provider: "chatgpt".into(),
        url: None,
        secret: Some(SecretText("x".into())),
    };
    assert!(t.c.call(call).await.is_err());
}

fn connections(t: &T) -> Arc<Connections> {
    let file = t.f.paths.data.join("connections.json");
    Arc::new(Connections::open(&file, t.vault.clone(), t.o.places()).unwrap())
}

#[tokio::test]
async fn it_is_renewed_once_and_to_its_end() {
    let mut t = T::new().await;
    let id = t.sign_in().await;
    let c = connections(&t);
    let client = c.client(&id).unwrap();
    // Several at once.
    let all = tokio::join!(
        client.list_models(),
        client.list_models(),
        client.list_models(),
        client.list_models()
    );
    assert!(all.0.is_ok() && all.1.is_ok() && all.2.is_ok() && all.3.is_ok());
    assert_eq!(t.o.renewals(), 1);
    client.list_models().await.unwrap();
    // Also by another client for it.
    c.client(&id).unwrap().list_models().await.unwrap();
    assert_eq!(t.o.renewals(), 1);

    // Who asked is gone before there is an answer: what came of it is kept.
    let c = connections(&t);
    let client = c.client(&id).unwrap();
    let gate = Arc::new(Notify::new());
    t.o.with(|o| o.wait = Some(gate.clone()));
    let asked = tokio::time::timeout(Duration::from_millis(300), client.list_models()).await;
    assert!(asked.is_err());
    t.o.with(|o| o.wait = None);
    gate.notify_one();
    client.list_models().await.unwrap();
    assert_eq!((t.o.renewals(), vaulted(&t.vault, &id).as_deref()), (2, Some("rt-canary-3")));

    // It is renewed before it ends, and one that is good still is used when that does not work.
    t.o.with(|o| o.expires_in = 2);
    let c = connections(&t);
    let client = c.client(&id).unwrap();
    client.list_models().await.unwrap();
    assert_eq!(t.o.renewals(), 3);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    t.o.with(|o| o.instead.push_back((503, json!({}))));
    client.list_models().await.unwrap();
    assert!(t.o.at("/v1/models").last().unwrap().0.contains("at-canary-4"));
    tokio::time::sleep(Duration::from_millis(1100)).await;
    t.o.with(|o| o.instead.push_back((503, json!({}))));
    let e = client.list_models().await.unwrap_err().to_string();
    assert!(e.contains("503") && !e.contains("canary"), "{e}");
    // And that was nothing for good.
    client.list_models().await.unwrap();
    assert_eq!(vaulted(&t.vault, &id).as_deref(), Some("rt-canary-5"));
}

#[tokio::test]
async fn when_it_is_over_somebody_signs_in_again() {
    let mut t = T::new().await;
    let id = t.sign_in().await;
    t.restart().await;
    t.o.with(|o| o.instead.push_back((400, json!({"error": "invalid_grant"}))));
    let e = refused(t.models(&id).await).message;
    assert!(e.contains("sign in again"), "{e}");
    let asked = t.o.at("/token").len();
    assert!(refused(t.models(&id).await).message.contains("sign in again"));
    assert_eq!(t.o.at("/token").len(), asked, "it is not asked again and again");
    let p = t.list().await[0].problem.clone();
    assert!(p.unwrap().contains("sign in again"));

    let g = t.try_begin(Some(&id)).await.unwrap();
    assert_eq!(
        (g.asked["client_id"].as_str(), g.asked["login_hint"].as_str()),
        (CLIENT, "me@example.com")
    );
    t.o.with(|o| o.email = "new@example.com".into());
    g.come("code=c").await;
    assert_eq!(t.end(&g).await, json!({"state": "done", "connection": id}));
    let [c] = t.list().await.try_into().unwrap();
    assert_eq!((c.id.as_str(), c.problem), (id.as_str(), None));
    t.models(&id).await.unwrap();
    assert!(t.o.at("/v1/models").last().unwrap().0.contains("at-canary-2"));
    assert_eq!(vaulted(&t.vault, &id).as_deref(), Some("rt-canary-2"));

    // Not as somebody else, nor as another registration; and what there was stays.
    let g = t.try_begin(Some(&id)).await.unwrap();
    assert_eq!(g.asked["login_hint"], "new@example.com");
    t.o.with(|o| o.subject = "user-2".into());
    g.come("code=c").await;
    assert!(t.problem(&g).await.contains("somebody else"));
    let g = t.try_begin(Some(&id)).await.unwrap();
    g.come("code=c&client_id=oaiapp_other").await;
    assert!(t.problem(&g).await.contains("another registration"));
    assert_eq!(vaulted(&t.vault, &id).as_deref(), Some("rt-canary-2"));
    // Nor what is no such account.
    let key = Call::ConnectionAdd {
        name: "k".into(),
        provider: "openai".into(),
        url: None,
        secret: Some(SecretText("k".into())),
    };
    let key = t.c.call(key).await.unwrap()["connection"].as_str().unwrap().to_owned();
    assert!(t.try_begin(Some(&key)).await.is_err() && t.try_begin(Some("c_x")).await.is_err());
    // Its part of the store is gone.
    t.vault.remove(&id).unwrap();
    t.restart().await;
    assert!(refused(t.models(&id).await).message.contains("sign in again"));
}

#[tokio::test]
async fn what_does_not_become_a_sign_in() {
    let mut t = T::new().await;
    let g = t.begin().await;
    let page = g.come("error=access_denied").await;
    assert!(page.contains("You are not signed in"), "{page}");
    assert!(t.problem(&g).await.contains("not allowed"));
    assert!(!g.open().await);

    let g = t.begin().await;
    t.c.call(Call::SigninCancel { signin: g.id.clone() }).await.unwrap();
    assert!(t.problem(&g).await.contains("cancelled"));
    assert!(!g.open().await);

    // The plan was not allowed: what was given is given back.
    t.o.with(|o| o.scope = "openid email".into());
    let g = t.begin().await;
    g.come("code=c&client_id=oaiapp_test1").await;
    assert!(t.problem(&g).await.contains("plan may not be used"));
    assert_eq!(t.o.at("/revoke")[0].1["token"], "rt-canary-1");
    t.o.with(|o| o.scope = "chatgpt.tokens.use.direct".into());

    // An identity of another sign-in, and a server that fails with words of its own.
    let g = t.begin().await;
    t.o.with(|o| o.nonce = "other".into());
    let page = g.come("code=c&client_id=oaiapp_test1").await;
    assert!(page.contains("another sign-in"), "{page}");
    let g = t.begin().await;
    t.o.with(|o| o.instead.push_back((400, json!({"error": "<script>x"}))));
    let page = g.come("code=c&client_id=oaiapp_test1").await;
    assert!(!page.contains("<script") && page.contains("&lt;script&gt;x"), "{page}");
    assert!(t.list().await.is_empty() && !t.f.paths.data.join("connections.json").exists());

    // No more than a few at once.
    let mut going = Vec::new();
    for _ in 0..4 {
        going.push(t.begin().await);
    }
    let Err(ClientError::Refused(e)) = t.try_begin(None).await else { panic!() };
    assert_eq!(e.code, ErrorCode::Busy);
    t.c.call(Call::SigninCancel { signin: going[0].id.clone() }).await.unwrap();
    t.problem(&going[0]).await;
    going.push(t.begin().await);
    assert!(
        refused(t.c.call(Call::SigninState { signin: "n_x".into() }).await).code
            == ErrorCode::NotFound
    );
    assert!(t.c.call(Call::SigninCancel { signin: "n_x".into() }).await.is_err());
    // A host that stops listens no more.
    t.c.call(Call::Shutdown { force: true }).await.unwrap();
    for _ in 0..500 {
        if !going[4].open().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("it listens still");
}

#[tokio::test]
async fn leaving_ends_it_there_as_well() {
    let mut t = T::new().await;
    let (a, b) = (t.sign_in().await, t.sign_in().await);
    let v = t.c.call(Call::ConnectionRemove { connection: b.clone() }).await.unwrap();
    assert_eq!(v, json!({"ended": true}));
    let asked = &t.o.at("/revoke")[0].1;
    assert_eq!((asked["token"].as_str(), asked["client_id"].as_str()), ("rt-canary-2", CLIENT));
    assert_eq!((vaulted(&t.vault, &b), t.list().await.len()), (None, 1));
    assert!(t.models(&b).await.is_err());
    // It cannot be said for certain, and it is gone here all the same.
    t.o.with(|o| o.revoke = 500);
    let v = t.c.call(Call::ConnectionRemove { connection: a.clone() }).await.unwrap();
    assert_eq!(v, json!({"ended": false}));
    assert!(vaulted(&t.vault, &a).is_none() && t.list().await.is_empty());
    assert!(t.c.call(Call::ConnectionRemove { connection: a }).await.is_err());

    // Who has a client for it from before gets nothing more, and renews nothing.
    t.o.with(|o| o.revoke = 200);
    let id = t.sign_in().await;
    let c = connections(&t);
    let client = c.client(&id).unwrap();
    client.list_models().await.unwrap();
    assert_eq!(c.clone().leave(id.clone()).await.unwrap(), Some(true));
    assert_eq!(t.o.at("/revoke").last().unwrap().1["token"], "rt-canary-4");
    let renewals = t.o.renewals();
    let e = client.list_models().await.unwrap_err().to_string();
    assert!(e.contains("removed"), "{e}");
    assert_eq!((t.o.renewals(), vaulted(&t.vault, &id)), (renewals, None));
}

/// Takes no more than it has been told to.
struct Full(MemoryVault, Mutex<usize>);
impl Vault for Full {
    fn put(&self, id: &str, secret: &Text) -> Result<(), String> {
        let mut n = self.1.lock().unwrap();
        *n = n.checked_sub(1).ok_or("the store is full")?;
        self.0.put(id, secret)
    }
    fn get(&self, id: &str) -> Result<Option<Text>, String> {
        self.0.get(id)
    }
    fn remove(&self, id: &str) -> Result<(), String> {
        self.0.remove(id)
    }
    fn lasts(&self) -> bool {
        true
    }
}

#[tokio::test]
async fn a_store_that_does_not_take_it() {
    let (f, o) = (fixture(), OpenAi::start().await);
    let vault = Arc::new(Full(MemoryVault::default(), Mutex::new(0)));
    let mut cfg = config(&f, &o, &Arc::default());
    cfg.vault = vault.clone();
    f.serve_with(cfg);
    let c = f.client().await;
    let mut t = T { f, o, vault: Arc::default(), c };
    // At the sign-in: there is none, here or there.
    let g = t.begin().await;
    g.come("code=c&client_id=oaiapp_test1").await;
    assert!(t.problem(&g).await.contains("the store is full"));
    assert_eq!(t.o.at("/revoke")[0].1["token"], "rt-canary-1");
    assert!(t.list().await.is_empty());
    // Later: it works as long as the host does, and that is said.
    *vault.1.lock().unwrap() = 1;
    t.o.with(|o| o.expires_in = 0);
    let id = t.sign_in().await;
    assert!(t.list().await[0].lasts);
    t.models(&id).await.unwrap();
    assert!(!t.list().await[0].lasts);
    t.models(&id).await.unwrap();
    assert_eq!((t.o.renewals(), t.o.with(|o| o.n)), (2, 4));
    *vault.1.lock().unwrap() = 1;
    t.models(&id).await.unwrap();
    assert!(t.list().await[0].lasts);
    assert_eq!(vault.0.get(&id).unwrap().unwrap().as_str(), "rt-canary-5");
}

/// Axiom's own loop on it, with its tools and nothing a plan refuses.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn an_agent_works_on_it() {
    let mut t = T::new().await;
    for d in ["w", "home"] {
        std::fs::create_dir(t.f.dir.path().join(d)).unwrap();
    }
    let id = t.sign_in().await;
    let w = t.f.dir.path().join("w").to_str().unwrap().to_owned();
    let call = Call::SessionCreate { title: "S".into(), workspace: Some(w) };
    let session = SessionId(t.c.call(call).await.unwrap()["session"].as_str().unwrap().into());
    t.c.call(Call::Follow { session: session.clone(), after: 0 }).await.unwrap();
    let call = Call::AgentAdd {
        session: session.clone(),
        name: "Lead".into(),
        execution: Execution {
            harness: "axiom".into(),
            provider: None,
            model: Some("gpt-a".into()),
            connection: Some(id.clone()),
            effort: None,
            environment: None,
            ..Execution::default()
        },
        access: axiom_proto::Access::Read,
        trust: false,
        instructions: vec![],
        apart: false,
        plan: false,
    };
    let agent = AgentId(t.c.call(call).await.unwrap()["agent"].as_str().unwrap().into());
    for over in [false, true] {
        if over {
            // Nothing is tried again and no other way is taken: that is the person's to choose.
            t.o.with(|o| o.spent = true);
        }
        let text = "hello".to_owned();
        let call = Call::Prompt { session: session.clone(), agent: agent.clone(), text, key: None };
        t.c.call(call).await.unwrap();
        let before = t.o.at("/v1/responses").len();
        let mut end = None;
        while end.is_none() {
            let m = t.c.next_within(LONG).await.unwrap().expect("the run ends");
            let Message::Events { events, .. } = m else { continue };
            end = events.into_iter().find_map(|e| match e.body {
                Body::RunEnded { outcome, error, .. } => Some((outcome, error)),
                _ => None,
            });
        }
        let asked = t.o.at("/v1/responses").len() - before;
        let (outcome, error) = end.unwrap();
        let expected = if over { Outcome::Failed } else { Outcome::Completed };
        assert_eq!((outcome, asked), (expected, 1), "{error:?}");
        assert!(!over || error.unwrap().contains("the plan is used up"));
    }
    let (head, body) = t.o.with(|o| {
        let a = o.asked.iter().find(|a| a.0 == "/v1/responses").unwrap();
        (a.1.clone(), serde_json::from_str::<Value>(&a.2).unwrap())
    });
    assert!(head.contains("Bearer at-canary-1"));
    let keys: Vec<&str> = body.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, ["input", "model", "store", "stream", "tools"]);
    assert_eq!((&body["store"], &body["stream"]), (&json!(false), &json!(true)));
    assert!(body["input"].as_array().unwrap().iter().all(|i| i["role"] != "system"));
    assert!(no_canary(t.f.dir.path()) > 3);
}

/// SI-20: the sign-in's listener is the only one there is on an IP address.
#[test]
fn nothing_else_listens() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_owned();
    let (mut todo, mut seen, mut found) = (vec![crates], 0, Vec::new());
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(d).unwrap().flatten() {
            let p = e.path();
            let name = p.file_name().unwrap().to_str().unwrap().to_owned();
            if p.is_dir() && name != "tests" && name != "examples" && name != "fixtures" {
                todo.push(p);
            } else if name.ends_with(".rs") && !name.ends_with("_tests.rs") {
                seen += 1;
                let all = std::fs::read_to_string(&p).unwrap();
                let own = all.split("#[cfg(test)]\nmod tests {").next().unwrap();
                for word in ["TcpListener", "TcpSocket", "UdpSocket", "socket2", "AF_INET"] {
                    if own.contains(word) {
                        found.push(format!("{name} {word}"));
                    }
                }
            }
        }
    }
    assert!(seen > 50, "{seen}");
    assert_eq!(found, ["signin.rs TcpListener"]);
    // And that on this machine's own address, which no other machine reaches.
    let own = include_str!("../src/signin.rs");
    assert_eq!(own.matches("TcpListener::bind").count(), 1);
    assert!(own.contains("TcpListener::bind((Ipv4Addr::LOCALHOST, 0))"));
}
