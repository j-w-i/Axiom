//! Signing in through the person's browser. It comes back to a listener on this machine's own
//! address, the only one Axiom ever has on an IP address (decision 0013): while a sign-in waits for
//! a person, at a port of the system's choice, for one request that knows what only OpenAI and the
//! browser were told.

use std::{
    net::Ipv4Addr,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use axiom_model::chatgpt::{self, Attempt, Back};
use axiom_proto::{ErrorCode, Signin, WireError};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{Notify, mpsc, oneshot},
    task::JoinSet,
    time::{sleep, timeout},
};

use crate::connections::{Connections, internal};

/// How long a person has.
const WITHIN: Duration = Duration::from_secs(10 * 60);
/// How long a request may take to be sent, and its answer to be read.
const SOON: Duration = Duration::from_secs(5);
/// Most of a request's head.
const HEAD: usize = 16 * 1024;
/// Most requests at once: browsers open more than they use.
const ROOM: usize = 16;
/// Most sign-ins that wait at once, and how many are remembered.
const WAITING: usize = 4;
const KEPT: usize = 16;

struct One {
    id: String,
    state: Signin,
    cancel: Arc<Notify>,
}

#[derive(Default)]
pub struct Signins(Mutex<Vec<One>>);

impl Signins {
    fn lock(&self) -> MutexGuard<'_, Vec<One>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// An address for the person's browser, and what to ask after it by.
    pub async fn begin(
        self: &Arc<Self>,
        connections: Arc<Connections>,
        again: Option<String>,
    ) -> Result<(String, String), WireError> {
        let known = again.as_deref().map(|c| connections.known(c)).transpose()?;
        let plans = connections.plans.clone();
        let named = tokio::task::spawn_blocking({
            let plans = plans.clone();
            move || plans.installation()
        });
        let host = named.await.map_err(internal)?.map_err(internal)?;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.map_err(internal)?;
        let port = listener.local_addr().map_err(internal)?.port();
        let attempt = Attempt::new(&plans.places, "Axiom", &host, port, known)
            .map_err(|e| internal(e.to_string()))?;
        let (id, url) =
            (axiom_proto::RequestId::new().0.replacen("q_", "n_", 1), attempt.url.clone());
        let cancel = Arc::new(Notify::new());
        {
            let mut all = self.lock();
            let waits = |o: &&One| matches!(o.state, Signin::Waiting | Signin::Finishing);
            if all.iter().filter(waits).count() >= WAITING {
                return Err(WireError::new(ErrorCode::Busy, "other sign-ins are waiting"));
            }
            if all.len() >= KEPT
                && let Some(at) = all.iter().position(|o| !waits(&o))
            {
                all.remove(at);
            }
            let one = One { id: id.clone(), state: Signin::Waiting, cancel: cancel.clone() };
            all.push(one);
        }
        let this = self.clone();
        let name = id.clone();
        tokio::spawn(async move {
            let end = this.run(&name, &connections, again, listener, port, attempt, &cancel).await;
            this.set(&name, end);
        });
        Ok((id, url))
    }

    pub fn state(&self, id: &str) -> Result<Signin, WireError> {
        let all = self.lock();
        let one = all.iter().find(|o| o.id == id);
        one.map(|o| o.state.clone())
            .ok_or_else(|| WireError::new(ErrorCode::NotFound, "no such sign-in"))
    }

    pub fn cancel(&self, id: &str) -> Result<(), WireError> {
        let all = self.lock();
        let one = all.iter().find(|o| o.id == id);
        let one = one.ok_or_else(|| WireError::new(ErrorCode::NotFound, "no such sign-in"))?;
        one.cancel.notify_one();
        Ok(())
    }

    /// Of all of them: the host stops.
    pub fn end(&self) {
        for o in self.lock().iter() {
            o.cancel.notify_one();
        }
    }

