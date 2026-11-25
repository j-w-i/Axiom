#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::sync::{Arc, Mutex};

use ring::signature::{KeyPair, RSA_PKCS1_SHA256, RsaKeyPair};
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

use super::*;

/// Made for these tests and good for nothing else.
const KEY: &[u8] = include_bytes!("../fixtures/chatgpt/test-only.pk8");
const CLIENT: &str = "oaiapp_test1";

fn pair() -> RsaKeyPair {
    RsaKeyPair::from_pkcs8(KEY).unwrap()
}
fn jwks() -> Value {
    let pair = pair();
    let public = RsaPublicKeyComponents::<Vec<u8>>::from(pair.public_key());
    json!({"keys": [
        {"kty": "EC", "kid": "k1", "crv": "P-256"},
        {"kty": "RSA", "kid": "k1", "use": "sig", "alg": "RS256", "n": base64url(&public.n), "e": base64url(&public.e)},
        {"kty": "RSA", "kid": "enc", "use": "enc", "n": base64url(&public.n), "e": base64url(&public.e)},
    ]})
}
fn token(head: Value, body: Value) -> String {
    let text = format!(
        "{}.{}",
        base64url(head.to_string().as_bytes()),
        base64url(body.to_string().as_bytes())
    );
    let pair = pair();
    let mut sign = vec![0; pair.public().modulus_len()];
    pair.sign(&RSA_PKCS1_SHA256, &SystemRandom::new(), text.as_bytes(), &mut sign).unwrap();
    format!("{text}.{}", base64url(&sign))
}
fn head() -> Value {
    json!({"alg": "RS256", "kid": "k1", "typ": "JWT"})
}
fn claims(nonce: &str) -> Value {
    claims_at(nonce, now())
}
fn claims_at(nonce: &str, t: u64) -> Value {
    json!({"iss": "https://auth.openai.com", "aud": [CLIENT], "sub": "user-1", "email": "me@example.com",
        "iat": t, "exp": t + 600, "nonce": nonce})
}
fn check(t: &str, at: u64) -> Result<Person, Failed> {
    let keys = serde_json::from_value(jwks()).unwrap();
    identity(t, &keys, &Places::default(), CLIENT, Some("n1"), at)
}

#[test]
fn base64url_is_read_whole_or_not() {
    for (b, s) in
        [(&b""[..], ""), (b"f", "Zg"), (b"fo", "Zm8"), (b"foo", "Zm9v"), (b"\xfb\xff\xbf", "-_-_")]
    {
        assert_eq!((base64url(b).as_str(), unbase64url(s).as_deref()), (s, Some(b)));
    }
    // One more letter that says nothing, padding, another alphabet, bits that are nobody's.
    for bad in ["Z", "Zg==", "Zm9v+", "Zm9v/w", "Zh", "Zm9"] {
        assert_eq!(unbase64url(bad), None, "{bad}");
    }
}

