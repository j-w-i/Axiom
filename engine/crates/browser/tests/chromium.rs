//! With the Chromium that is installed: CI's runners have one, and so must a machine that runs this.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::panic)]

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use axiom_browser::{Browser, Gate, Input, Setup};
use axiom_policy::Place;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Serves(Vec<u16>);
impl Gate for Serves {
    fn serves(&self, port: u16) -> bool {
        self.0.contains(&port)
    }
}

const PAGE: &str = "<html><head><title>Agent's page</title></head><body>\
    <h1 id=t class=big>Hello</h1>\
    <button onclick=\"document.getElementById('t').textContent='Pressed'\">Press me</button>\
    <input id=i placeholder=Name>\
    <img src='https://example.com/a.png'>\
    <script>fetch('http://127.0.0.1:1/secret').catch(()=>{});\
    try { new WebSocket('ws://127.0.0.1:OTHER/') } catch (e) {}</script>\
    </body></html>";

/// An agent's server, as far as the browser can tell: it answers a few pages and keeps what it was
/// asked.
async fn server(asked: Arc<Mutex<Vec<String>>>, other: u16) -> u16 {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = l.accept().await else { return };
            let asked = asked.clone();
            tokio::spawn(async move {
                let mut b = vec![0; 16 * 1024];
                let n = s.read(&mut b).await.unwrap_or(0);
                let head = String::from_utf8_lossy(&b[..n]).into_owned();
                let line = head.lines().next().unwrap_or_default().to_owned();
                asked.lock().unwrap().push(line.clone());
                let page = PAGE.replace("OTHER", &other.to_string());
                let body = if line.starts_with("GET / ") { page.as_str() } else { "nothing" };
                let res = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = s.write_all(res.as_bytes()).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn a_page_of_the_agents_own_server_is_seen_used_and_pointed_at() {
    let home = std::env::var("HOME").ok().and_then(|h| Place::new(h).ok());
    let Some(chromium) = axiom_spawn::chromium(home.as_ref()) else {
        panic!("no Chromium is installed here: Chrome, Chromium, Edge or Brave");
    };
    let d = tempfile::tempdir().unwrap();
    // As deep as Axiom's own folder puts it: longer than a socket's name may be.
    let deep = "Library/Application Support/axiom/browsers/s_1a21a0b0c0c6d183aa0aabbb0d4002a/a_1a21a0b0c0c6d183aa0aabbb0d4002b";
    let profile = Place::new(d.path().canonicalize().unwrap().join(deep)).unwrap();
    let asked = Arc::new(Mutex::new(Vec::new()));
    // What is not the agent's, on this computer too: a WebSocket does not pass through `Fetch`.
    let other = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let reached = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let other_port = other.local_addr().unwrap().port();
    let r = reached.clone();
    tokio::spawn(async move {
        if other.accept().await.is_ok() {
            r.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    });
    let port = server(asked.clone(), other_port).await;
    let path = vec![Place::new("/usr/bin").unwrap(), Place::new("/bin").unwrap()];
    let b = Browser::start(
        Setup { chromium, profile, path, gate: Arc::new(Serves(vec![port])) },
        800,
        600,
        true,
    )
    .await
    .unwrap();

    // Not what is not the agent's.
    assert!(b.open("http://127.0.0.1:1/").await.is_err());
    assert!(b.open("https://example.com/").await.is_err());
    assert!(b.open("file:///etc/passwd").await.is_err());

    b.open(&format!("http://127.0.0.1:{port}/")).await.unwrap();
    let look = b.look().await.unwrap();
    assert!(look.text.contains("Hello"), "{look:?}");
    let button = look.items.iter().find(|i| i.name == "Press me").unwrap();
    assert!(look.items.iter().any(|i| i.role.starts_with("input") && i.name == "Name"));

    // A picture comes, the page's title and address with it.
    let v = b.view_after(0, Duration::from_secs(10)).await;
    let mut v = v;
    for _ in 0..50 {
        if v.frame.is_some() && v.title == "Agent's page" {
            break;
        }
        v = b.view_after(v.version, Duration::from_secs(1)).await;
    }
    assert!(v.frame.is_some(), "{v:?}");
    assert_eq!(v.title, "Agent's page");
    assert!(v.url.ends_with(&format!(":{port}/")));
    assert!(v.dark);

    // What the page asked for elsewhere was refused, and said; the agent's server never saw it.
    let mut refused = v.refused.clone();
    for _ in 0..50 {
        if refused.len() >= 2 {
            break;
        }
        refused = b.view_after(b.view().borrow().version, Duration::from_millis(200)).await.refused;
    }
    assert!(refused.iter().any(|r| r.starts_with("https://example.com/a.png")), "{refused:?}");
    assert!(refused.iter().any(|r| r.starts_with("http://127.0.0.1:1/secret")), "{refused:?}");

    b.press(button.x, button.y).await.unwrap();
    assert!(b.look().await.unwrap().text.contains("Pressed"));

    // Its person's input only while they hold it; the agent waits for it.
    let typed = Input::Text("x".into());
    assert!(b.input(typed.clone()).await.is_err());
    b.hold(true);
    b.input(typed).await.unwrap();
    b.hold(false);
    assert!(b.agents_turn().await);

    // Pointed at: what it is, how it looks, and a picture of it.
    let p = b.point(20.0, 30.0).await.unwrap();
    assert_eq!(p.selector, "h1#t");
    assert_eq!(p.text, "Pressed");
    assert!(p.styles.iter().any(|(n, v)| n == "font-size" && v.ends_with("px")));
    assert!(p.picture.is_some());

    b.size(400, 700).await.unwrap();
    assert!(b.size(10, 10).await.is_err());

    assert!(!reached.load(std::sync::atomic::Ordering::SeqCst), "another's server was reached");
    let asked = asked.lock().unwrap().clone();
    assert!(asked.iter().all(|a| !a.contains("secret")), "{asked:?}");
    b.end().await;
    assert!(b.ended());
}
