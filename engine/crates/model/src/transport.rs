//! HTTP to one provider. One request, one answer: nothing is retried, redirected, cached, proxied
//! (unless the caller names a proxy) or decompressed. HTTP/1.1, so that dropping an answer closes its
//! connection and the provider can stop.

use std::{
    collections::VecDeque,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use reqwest::{
    Response, Url,
    header::{self, HeaderMap, HeaderValue},
    redirect,
};
use serde::Deserialize;

use crate::{
    codec::{self, Decoder, EncodeError, Lost},
    error::{self, Class, Error, Retry},
    profile::Profile,
    secret::{Credential, Endpoint, Secret, SetupError},
    sse,
    types::*,
};

/// Longest event of a stream.
const EVENT: usize = 8 * 1024 * 1024;
/// Most of an error's body that is read.
const ERROR_BODY: usize = 64 * 1024;
/// Longest list of models.
const LIST: usize = 4 * 1024 * 1024;
const MODELS: usize = 10_000;
/// How long a finished answer may take to let go of its connection, which can then be used again.
const DRAIN: Duration = Duration::from_millis(100);

#[derive(Debug, Clone)]
pub struct Options {
    /// Name, connection and TLS.
    pub connect: Duration,
    /// All of it to the answer's head. A model may be loaded or a long prompt read in this time.
    pub first_byte: Duration,
    /// Between one piece of the answer and the next.
    pub idle: Duration,
    /// For https only. The environment's proxies are never used.
    pub proxy: Option<String>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(15),
            first_byte: Duration::from_secs(600),
            idle: Duration::from_secs(300),
            proxy: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CallError {
    /// Nothing was sent.
    #[error(transparent)]
    Encode(#[from] EncodeError),
    #[error(transparent)]
    Provider(#[from] Error),
}

/// One connection: a provider, where it is, and the credential for it.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    endpoint: Endpoint,
    credential: Option<Arc<Credential>>,
    profile: Arc<Profile>,
    headers: HeaderMap,
    options: Options,
}

impl Client {
    pub fn new(
        profile: Profile,
        endpoint: Endpoint,
        credential: Option<Credential>,
        options: Options,
    ) -> Result<Self, SetupError> {
        if let Some(c) = &credential
            && c.origin() != endpoint.origin()
        {
            return Err(SetupError::OtherOrigin {
                made_for: c.origin().clone(),
                used_at: endpoint.origin().clone(),
            });
        }
        let http = build(&endpoint, &options)?;
        let headers = profile.checked_headers()?;
        let (credential, profile) = (credential.map(Arc::new), Arc::new(profile));
        Ok(Self { http, endpoint, credential, profile, headers, options })
    }

    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    /// Sends the request and returns its answer once it has begun. Dropping this future, or the
    /// answer, closes the connection.
    pub async fn stream(&self, request: &Request) -> Result<Answer, CallError> {
        let encoded = codec::encode(request, &self.profile)?;
        let path = match self.profile.codec {
            Codec::Messages => "messages",
            Codec::Chat => "chat/completions",
        };
        let answer = self.post(path, encoded.body).await.map_err(|e| self.sealed(e))?;
        let quirks = self.profile.quirks(&request.model);
        Ok(Answer {
            request_id: request_id(answer.headers(), self.secret()),
            response: Some(answer),
            sse: sse::Decoder::new(EVENT),
            decoder: Decoder::new(self.profile.codec, &quirks),
            scope: codec::scope(request, &self.profile),
            ready: VecDeque::new(),
            failed: None,
            idle: self.options.idle,
            credential: self.credential.clone(),
            lost: encoded.lost,
        })
    }

    /// What the server has. Only Chat Completions servers list it in one answer.
    pub async fn list_models(&self) -> Result<Vec<ModelInfo>, CallError> {
        if self.profile.codec != Codec::Chat {
            return Err(EncodeError::Unsupported("the list of models").into());
        }
        Ok(self.list().await.map_err(|e| self.sealed(e))?)
    }

    async fn list(&self) -> Result<Vec<ModelInfo>, Error> {
        let url = self.endpoint.at("models").map_err(setup)?;
        let request = self.request(self.http.get(url.clone()), &url, "application/json")?;
        let mut answer = self.send(request).await?;
        let body = read(&mut answer, LIST, self.options.idle).await?;
        if body.len() > LIST {
            return Err(Error::protocol(format!("the list of models is larger than {LIST} bytes")));
        }
        #[derive(Deserialize)]
        struct List {
            data: Vec<Box<serde_json::value::RawValue>>,
        }
        #[derive(Deserialize)]
        struct Named {
            id: String,
        }
        // Some servers answer an address they do not have with success and an error.
        let odd = |e: serde_json::Error| match error::is_error(&text(&body)) {
            true => with_head(Error::provider(None, &text(&body)), answer.headers()),
            false => Error::protocol(format!("the list of models cannot be read: {e}")),
        };
        let list: List = serde_json::from_slice(&body).map_err(odd)?;
        if list.data.len() > MODELS {
            return Err(Error::protocol(format!("the server lists more than {MODELS} models")));
        }
        let mut models = Vec::with_capacity(list.data.len());
        for raw in list.data {
            let Named { id } = serde_json::from_str(raw.get()).map_err(odd)?;
            models.push(ModelInfo { id, native: Json::from(&*raw) });
        }
        Ok(models)
    }

    async fn post(&self, path: &str, body: Vec<u8>) -> Result<Response, Error> {
        let url = self.endpoint.at(path).map_err(setup)?;
        let request = self
            .request(self.http.post(url.clone()), &url, "text/event-stream")?
            .header(header::CONTENT_TYPE, "application/json")
            .body(body);
        let mut answer = self.send(request).await?;
        let kind = answer.headers().get(header::CONTENT_TYPE);
        let kind = kind.and_then(|v| v.to_str().ok()).unwrap_or("").trim().to_ascii_lowercase();
        if kind.is_empty() || kind.starts_with("text/event-stream") {
            return Ok(answer);
        }
        // Not a stream although it succeeded. It may still say what went wrong.
        let status = answer.status().as_u16();
        let body = text(&read(&mut answer, ERROR_BODY, self.options.idle).await?);
        Err(if error::is_error(&body) {
            let mut e = Error::provider(None, &body);
            e.status = Some(status);
            with_head(e, answer.headers())
        } else {
            let what = if kind.starts_with("text/html") { "a web page" } else { "something else" };
            Error::protocol(format!("the answer is not a stream but {what}"))
        })
    }

    fn request(
        &self,
        request: reqwest::RequestBuilder,
        url: &Url,
        accept: &'static str,
    ) -> Result<reqwest::RequestBuilder, Error> {
        let mut request = request
            .headers(self.headers.clone())
            .header(header::ACCEPT, HeaderValue::from_static(accept));
        if let Some(credential) = &self.credential {
            let (name, value) = credential.header(url).map_err(setup)?;
            request = request.header(name, value);
        }
        Ok(request)
    }

    /// The answer's head, if it is one of success.
    async fn send(&self, request: reqwest::RequestBuilder) -> Result<Response, Error> {
        let limit = self.options.first_byte;
        let mut answer = match tokio::time::timeout(limit, request.send()).await {
            Err(_) => return Err(timeout(Class::Network, "the provider did not answer", limit)),
            Ok(Err(e)) => return Err(not_sent(&e, &self.options)),
            Ok(Ok(answer)) => answer,
        };
        let status = answer.status();
        if status.is_redirection() {
            return Err(Error {
                status: Some(status.as_u16()),
                ..Error::protocol("the provider answered with a redirect, which is never followed")
            });
        }
        if answer.headers().get(header::CONTENT_ENCODING).is_some_and(|e| e != "identity") {
            return Err(Error::protocol("the answer is compressed, which was not asked for"));
        }
        if status.is_success() {
            return Ok(answer);
        }
        // What can be had of it: the status alone already says what to do.
        let body = read(&mut answer, ERROR_BODY, self.options.idle).await.unwrap_or_default();
        Err(with_head(Error::provider(Some(status.as_u16()), &text(&body)), answer.headers()))
    }

    fn secret(&self) -> Option<&Secret> {
        self.credential.as_ref().map(|c| c.secret())
    }
    fn sealed(&self, e: Error) -> Error {
        e.sealed(self.secret())
    }
}

/// A model a server has, and all it says of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: String,
    pub native: Json,
}

/// An answer on its way.
pub struct Answer {
    response: Option<Response>,
    sse: sse::Decoder,
    decoder: Decoder,
    scope: Scope,
    /// Of the last piece that came.
    ready: VecDeque<Event>,
    failed: Option<Error>,
    idle: Duration,
    credential: Option<Arc<Credential>>,
    request_id: Option<String>,
    lost: Vec<Lost>,
}

impl std::fmt::Debug for Answer {
    /// Without the head: a server can put anything in it, also what it was sent.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Answer").field("scope", &self.scope).finish_non_exhaustive()
    }
}

impl Answer {
    /// What the provider's support would ask for.
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }
    /// What of the history was not sent.
    pub fn lost(&self) -> &[Lost] {
        &self.lost
    }
    /// For an [`crate::Accumulator`].
    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    /// The next event. After an error, or after [`Event::End`], there is none. A call that is
    /// dropped before it returns loses nothing.
    pub async fn next(&mut self) -> Option<Result<Event, Error>> {
        loop {
            if let Some(event) = self.ready.pop_front() {
                return Some(Ok(event));
            }
            if let Some(e) = self.failed.take() {
                self.response = None;
                let secret = self.credential.as_ref().map(|c| c.secret());
                let mut e = e.sealed(secret);
                e.request_id = e.request_id.or_else(|| self.request_id.clone());
                return Some(Err(e));
            }
            let response = self.response.as_mut()?;
            if self.decoder.ended() {
                // All that is left is the end of the body.
                let _ = tokio::time::timeout(DRAIN, response.chunk()).await;
                self.response = None;
                return None;
            }
            let chunk = tokio::time::timeout(self.idle, response.chunk()).await;
            self.take(chunk);
        }
    }

    fn take<B: AsRef<[u8]>, E>(&mut self, chunk: Result<reqwest::Result<Option<B>>, E>) {
        let mut out = Vec::new();
        let done = match chunk {
            Ok(Ok(Some(bytes))) => {
                let mut events = Vec::new();
                let read = self.sse.push(bytes.as_ref(), &mut events);
                events.iter().try_for_each(|e| self.decoder.event(e, &mut out)).and_then(|()| {
                    match read {
                        Err(_) if self.decoder.ended() => Ok(()),
                        read => read.map_err(|e| Error::protocol(e.to_string())),
                    }
                })
            }
            Ok(Ok(None)) => self.decoder.finish(&mut out),
            Ok(Err(e)) => self.decoder.finish(&mut out).map_err(|_| {
                Error::cut_short(format!("the connection was lost: {}", innermost(&e)))
            }),
            Err(_) => self
                .decoder
                .finish(&mut out)
                .map_err(|_| timeout(Class::CutShort, "nothing more came", self.idle)),
        };
        self.failed = done.err();
        self.ready.extend(out);
    }
}