#[test]
fn who_signed_in_is_who_the_token_vouches_for() {
    const T: u64 = 1_790_000_000;
    let good = token(head(), claims_at("n1", T));
    let me = check(&good, T).unwrap();
    assert_eq!((me.subject.as_str(), me.email.as_deref()), ("user-1", Some("me@example.com")));
    let with = |k: &str, v: Value| {
        let mut c = claims_at("n1", T);
        c[k] = v;
        token(head(), c)
    };
    assert!(check(&with("aud", json!(CLIENT)), T).is_ok());
    let without = |k: &str| {
        let mut c = claims_at("n1", T);
        c.as_object_mut().unwrap().remove(k);
        token(head(), c)
    };
    let as_ = |h: Value| token(h, claims_at("n1", T));
    let mut cut = good.clone();
    cut.pop();
    let parts: Vec<&str> = good.split('.').collect();
    let other = base64url(claims_at("n1", T).to_string().replace("user-1", "user-2").as_bytes());
    let none = format!("{}.{}.", base64url(br#"{"alg":"none","kid":"k1"}"#), parts[1]);
    for (bad, why) in [
        (with("iss", json!("https://evil.example")), "not OpenAI's"),
        (with("aud", json!(["another"])), "another program"),
        (with("aud", json!([])), "another program"),
        (with("aud", json!("dynamic_agent_client")), "another program"),
        (with("exp", json!(T - 60)), "out of date"),
        (without("exp"), "out of date"),
        (with("exp", json!("tomorrow")), "out of date"),
        (with("iat", json!(T + 3600)), "not of now"),
        (without("iat"), "not of now"),
        (with("nonce", json!("n2")), "another sign-in"),
        (without("nonce"), "another sign-in"),
        (with("sub", json!("")), "nobody"),
        (without("sub"), "nobody"),
        (as_(json!({"alg": "HS256", "kid": "k1"})), "another way"),
        (as_(json!({"alg": "RS512", "kid": "k1"})), "another way"),
        (as_(json!({"alg": "RS256", "kid": "k1", "crit": ["x"]})), "another way"),
        (as_(json!({"alg": "RS256"})), "names no key"),
        (as_(json!({"alg": "RS256", "kid": "k2"})), "does not have"),
        (as_(json!({"alg": "RS256", "kid": "enc"})), "does not have"),
        (none, "another way"),
        (format!("{}.{other}.{}", parts[0], parts[2]), "signature"),
        (cut, "read"),
        (format!("{good}.x"), "read"),
        ("a.b".into(), "read"),
        (String::new(), "read"),
    ] {
        match check(&bad, T) {
            Err(Failed::Untrusted(w)) => assert!(w.contains(why), "{w}, not {why}"),
            r => panic!("{why}: {r:?}"),
        }
    }
    // Clocks differ a little; not more.
    assert!(check(&good, T + 604).is_ok());
    assert!(check(&good, T + 605).is_err());
}

fn query(url: &str) -> std::collections::BTreeMap<String, String> {
    Url::parse(url).unwrap().query_pairs().into_owned().collect()
}

#[test]
fn what_is_asked_for_the_first_time_and_after() {
    let p = Places::default();
    let a = Attempt::new(&p, "Axiom", "urn:uuid:1", 4321, None).unwrap();
    assert!(a.url.starts_with("https://auth.openai.com/api/accounts/authorize?"));
    let q = query(&a.url);
    for (k, v) in [
        ("client_id", "dynamic_agent_client"),
        ("agent_name_hint", "Axiom"),
        ("ext_agent_host_id", "urn:uuid:1"),
        ("response_type", "code"),
        ("redirect_uri", "http://127.0.0.1:4321/auth/callback"),
        ("scope", "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct"),
        ("resource", "https://api.openai.com/v1"),
        ("code_challenge_method", "S256"),
    ] {
        assert_eq!(q.get(k).map(String::as_str), Some(v), "{k}");
    }
    assert!(!q.contains_key("login_hint") && !q.contains_key("prompt"));
    let digest = digest::digest(&digest::SHA256, a.verifier.as_bytes());
    assert_eq!(q["code_challenge"], base64url(digest.as_ref()));
    assert!(a.verifier.len() >= 43 && !a.url.contains(a.verifier.as_str()));
    // Nothing of it twice.
    let b = Attempt::new(&p, "Axiom", "urn:uuid:1", 4321, None).unwrap();
    let r = query(&b.url);
    for k in ["state", "nonce", "code_challenge"] {
        assert!(q[k].len() >= 43 && q[k] != r[k], "{k}");
    }
    let known = Known {
        client: CLIENT.into(),
        subject: "user-1".into(),
        email: Some("me@example.com".into()),
    };
    let q = query(&Attempt::new(&p, "Axiom", "urn:uuid:1", 1, Some(known)).unwrap().url);
    assert_eq!((q["client_id"].as_str(), q["login_hint"].as_str()), (CLIENT, "me@example.com"));
    assert!(!q.contains_key("agent_name_hint"));
}

#[test]
fn only_its_own_answer_is_taken() {
    let p = Places::default();
    let a = Attempt::new(&p, "Axiom", "h", 1, None).unwrap();
    let s = a.state.clone();
    let back = |q: &str| a.back(&format!("/auth/callback?{}", q.replace("STATE", &s)));
    assert_eq!(
        back("state=STATE&code=c1&client_id=oaiapp_test1&scope=x"),
        Back::Code { code: "c1".into(), client: CLIENT.into() }
    );
    for other in [
        "code=c1&client_id=oaiapp_test1",
        "state=x&code=c1&client_id=oaiapp_test1",
        "state=&code=c1&client_id=oaiapp_test1",
        "state=x&error=access_denied",
        "state=STATE&state=STATE&code=c1&client_id=oaiapp_test1",
        "state=STATEx&code=c1&client_id=oaiapp_test1",
    ] {
        assert_eq!(back(other), Back::NotOurs, "{other}");
    }
    for other in [
        "/",
        "/callback",
        "/auth/callback/",
        "/auth/callbackx",
        "//evil/auth/callback",
        "http://evil/auth/callback",
    ] {
        assert_eq!(
            a.back(&format!("{other}?state={s}&code=c&client_id=oaiapp_x")),
            Back::NotOurs,
            "{other}"
        );
    }
    assert_eq!(back("state=STATE&error=access_denied"), Back::Failed(Failed::Denied));
    assert_eq!(
        back("state=STATE&error=server_error&code=c1&client_id=oaiapp_x"),
        Back::Failed(Failed::Later("server_error".into()))
    );
    for (q, why) in [
        ("state=STATE&client_id=oaiapp_x", "without a code"),
        ("state=STATE&code=&client_id=oaiapp_x", "without a code"),
        ("state=STATE&code=a&code=b&client_id=oaiapp_x", "without a code"),
        ("state=STATE&code=c", "without a registration"),
        ("state=STATE&code=c&client_id=", "without a registration"),
        ("state=STATE&code=c&client_id=dynamic_agent_client", "without a registration"),
        ("state=STATE&code=c&client_id=a%20b", "without a registration"),
    ] {
        let Back::Failed(Failed::Untrusted(w)) = back(q) else { panic!("{q}") };
        assert!(w.contains(why), "{w}");
    }
    let known = Known { client: CLIENT.into(), subject: "user-1".into(), email: None };
    let a = Attempt::new(&p, "Axiom", "h", 1, Some(known)).unwrap();
    let s = a.state.clone();
    for q in ["", "&client_id=oaiapp_test1"] {
        let r = a.back(&format!("/auth/callback?state={s}&code=c{q}"));
        assert_eq!(r, Back::Code { code: "c".into(), client: CLIENT.into() });
    }
    let r = a.back(&format!("/auth/callback?state={s}&code=c&client_id=oaiapp_other"));
    assert!(matches!(r, Back::Failed(Failed::Untrusted(_))));
}

/// A sign-in service that answers from a list, and keeps what it was asked.
struct Service {
    places: Places,
    asked: Arc<Mutex<Vec<String>>>,
}
async fn service(answers: Vec<(u16, Value)>) -> Service {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let at = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
    let asked: Arc<Mutex<Vec<String>>> = Arc::default();
    let heard = asked.clone();
    tokio::spawn(async move {
        for (status, body) in answers {
            let Ok((mut s, _)) = l.accept().await else { return };
            let mut b = Vec::new();
            let mut buf = [0; 8192];
            loop {
                let n = s.read(&mut buf).await.unwrap_or(0);
                b.extend_from_slice(&buf[..n]);
                let t = String::from_utf8_lossy(&b).into_owned();
                let want = t
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase().strip_prefix("content-length: ").map(str::to_owned)
                    })
                    .and_then(|n| n.parse::<usize>().ok())
                    .unwrap_or(0);
                if n == 0 || t.split_once("\r\n\r\n").is_some_and(|(_, b)| b.len() >= want) {
                    break;
                }
            }
            heard.lock().unwrap().push(String::from_utf8_lossy(&b).into_owned());
            let body = match body {
                Value::String(s) => s,
                v => v.to_string(),
            };
            let head = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nLocation: http://127.0.0.1:9/\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = s.write_all(head.as_bytes()).await;
            let _ = s.write_all(body.as_bytes()).await;
            let _ = s.shutdown().await;
        }
    });
    let places = Places {
        authorize: format!("{at}/authorize"),
        token: format!("{at}/token"),
        revoke: format!("{at}/revoke"),
        keys: format!("{at}/keys"),
        ..Places::default()
    };
    Service { places, asked }
}
impl Service {
    fn forms(&self) -> Vec<std::collections::BTreeMap<String, String>> {
        let all = self.asked.lock().unwrap();
        all.iter()
            .map(|t| query(&format!("http://x/?{}", t.split_once("\r\n\r\n").unwrap().1)))
            .collect()
    }
}
fn gave(id: Option<String>) -> Value {
    json!({"access_token": "at-canary-1", "refresh_token": "rt-canary-1", "id_token": id, "token_type": "Bearer",
        "expires_in": 3600, "earliest_refresh_at": 1, "scope": "openid email offline_access resource.invoke chatgpt.tokens.use.direct"})
}