    fn set(&self, id: &str, state: Signin) {
        if let Some(o) = self.lock().iter_mut().find(|o| o.id == id) {
            o.state = state;
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn run(
        &self,
        id: &str,
        connections: &Arc<Connections>,
        again: Option<String>,
        listener: TcpListener,
        port: u16,
        attempt: Attempt,
        cancel: &Notify,
    ) -> Signin {
        let failed = |words: &str| Signin::Failed { problem: words.into() };
        let attempt = Arc::new(attempt);
        let (came, mut comes) = mpsc::channel(1);
        let mut requests = JoinSet::new();
        let within = sleep(WITHIN);
        tokio::pin!(within);
        let (back, answer): (Back, oneshot::Sender<String>) = loop {
            tokio::select! {
                s = listener.accept(), if requests.len() < ROOM => match s {
                    Ok((s, _)) => {
                        requests.spawn(request(s, port, attempt.clone(), came.clone()));
                    }
                    // Too many open files: it may pass.
                    Err(_) => sleep(Duration::from_millis(100)).await,
                },
                Some(_) = requests.join_next() => {}
                Some(b) = comes.recv() => break b,
                () = cancel.notified() => return failed("it was cancelled"),
                () = &mut within => return failed("nobody came back from the browser in time"),
            }
        };
        // Nothing listens from here on.
        drop((listener, came, comes));
        let end = match back {
            Back::NotOurs => failed("it came back wrong"),
            Back::Failed(e) => failed(&e.to_string()),
            Back::Code { code, client } => {
                self.set(id, Signin::Finishing);
                match finish(connections, again, &attempt, &code, &client).await {
                    Ok(connection) => Signin::Done { connection },
                    Err(words) => failed(&words),
                }
            }
        };
        // Before whoever still holds a connection open is waited for.
        self.set(id, end.clone());
        let _ = answer.send(match &end {
            Signin::Failed { problem } => format!("You are not signed in: {problem}."),
            _ => "You are signed in. You can close this page and go back to Axiom.".into(),
        });
        let _ = timeout(SOON, async { while requests.join_next().await.is_some() {} }).await;
        end
    }
}

async fn finish(
    connections: &Arc<Connections>,
    again: Option<String>,
    attempt: &Attempt,
    code: &str,
    client: &str,
) -> Result<String, String> {
    let places = &connections.plans.places;
    let signed = attempt.finish(places, code, client).await.map_err(|e| e.to_string())?;
    let refresh = signed.tokens.refresh.clone();
    match connections.clone().signed_in(again, signed).await {
        Ok(id) => Ok(id),
        Err(e) => {
            // What could not be kept is not left behind there.
            chatgpt::revoke(places, client, &refresh).await;
            Err(e.message)
        }
    }
}

type Came = mpsc::Sender<(Back, oneshot::Sender<String>)>;

/// One request. What is not the one that is waited for learns nothing from its answer.
async fn request(mut s: TcpStream, port: u16, attempt: Arc<Attempt>, came: Came) {
    let Ok(Some(head)) = timeout(SOON, head(&mut s)).await else { return };
    let back = target(&head, port).map(|t| attempt.back(t)).filter(|b| *b != Back::NotOurs);
    let page = match back {
        None => None,
        Some(back) => {
            let (answer, page) = oneshot::channel();
            match came.try_send((back, answer)) {
                Ok(()) => page.await.ok(),
                // It has come already.
                Err(_) => None,
            }
        }
    };
    let (status, body) = match page {
        Some(words) => ("200 OK", html(&words)),
        None => ("404 Not Found", String::new()),
    };
    let answer = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
         Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; frame-ancestors 'none'\r\n\
         X-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nCache-Control: no-store\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = timeout(SOON, async {
        s.write_all(answer.as_bytes()).await?;
        s.shutdown().await
    })
    .await;
}

async fn head(s: &mut TcpStream) -> Option<String> {
    let (mut all, mut buf) = (Vec::new(), [0u8; 2048]);
    loop {
        let n = s.read(&mut buf).await.ok().filter(|n| *n > 0)?;
        all.extend_from_slice(&buf[..n]);
        if let Some(end) = all.windows(4).position(|w| w == b"\r\n\r\n") {
            all.truncate(end);
            return String::from_utf8(all).ok();
        }
        if all.len() > HEAD {
            return None;
        }
    }
}

/// What a request is for, if it is one a browser makes to the address it was given. A page whose
/// own name leads here says that name.
fn target(head: &str, port: u16) -> Option<&str> {
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, target, version) = (first.next()?, first.next()?, first.next()?);
    if method != "GET" || version != "HTTP/1.1" || first.next().is_some() {
        return None;
    }
    let mut hosts = lines.filter_map(|l| {
        let (name, value) = l.split_once(':')?;
        name.eq_ignore_ascii_case("host").then(|| value.trim())
    });
    let host = hosts.next()?;
    let ours = host == format!("127.0.0.1:{port}") && hosts.next().is_none();
    (ours && target.starts_with('/')).then_some(target)
}

fn html(words: &str) -> String {
    let words = words.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>Axiom</title>\
         <style>body{{font:16px system-ui,sans-serif;margin:20vh auto;max-width:32em;padding:0 1em;\
         color:#e6e8ea;background:#1b1d1f}}</style><p>{words}</p></html>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_what_a_browser_asks_at_this_address() {
        let ok = "GET /auth/callback?a=b HTTP/1.1\r\nHost: 127.0.0.1:80\r\nAccept: */*";
        assert_eq!(target(ok, 80), Some("/auth/callback?a=b"));
        assert_eq!(target(&ok.replace("Host", "hOST"), 80), Some("/auth/callback?a=b"));
        for bad in [
            ok.replace("GET", "POST"),
            ok.replace("GET", "HEAD"),
            ok.replace("HTTP/1.1", "HTTP/1.0"),
            ok.replace("HTTP/1.1", "HTTP/1.1 x"),
            ok.replace("/auth", "http://evil/auth"),
            ok.replace("127.0.0.1:80", "evil.example:80"),
            ok.replace("127.0.0.1:80", "localhost:80"),
            ok.replace("127.0.0.1:80", "127.0.0.1"),
            ok.replace("127.0.0.1:80", "127.0.0.1:81"),
            ok.replace("Accept: */*", "Host: evil"),
            ok.replace("Host", "X"),
            "GET /".into(),
            String::new(),
        ] {
            assert_eq!(target(&bad, 80), None, "{bad}");
        }
    }

    #[test]
    fn its_page_is_words() {
        let p = html("a <script>&");
        assert!(p.contains("a &lt;script&gt;&amp;") && !p.contains("<script"));
    }
}
