//! Signing in with ChatGPT, for a program that runs on its person's own computer, so that a plan of
//! theirs may be used (developers.openai.com/siwc/token-sharing-open-source, read 2026-11-25). Axiom
//! is registered for each account by the sign-in itself and has no secret. Nothing here opens a
//! browser, listens or keeps anything: that is the caller's.

use std::{
    fmt,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Url, header};
use ring::{
    digest,
    rand::{SecureRandom, SystemRandom},
    signature::{RSA_PKCS1_2048_8192_SHA256, RsaPublicKeyComponents},
};
use serde::Deserialize;
use serde_json::Value;
use zeroize::Zeroizing;

use crate::{
    secret::{Endpoint, SetupError},
    transport,
};

/// Text that is wiped when it is dropped.
pub type Text = Zeroizing<String>;

/// The path of the address the browser comes back to. Once registered it may not change.
pub const PATH: &str = "/auth/callback";
/// What plan use itself is called among the scopes.
const PLAN: &str = "chatgpt.tokens.use.direct";
const SCOPE: &str = "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
/// Who asks before it is registered.
const NEW: &str = "dynamic_agent_client";
const BODY: usize = 256 * 1024;
const LONGEST: Duration = Duration::from_secs(30);
/// Clocks differ.
const LEEWAY: u64 = 5;

/// Where OpenAI's parts are.
#[derive(Debug, Clone)]
pub struct Places {
    pub authorize: String,
    pub token: String,
    pub revoke: String,
    pub keys: String,
    pub issuer: String,
    pub resource: String,
}
impl Default for Places {
    fn default() -> Self {
        Self {
            authorize: "https://auth.openai.com/api/accounts/authorize".into(),
            token: "https://auth.openai.com/api/accounts/oauth/token".into(),
            revoke: "https://auth.openai.com/api/accounts/oauth/revoke".into(),
            keys: "https://auth.openai.com/.well-known/jwks.json".into(),
            issuer: "https://auth.openai.com".into(),
            resource: "https://api.openai.com/v1".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Failed {
    /// A person said no, there or here.
    #[error("the sign-in was not allowed")]
    Denied,
    #[error("the plan may not be used: that was not allowed at the sign-in")]
    NoPlan,
    /// What came back is not an answer to what was asked.
    #[error("the sign-in cannot be trusted: {0}")]
    Untrusted(&'static str),
    /// It is over: somebody has to sign in again.
    #[error("the sign-in has ended ({0}); sign in again")]
    Over(String),
    /// Later it may work. Nothing is to be thrown away for it.
    #[error("OpenAI's sign-in did not answer as it should: {0}")]
    Later(String),
    #[error("it cannot be set up: {0}")]
    Setup(String),
}
impl From<SetupError> for Failed {
    fn from(e: SetupError) -> Self {
        Failed::Setup(e.to_string())
    }
}

/// Who signed in, as their identity token says and its signature vouches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    pub subject: String,
    pub email: Option<String>,
}

/// What a sign-in gave.
pub struct Signed {
    /// What Axiom is called for this account, from now on.
    pub client: String,
    pub person: Person,
    pub tokens: Tokens,
}

/// An access token, until when, and the one that renews it, which is another each time.
pub struct Tokens {
    pub access: Text,
    pub until: SystemTime,
    pub refresh: Text,
}
impl fmt::Debug for Tokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Tokens")
    }
}
impl fmt::Debug for Signed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Signed")
            .field("client", &self.client)
            .field("person", &self.person)
            .finish()
    }
}

/// An account that has signed in before.
#[derive(Debug, Clone)]
pub struct Known {
    pub client: String,
    pub subject: String,
    pub email: Option<String>,
}

/// One sign-in, from the address that is opened to what came of it.
pub struct Attempt {
    state: String,
    nonce: String,
    verifier: Text,
    redirect: String,
    known: Option<Known>,
    /// What a person opens in their browser.
    pub url: String,
}