#[tokio::test]
async fn a_code_becomes_tokens_of_a_person() {
    let a = Attempt::new(&Places::default(), "Axiom", "h", 77, None).unwrap();
    let id = token(head(), claims(&a.nonce));
    let s = service(vec![(200, gave(Some(id.clone()))), (200, jwks())]).await;
    let got = a.finish(&s.places, "code-1", CLIENT).await.unwrap();
    assert_eq!((got.client.as_str(), got.person.subject.as_str()), (CLIENT, "user-1"));
    assert_eq!(
        (got.tokens.access.as_str(), got.tokens.refresh.as_str()),
        ("at-canary-1", "rt-canary-1")
    );
    let left = got.tokens.until.duration_since(SystemTime::now()).unwrap();
    assert!(left > Duration::from_secs(3500) && left <= Duration::from_secs(3600));
    assert!(!format!("{got:?}").contains("canary"));
    let f = &s.forms()[0];
    for (k, v) in [
        ("grant_type", "authorization_code"),
        ("client_id", CLIENT),
        ("code", "code-1"),
        ("redirect_uri", "http://127.0.0.1:77/auth/callback"),
        ("resource", "https://api.openai.com/v1"),
    ] {
        assert_eq!(f[k], v, "{k}");
    }
    assert_eq!(f["code_verifier"], a.verifier.as_str());
    assert!(s.asked.lock().unwrap()[0].starts_with("POST /token "));
}