fn build(endpoint: &Endpoint, options: &Options) -> Result<reqwest::Client, SetupError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let tls = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| SetupError::Http(e.to_string()))?
        .dangerous() // The system's own verifier is set by this door; there is no other.
        .with_custom_certificate_verifier(Arc::new(
            rustls_platform_verifier::Verifier::new(provider)
                .map_err(|e| SetupError::Http(e.to_string()))?,
        ))
        .with_no_client_auth();
    let mut builder = reqwest::Client::builder()
        .user_agent(concat!("axiom/", env!("CARGO_PKG_VERSION")))
        .tls_backend_preconfigured(tls)
        .redirect(redirect::Policy::none())
        .retry(reqwest::retry::never())
        .referer(false)
        .http1_only()
        .no_proxy()
        .connect_timeout(options.connect)
        .pool_idle_timeout(Duration::from_secs(30));
    if endpoint.is_plain() {
        // Whatever a resolver says of `localhost`, it is this machine.
        let (host, port) = endpoint.host();
        let here =
            [(IpAddr::V4(Ipv4Addr::LOCALHOST), port), (IpAddr::V6(Ipv6Addr::LOCALHOST), port)];
        builder = builder.resolve_to_addrs(host, &here.map(SocketAddr::from));
    }
    if let Some(proxy) = &options.proxy {
        let url = Url::parse(proxy).map_err(|_| SetupError::BadProxy)?;
        let named = !url.username().is_empty() || url.password().is_some();
        if endpoint.is_plain() || named || !["http", "https"].contains(&url.scheme()) {
            return Err(SetupError::BadProxy);
        }
        let proxy = reqwest::Proxy::https(url).map_err(|_| SetupError::BadProxy)?;
        builder = builder.proxy(proxy);
    }
    builder.build().map_err(|e| SetupError::Http(innermost(&e)))
}