/// What the browser came back with.
#[derive(Debug, PartialEq, Eq)]
pub enum Back {
    /// Not from this sign-in: nothing is said to it and nothing changes.
    NotOurs,
    Failed(Failed),
    Code {
        code: String,
        client: String,
    },
}

fn random(bytes: usize) -> Result<String, Failed> {
    let mut b = vec![0u8; bytes];
    SystemRandom::new().fill(&mut b).map_err(|_| Failed::Setup("no randomness".into()))?;
    Ok(base64url(&b))
}

/// A name for an installation: random, and nothing else.
pub fn installation() -> Result<String, Failed> {
    let mut b = [0u8; 16];
    SystemRandom::new().fill(&mut b).map_err(|_| Failed::Setup("no randomness".into()))?;
    (b[6], b[8]) = (b[6] & 0x0f | 0x40, b[8] & 0x3f | 0x80);
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    let part = [&hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..]];
    Ok(format!("urn:uuid:{}", part.join("-")))
}

impl Attempt {
    /// `host` names this installation, always the same; `port` is where the browser comes back to on
    /// this machine.
    pub fn new(
        p: &Places,
        app: &str,
        host: &str,
        port: u16,
        known: Option<Known>,
    ) -> Result<Self, Failed> {
        let (state, nonce, verifier) = (random(32)?, random(32)?, Zeroizing::new(random(48)?));
        let challenge = base64url(digest::digest(&digest::SHA256, verifier.as_bytes()).as_ref());
        // The address and not the name: what a name leads to is somebody else's word.
        let redirect = format!("http://127.0.0.1:{port}{PATH}");
        let mut url = Url::parse(&p.authorize).map_err(|e| Failed::Setup(e.to_string()))?;
        {
            let mut q = url.query_pairs_mut();
            match &known {
                Some(k) => {
                    q.append_pair("client_id", &k.client);
                    if let Some(e) = &k.email {
                        q.append_pair("login_hint", e);
                    }
                }
                None => {
                    q.append_pair("client_id", NEW).append_pair("agent_name_hint", app);
                }
            }
            q.append_pair("ext_agent_host_id", host)
                .append_pair("response_type", "code")
                .append_pair("redirect_uri", &redirect)
                .append_pair("scope", SCOPE)
                .append_pair("resource", &p.resource)
                .append_pair("state", &state)
                .append_pair("nonce", &nonce)
                .append_pair("code_challenge_method", "S256")
                .append_pair("code_challenge", &challenge);
        }
        Ok(Self { state, nonce, verifier, redirect, known, url: url.into() })
    }

    /// Reads what a request to the loopback address asked for, from its path on.
    pub fn back(&self, target: &str) -> Back {
        let Ok(url) = Url::parse("http://127.0.0.1").and_then(|u| u.join(target)) else {
            return Back::NotOurs;
        };
        if url.path() != PATH || url.host_str() != Some("127.0.0.1") {
            return Back::NotOurs;
        }
        let mut all: Vec<(String, String)> = url.query_pairs().into_owned().collect();
        // Said twice, it would be for whoever reads it to choose.
        let mut take = |k: &str| {
            let mut found = all.iter().position(|p| p.0 == k).map(|i| all.swap_remove(i).1);
            if all.iter().any(|p| p.0 == k) {
                found = None;
            }
            found
        };
        // Before anything else, also before an error is believed.
        if !take("state").is_some_and(|s| same(&s, &self.state)) {
            return Back::NotOurs;
        }
        if let Some(e) = take("error") {
            return Back::Failed(if e == "access_denied" {
                Failed::Denied
            } else {
                Failed::Later(plain(&e))
            });
        }
        let Some(code) = take("code").filter(|c| c.len() <= 4096 && !c.is_empty()) else {
            return Back::Failed(Failed::Untrusted("it came back without a code"));
        };
        let issued = take("client_id");
        match (&self.known, issued) {
            // It is who it was, whether or not that is said again.
            (Some(k), None) => Back::Code { code, client: k.client.clone() },
            (Some(k), Some(c)) if c == k.client => Back::Code { code, client: c },
            (Some(_), Some(_)) => {
                Back::Failed(Failed::Untrusted("it came back as another registration"))
            }
            (None, Some(client)) if registered(&client) => Back::Code { code, client },
            (None, _) => Back::Failed(Failed::Untrusted("it came back without a registration")),
        }
    }