#[tokio::test]
async fn what_is_not_as_it_should_be_is_not_signed_in() {
    let fresh = || Attempt::new(&Places::default(), "Axiom", "h", 77, None).unwrap();
    // Without the plan it is not kept, there either.
    let a = fresh();
    let mut g = gave(Some(token(head(), claims(&a.nonce))));
    g["scope"] = json!("openid email");
    let s = service(vec![(200, g), (200, jwks()), (200, json!(""))]).await;
    assert_eq!(a.finish(&s.places, "c", CLIENT).await.unwrap_err(), Failed::NoPlan);
    let asked = s.asked.lock().unwrap().clone();
    assert!(asked[2].starts_with("POST /revoke ") && asked[2].contains("token=rt-canary-1"));
    // Another person than the one who was to come back.
    let known = Known { client: CLIENT.into(), subject: "user-9".into(), email: None };
    let a = Attempt::new(&Places::default(), "Axiom", "h", 77, Some(known)).unwrap();
    let s = service(vec![(200, gave(Some(token(head(), claims(&a.nonce))))), (200, jwks())]).await;
    let e = a.finish(&s.places, "c", CLIENT).await.unwrap_err();
    assert_eq!(e, Failed::Untrusted("somebody else signed in"));
    // No identity; an identity of another sign-in; keys that are not there.
    let a = fresh();
    let s = service(vec![(200, gave(None))]).await;
    assert!(matches!(a.finish(&s.places, "c", CLIENT).await, Err(Failed::Untrusted(_))));
    let s = service(vec![(200, gave(Some(token(head(), claims("other"))))), (200, jwks())]).await;
    assert!(matches!(a.finish(&s.places, "c", CLIENT).await, Err(Failed::Untrusted(_))));
    let s =
        service(vec![(200, gave(Some(token(head(), claims(&a.nonce))))), (503, json!({}))]).await;
    assert!(matches!(a.finish(&s.places, "c", CLIENT).await, Err(Failed::Later(_))));
    for (status, body) in [
        (302, json!({})),
        (200, json!("nonsense")),
        (500, json!("<html>")),
        (400, json!({"error": "invalid_client"})),
    ] {
        let s = service(vec![(status, body)]).await;
        let r = a.finish(&s.places, "c", CLIENT).await;
        assert!(matches!(r, Err(Failed::Later(_))), "{status}: {r:?}");
        assert_eq!(s.asked.lock().unwrap().len(), 1, "a redirect is not followed");
    }
    let big = json!("x".repeat(BODY + 1));
    let s = service(vec![(200, big)]).await;
    assert!(matches!(a.finish(&s.places, "c", CLIENT).await, Err(Failed::Later(_))));
}