/// At most one byte more than `most`, so that too much can be told from enough.
async fn read(answer: &mut Response, most: usize, idle: Duration) -> Result<Vec<u8>, Error> {
    let mut body = Vec::new();
    while body.len() <= most {
        match tokio::time::timeout(idle, answer.chunk()).await {
            Ok(Ok(Some(bytes))) => {
                let room = most + 1 - body.len();
                body.extend_from_slice(&bytes[..bytes.len().min(room)]);
            }
            Ok(Ok(None)) => break,
            Ok(Err(e)) => {
                let what = format!("the connection was lost: {}", innermost(&e));
                return Err(Error::cut_short(what));
            }
            Err(_) => return Err(timeout(Class::CutShort, "nothing more came", idle)),
        }
    }
    Ok(body)
}

fn text(body: &[u8]) -> String {
    String::from_utf8_lossy(body).into_owned()
}

fn with_head(mut e: Error, head: &HeaderMap) -> Error {
    let get = |name: &str| head.get(name).and_then(|v| v.to_str().ok());
    e.request_id = get("request-id").or(get("x-request-id")).map(str::to_owned).or(e.request_id);
    e.retry_after = get("retry-after").and_then(|v| error::retry_after(v, now()));
    e
}

fn request_id(head: &HeaderMap, secret: Option<&Secret>) -> Option<String> {
    with_head(Error::protocol(""), head).sealed(secret).request_id
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn setup(e: SetupError) -> Error {
    Error::new(Class::BadRequest, Retry::Never, e.to_string())
}

fn timeout(class: Class, what: &str, after: Duration) -> Error {
    Error::new(class, Retry::Restart, format!("{what} for {} s", after.as_secs_f32()))
}

/// The request did not get an answer.
fn not_sent(e: &reqwest::Error, options: &Options) -> Error {
    if e.is_connect() && e.is_timeout() {
        let what = "no connection to the provider";
        return Error { retry: Retry::Safe, ..timeout(Class::Network, what, options.connect) };
    }
    if e.is_connect() {
        let what = format!("no connection to the provider: {}", innermost(e));
        return Error::new(Class::Network, Retry::Safe, what);
    }
    if e.is_builder() {
        return Error::new(Class::BadRequest, Retry::Never, "the request cannot be made");
    }
    let what = format!("the connection was lost before an answer: {}", innermost(e));
    Error::new(Class::Network, Retry::Restart, what)
}

/// What went wrong at the bottom. The layers above add the address and say "error" again.
fn innermost(e: &dyn std::error::Error) -> String {
    let mut e = e;
    while let Some(source) = e.source() {
        e = source;
    }
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::Auth;

    #[tokio::test]
    async fn a_plain_address_is_this_machine_whatever_a_resolver_says() {
        let here = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        // A name no resolver knows (RFC 6761), as an endpoint never has it.
        let url = format!("http://nobody.invalid:{}/v1", here.local_addr().unwrap().port());
        std::thread::spawn(move || here.accept().is_ok());
        let profile = Profile::openai_compatible("p", "");
        let options = Options::default();
        let client = Client::new(profile, Endpoint::unchecked(&url), None, options).unwrap();
        // It was reached, and then said nothing.
        let e = client.list().await.unwrap_err();
        assert_eq!((e.class, e.retry), (Class::Network, Retry::Restart));
    }

    /// None of reqwest's own words reach an error unread, but they are kept clean all the same.
    #[tokio::test]
    async fn the_http_client_does_not_print_the_credential() {
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
        let rude = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let polite = rude.local_addr().unwrap();
        std::thread::spawn(move || while rude.accept().is_ok() {});
        for url in [format!("http://{closed}/v1"), format!("http://{polite}/v1")] {
            let endpoint = Endpoint::parse(&url).unwrap();
            let secret = Secret::new("sk-canary".into());
            let credential = Credential::new(endpoint.origin(), &Auth::Bearer, secret).unwrap();
            let profile = Profile::openai_compatible("p", &url);
            let client =
                Client::new(profile, endpoint, Some(credential), Options::default()).unwrap();
            let url = client.endpoint.at("models").unwrap();
            let request = client.request(client.http.get(url.clone()), &url, "x").unwrap();
            let mut shown = format!("{request:?} {client:?}");
            let request = request.build().unwrap();
            shown += &format!("{request:?}");
            assert!(request.headers()[header::AUTHORIZATION] == "Bearer sk-canary");
            let e = client.http.execute(request).await.unwrap_err();
            shown += &format!("{e} {e:?} {e:#?}");
            let mut source = std::error::Error::source(&e);
            while let Some(s) = source {
                shown += &format!("{s} {s:?}");
                source = s.source();
            }
            assert!(!shown.contains("canary"), "{shown}");
        }
    }
}