    /// Trades the code for tokens, and makes sure of who they are for.
    pub async fn finish(&self, p: &Places, code: &str, client: &str) -> Result<Signed, Failed> {
        let form = [
            ("grant_type", "authorization_code"),
            ("client_id", client),
            ("code", code),
            ("code_verifier", self.verifier.as_str()),
            ("redirect_uri", self.redirect.as_str()),
            ("resource", p.resource.as_str()),
        ];
        let got = tokens(p, &form).await?;
        let id = got.id_token.as_deref().ok_or(Failed::Untrusted("there is no identity"))?;
        let keys = fetch_keys(p).await?;
        let person = identity(id, &keys, p, client, Some(&self.nonce), now())?;
        if let Some(k) = &self.known
            && k.subject != person.subject
        {
            return Err(Failed::Untrusted("somebody else signed in"));
        }
        let plan = got.plan();
        let tokens = got.whole(None)?;
        if !plan {
            // It is not kept: what it would be good for was not given.
            revoke(p, client, &tokens.refresh).await;
            return Err(Failed::NoPlan);
        }
        Ok(Signed { client: client.to_owned(), person, tokens })
    }
}

/// Another access token, and another to renew it by: the one that was used is gone.
pub async fn renew(p: &Places, client: &str, refresh: &Text) -> Result<Tokens, Failed> {
    let form = [
        ("grant_type", "refresh_token"),
        ("client_id", client),
        ("refresh_token", refresh.as_str()),
        ("resource", p.resource.as_str()),
    ];
    let got = tokens(p, &form).await?;
    if got.scope.is_some() && !got.plan() {
        return Err(Failed::NoPlan);
    }
    got.whole(Some(refresh))
}

/// Ends it there. Whether that is certain.
pub async fn revoke(p: &Places, client: &str, refresh: &Text) -> bool {
    let form =
        [("token", refresh.as_str()), ("token_type_hint", "refresh_token"), ("client_id", client)];
    matches!(post(&p.revoke, &form).await, Ok((200, _)))
}

fn wiped<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Text>, D::Error> {
    Ok(Option::<String>::deserialize(d)?.map(Zeroizing::new))
}

#[derive(Deserialize)]
struct Got {
    #[serde(default, deserialize_with = "wiped")]
    access_token: Option<Text>,
    #[serde(default, deserialize_with = "wiped")]
    refresh_token: Option<Text>,
    #[serde(default, deserialize_with = "wiped")]
    id_token: Option<Text>,
    token_type: Option<String>,
    expires_in: Option<u64>,
    scope: Option<String>,
}
impl Got {
    fn plan(&self) -> bool {
        self.scope.as_deref().unwrap_or_default().split_whitespace().any(|s| s == PLAN)
    }
    fn whole(self, was: Option<&Text>) -> Result<Tokens, Failed> {
        let not = |w| Err(Failed::Later(format!("it gave no {w}")));
        let Some(access) = self.access_token.filter(|t| good(t)) else { return not("access") };
        if !self.token_type.is_none_or(|t| t.eq_ignore_ascii_case("bearer")) {
            return not("token of a kind that is used here");
        }
        // Documented to come with each; without one, the old may be good still.
        let Some(refresh) = self.refresh_token.filter(|t| good(t)).or_else(|| was.cloned()) else {
            return not("way to renew it");
        };
        let lasts = Duration::from_secs(self.expires_in.unwrap_or(3600).min(24 * 3600));
        Ok(Tokens { access, until: SystemTime::now() + lasts, refresh })
    }
}
fn good(t: &str) -> bool {
    !t.is_empty() && t.len() <= 64 * 1024 && t.bytes().all(|b| b.is_ascii_graphic())
}