#[tokio::test]
async fn renewing_gives_another_of_both() {
    let old = Zeroizing::new("rt-old".to_owned());
    let s = service(vec![(200, gave(None))]).await;
    let t = renew(&s.places, CLIENT, &old).await.unwrap();
    assert_eq!((t.access.as_str(), t.refresh.as_str()), ("at-canary-1", "rt-canary-1"));
    let f = &s.forms()[0];
    assert_eq!(
        (f["grant_type"].as_str(), f["refresh_token"].as_str()),
        ("refresh_token", "rt-old")
    );
    assert_eq!((f["client_id"].as_str(), f.contains_key("scope")), (CLIENT, false));
    // Without another, the old one is all there is.
    let mut g = gave(None);
    g.as_object_mut().unwrap().remove("refresh_token");
    let s = service(vec![(200, g)]).await;
    assert_eq!(renew(&s.places, CLIENT, &old).await.unwrap().refresh.as_str(), "rt-old");
    for over in [
        "invalid_grant",
        "invalid_refresh_token",
        "token_expired",
        "refresh_token_expired",
        "refresh_token_invalidated",
        "refresh_token_reused",
    ] {
        for body in [json!({"error": over}), json!({"error": {"code": over}})] {
            let s = service(vec![(400, body)]).await;
            assert_eq!(
                renew(&s.places, CLIENT, &old).await.unwrap_err(),
                Failed::Over(over.into())
            );
        }
    }
    // Nothing of these is a reason to sign anybody out.
    for (status, body) in [
        (503, json!({})),
        (429, json!({"error": "slow_down"})),
        (400, json!({"error": "invalid_client"})),
        (200, json!({"access_token": "", "refresh_token": "x"})),
        (200, json!({"access_token": "a b", "refresh_token": "x"})),
        (200, json!({"access_token": "a", "refresh_token": "x", "token_type": "mac"})),
    ] {
        let s = service(vec![(status, body.clone())]).await;
        let r = renew(&s.places, CLIENT, &old).await;
        assert!(matches!(r, Err(Failed::Later(_))), "{body}: {r:?}");
    }
    let mut g = gave(None);
    g["scope"] = json!("openid");
    let s = service(vec![(200, g)]).await;
    assert_eq!(renew(&s.places, CLIENT, &old).await.unwrap_err(), Failed::NoPlan);
    let e =
        renew(&Places { token: "http://127.0.0.1:9/x".into(), ..Places::default() }, CLIENT, &old)
            .await
            .unwrap_err();
    assert!(matches!(e, Failed::Later(_)) && !e.to_string().contains("rt-old"), "{e}");
}

#[tokio::test]
async fn ending_it_is_certain_or_said_not_to_be() {
    let t = Zeroizing::new("rt-canary".to_owned());
    let s = service(vec![(200, json!("")), (500, json!(""))]).await;
    assert!(revoke(&s.places, CLIENT, &t).await);
    assert!(!revoke(&s.places, CLIENT, &t).await);
    let f = &s.forms()[0];
    assert_eq!(
        (f["token"].as_str(), f["token_type_hint"].as_str(), f["client_id"].as_str()),
        ("rt-canary", "refresh_token", CLIENT)
    );
}

#[test]
fn nothing_of_it_goes_over_a_network_in_the_open() {
    for url in ["http://auth.openai.com/x", "http://10.0.0.1/x", "ftp://a/x"] {
        assert!(Endpoint::parse(url).is_err(), "{url}");
    }
}