async fn tokens(p: &Places, form: &[(&str, &str)]) -> Result<Got, Failed> {
    let (status, body) = post(&p.token, form).await?;
    if status == 200 {
        return serde_json::from_slice(&body)
            .map_err(|_| Failed::Later("its answer cannot be read".into()));
    }
    let v: Value = serde_json::from_slice(&body).unwrap_or_default();
    let code = v["error"].as_str().or_else(|| v["error"]["code"].as_str()).unwrap_or_default();
    Err(match code {
        "invalid_grant"
        | "invalid_refresh_token"
        | "token_expired"
        | "refresh_token_expired"
        | "refresh_token_invalidated"
        | "refresh_token_reused" => Failed::Over(code.into()),
        // Also what is wrong with us: nothing is thrown away for it.
        _ => Failed::Later(match plain(code) {
            c if c.is_empty() => format!("status {status}"),
            c => format!("{c}, status {status}"),
        }),
    })
}

async fn send(request: reqwest::RequestBuilder) -> Result<(u16, Zeroizing<Vec<u8>>), Failed> {
    let later = |e: &reqwest::Error| Failed::Later(transport::innermost(e));
    let mut r = request.timeout(LONGEST).send().await.map_err(|e| later(&e))?;
    let mut body = Zeroizing::new(Vec::new());
    while let Some(b) = r.chunk().await.map_err(|e| later(&e))? {
        if body.len() + b.len() > BODY {
            return Err(Failed::Later("its answer is too long".into()));
        }
        body.extend_from_slice(&b);
    }
    Ok((r.status().as_u16(), body))
}

async fn post(url: &str, form: &[(&str, &str)]) -> Result<(u16, Zeroizing<Vec<u8>>), Failed> {
    let endpoint = Endpoint::parse(url)?;
    let http = transport::build(&endpoint, &transport::Options::default())?;
    let mut q = Url::parse("http://x").map_err(|e| Failed::Setup(e.to_string()))?;
    q.query_pairs_mut().extend_pairs(form);
    let body = q.query().unwrap_or_default().to_owned();
    let request = http
        .post(url)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::ACCEPT, "application/json")
        .body(body);
    send(request).await
}

#[derive(Debug, Deserialize)]
struct Keys {
    keys: Vec<Key>,
}
#[derive(Debug, Deserialize)]
struct Key {
    kty: String,
    kid: Option<String>,
    #[serde(rename = "use")]
    for_: Option<String>,
    alg: Option<String>,
    n: Option<String>,
    e: Option<String>,
}

async fn fetch_keys(p: &Places) -> Result<Keys, Failed> {
    let endpoint = Endpoint::parse(&p.keys)?;
    let http = transport::build(&endpoint, &transport::Options::default())?;
    let (status, body) = send(http.get(&p.keys).header(header::ACCEPT, "application/json")).await?;
    if status != 200 {
        return Err(Failed::Later(format!("its keys, status {status}")));
    }
    serde_json::from_slice(&body).map_err(|_| Failed::Later("its keys cannot be read".into()))
}

/// Who an identity token is of, if it is OpenAI's, for this registration, for this sign-in and now.
fn identity(
    token: &str,
    keys: &Keys,
    p: &Places,
    client: &str,
    nonce: Option<&str>,
    now: u64,
) -> Result<Person, Failed> {
    let no = Failed::Untrusted;
    let mut parts = token.split('.');
    let (Some(head), Some(body), Some(sign), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(no("its identity cannot be read"));
    };
    let read = |s: &str| {
        let b = unbase64url(s)?;
        serde_json::from_slice::<Value>(&b).ok()
    };
    let h = read(head).ok_or(no("its identity cannot be read"))?;
    // One kind of signature, whatever it says it is.
    if h["alg"] != "RS256" || h.get("crit").is_some() {
        return Err(no("its identity is signed in another way"));
    }
    let kid = h["kid"].as_str().ok_or(no("its identity names no key"))?;
    let key = keys
        .keys
        .iter()
        .find(|k| {
            k.kty == "RSA"
                && k.kid.as_deref() == Some(kid)
                && k.for_.as_deref().is_none_or(|u| u == "sig")
                && k.alg.as_deref().is_none_or(|a| a == "RS256")
        })
        .ok_or(no("its identity is signed with a key OpenAI does not have"))?;
    let part = |s: &Option<String>| s.as_deref().and_then(unbase64url);
    let (n, e) = part(&key.n).zip(part(&key.e)).ok_or(no("OpenAI's key cannot be read"))?;
    let signature = unbase64url(sign).ok_or(no("its identity cannot be read"))?;
    let signed = &token[..head.len() + 1 + body.len()];
    RsaPublicKeyComponents { n: &n, e: &e }
        .verify(&RSA_PKCS1_2048_8192_SHA256, signed.as_bytes(), &signature)
        .map_err(|_| no("the signature of its identity is wrong"))?;
    let b = read(body).ok_or(no("its identity cannot be read"))?;
    if b["iss"] != p.issuer.as_str() {
        return Err(no("its identity is not OpenAI's"));
    }
    let for_us = match &b["aud"] {
        Value::String(a) => a == client,
        Value::Array(all) => all.iter().any(|a| a == client),
        _ => false,
    };
    if !for_us {
        return Err(no("its identity is for another program"));
    }
    if b["exp"].as_u64().is_none_or(|t| t + LEEWAY <= now) {
        return Err(no("its identity is out of date"));
    }
    if b["iat"].as_u64().is_none_or(|t| t > now + 60) {
        return Err(no("its identity is not of now"));
    }
    if let Some(n) = nonce
        && !b["nonce"].as_str().is_some_and(|x| same(x, n))
    {
        return Err(no("its identity is of another sign-in"));
    }
    let subject = b["sub"].as_str().filter(|s| !s.is_empty() && s.len() <= 256);
    let subject = subject.ok_or(no("its identity names nobody"))?.to_owned();
    let email = b["email"].as_str().filter(|e| e.len() <= 320).map(plain);
    Ok(Person { subject, email: email.filter(|e| !e.is_empty()) })
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Not the one everybody has before they are registered.
fn registered(c: &str) -> bool {
    c != NEW
        && (1..=200).contains(&c.len())
        && c.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

/// Fit to be shown.
fn plain(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(120).collect()
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |d, (x, y)| d | (x ^ y)) == 0
}

const SIGNS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64url(b: &[u8]) -> String {
    let mut out = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = c.iter().fold(0u32, |n, b| n << 8 | u32::from(*b)) << (8 * (3 - c.len()));
        for i in 0..=c.len() {
            out.push(char::from(SIGNS[(n >> (18 - 6 * i) & 63) as usize]));
        }
    }
    out
}

/// Without padding and nothing else, as a token has it.
fn unbase64url(s: &str) -> Option<Vec<u8>> {
    if s.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3 + 2);
    let (mut acc, mut bits) = (0u32, 0);
    for b in s.bytes() {
        let v = SIGNS.iter().position(|x| *x == b)?;
        acc = acc << 6 | u32::try_from(v).ok()?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits & 0xff) as u8);
        }
    }
    // What is left over of a letter is nothing, or it is another text's.
    (acc & ((1 << bits) - 1) == 0).then_some(out)
}

#[cfg(test)]
#[path = "chatgpt_tests.rs"]
mod tests;
