//! An agent's browser (decision 0021): a Chromium of the system's, headless, with a profile of
//! Axiom's own, spoken to over the DevTools pipe. Its own network leads nowhere (a proxy that does not
//! exist, names that do not resolve); every request it would make is paused and decided here, and
//! what is let through, the host makes itself ([`proxy`]): only to a server of the agent's own.
//!
//! One page. Its person sees it as pictures and may take it; while they hold it, the agent waits.

#![cfg(unix)]

pub mod proxy;

use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Write},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use axiom_policy::Place;
use axiom_spawn::{Chromium, Env, Running};
use serde_json::{Value, json};
use tokio::sync::{Semaphore, mpsc, oneshot, watch};

pub use proxy::Gate;

/// An answer is waited for this long.
const ANSWER: Duration = Duration::from_secs(30);
/// A page is waited for this long to be loaded.
const LOAD: Duration = Duration::from_secs(15);
/// The agent waits this long for its person to give the page back.
const TURN: Duration = Duration::from_secs(60);
/// Of one message from the browser: a picture of a large page is some megabytes.
const MESSAGE: usize = 64 * 1024 * 1024;
/// Requests decided at once; the rest wait.
const LANES: usize = 16;
/// Addresses that were refused, kept to be shown.
const REFUSED: usize = 20;
const SIZES: std::ops::RangeInclusive<u32> = 200..=4000;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What its person is shown of it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct View {
    /// Grows with each change.
    pub version: u64,
    /// The last picture of the page, JPEG in base64.
    pub frame: Option<Arc<str>>,
    pub width: u32,
    pub height: u32,
    pub url: String,
    pub title: String,
    /// Its person holds it: the agent waits.
    pub person: bool,
    pub dark: bool,
    pub loading: bool,
    /// Addresses the page asked for and was refused, last first.
    pub refused: Vec<String>,
    pub ended: Option<String>,
}

/// What is needed to start one.
pub struct Setup {
    pub chromium: Chromium,
    /// Axiom's own, never the person's: its cookies and storage stay there.
    pub profile: Place,
    /// Where programs are found; its home is the profile.
    pub path: Vec<Place>,
    pub gate: Arc<dyn Gate>,
}

/// A thing on the page that can be pressed or typed into, as the agent is told of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub role: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
}

/// The page as the agent is told of it. All of it was written by others (SI-17).
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub url: String,
    pub title: String,
    pub text: String,
    pub items: Vec<Item>,
}

/// What a person pointed at (W-51).
#[derive(Debug, Clone, PartialEq)]
pub struct Pointed {
    pub selector: String,
    pub text: String,
    /// Of its computed style, what matters for a look: name and value.
    pub styles: Vec<(String, String)>,
    /// Where it is on the page, in CSS pixels.
    pub rect: (f64, f64, f64, f64),
    /// A picture of it, PNG in base64.
    pub picture: Option<String>,
}

/// What a person does on the page while they hold it.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// `kind`: pressed, released, moved; `button`: left, middle, right, none.
    Mouse {
        kind: String,
        x: f64,
        y: f64,
        button: String,
        clicks: u32,
        modifiers: u32,
    },
    Wheel {
        x: f64,
        y: f64,
        dx: f64,
        dy: f64,
    },
    /// `kind`: down, up. As the DOM names them.
    Key {
        kind: String,
        key: String,
        code: String,
        text: String,
        modifiers: u32,
    },
    Text(String),
}

struct Inner {
    to: Mutex<File>,
    next: AtomicU64,
    waiting: Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>,
    page: Mutex<String>,
    target: Mutex<String>,
    view: watch::Sender<View>,
    gate: Arc<dyn Gate>,
    client: reqwest::Client,
    lanes: Semaphore,
    loaded: watch::Sender<u64>,
    /// The end of what it wrote to its errors: why it ended, if it did.
    said: Mutex<Vec<u8>>,
}

pub struct Browser {
    inner: Arc<Inner>,
    running: tokio::sync::Mutex<Option<Running>>,
    /// Its own temporary folder: short, for the name of a socket in it is (Linux: 108 bytes).
    tmp: std::path::PathBuf,
}

impl Drop for Browser {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.tmp);
    }
}

impl std::fmt::Debug for Browser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Browser").finish_non_exhaustive()
    }
}

fn args(profile: &Place, width: u32, height: u32) -> Vec<String> {
    [
        "--headless=new",
        "--remote-debugging-pipe",
        "--no-first-run",
        "--no-default-browser-check",
        // No question about the system's keychain, which would be a window.
        "--use-mock-keychain",
        "--password-store=basic",
        "--disable-background-networking",
        "--disable-sync",
        "--disable-extensions",
        "--disable-component-update",
        "--disable-default-apps",
        "--disable-breakpad",
        "--mute-audio",
        // Its own network leads nowhere: what it gets, the host gave it.
        "--proxy-server=http://proxy.invalid:1",
        "--proxy-bypass-list=<-loopback>",
        "--host-resolver-rules=MAP * ~NOTFOUND",
        "--force-webrtc-ip-handling-policy=disable_non_proxied_udp",
        "--webrtc-ip-handling-policy=disable_non_proxied_udp",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .chain([
        format!("--user-data-dir={}", profile.as_path().display()),
        format!("--window-size={width},{height}"),
    ])
    .collect()
}

/// Reads what the browser says, a message at a time, until it ends.
fn read(mut from: File, inner: Arc<Inner>, events: mpsc::Sender<Value>) {
    let mut buf: Vec<u8> = Vec::new();
    let mut b = vec![0; 1 << 20];
    loop {
        let n = match from.read(&mut b) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        buf.extend_from_slice(&b[..n]);
        while let Some(end) = buf.iter().position(|c| *c == 0) {
            let m: Vec<u8> = buf.drain(..=end).collect();
            let Ok(v) = serde_json::from_slice::<Value>(&m[..m.len() - 1]) else { continue };
            match v.get("id").and_then(Value::as_u64) {
                Some(id) => {
                    if let Some(tx) = lock(&inner.waiting).remove(&id) {
                        let r = match v.get("error") {
                            Some(e) => Err(e["message"].as_str().unwrap_or("failed").to_owned()),
                            None => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
                        };
                        let _ = tx.send(r);
                    }
                }
                None => {
                    if events.blocking_send(v).is_err() {
                        return;
                    }
                }
            }
        }
        if buf.len() > MESSAGE {
            break;
        }
    }
    // What it said last, if anything; it may still be on its way.
    std::thread::sleep(Duration::from_millis(100));
    let why = last_line(&lock(&inner.said));
    inner.view.send_modify(|v| {
        v.ended.get_or_insert_with(|| match why {
            Some(w) => format!("the browser ended: {w}"),
            None => "the browser ended".into(),
        });
        v.version += 1;
    });
    // Who waits is told why.
    lock(&inner.waiting).clear();
}

/// Of what a browser wrote to its errors, kept.
const SAID: usize = 8 * 1024;

/// The last line that says something, plain and short.
fn last_line(b: &[u8]) -> Option<String> {
    let t = String::from_utf8_lossy(b);
    let l = t.lines().map(str::trim).rfind(|l| !l.is_empty())?;
    Some(l.chars().filter(|c| !c.is_control()).take(300).collect())
}

impl Inner {
    async fn call(&self, method: &str, params: Value, page: bool) -> Result<Value, String> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let mut m = json!({"id": id, "method": method, "params": params});
        if page {
            m["sessionId"] = json!(lock(&self.page).clone());
        }
        let (tx, rx) = oneshot::channel();
        lock(&self.waiting).insert(id, tx);
        let mut bytes = m.to_string().into_bytes();
        bytes.push(0);
        let sent = { lock(&self.to).write_all(&bytes) };
        if let Err(e) = sent {
            lock(&self.waiting).remove(&id);
            return Err(format!("the browser cannot be told: {e}"));
        }
        match tokio::time::timeout(ANSWER, rx).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => Err(self
                .view
                .borrow()
                .ended
                .clone()
                .unwrap_or_else(|| "the browser has ended".into())),
            Err(_) => {
                lock(&self.waiting).remove(&id);
                Err(format!("the browser did not answer {method}"))
            }
        }
    }

    async fn on_page(&self, method: &str, params: Value) -> Result<Value, String> {
        self.call(method, params, true).await
    }

    fn changed(&self, f: impl FnOnce(&mut View)) {
        self.view.send_modify(|v| {
            f(v);
            v.version += 1;
        });
    }

    async fn event(self: &Arc<Self>, e: Value) {
        let method = e["method"].as_str().unwrap_or_default();
        let p = &e["params"];
        match method {
            "Fetch.requestPaused" => {
                let me = self.clone();
                let p = p.clone();
                tokio::spawn(async move {
                    let Ok(_lane) = me.lanes.acquire().await else { return };
                    proxy::answer(&me, &p).await;
                });
            }
            "Page.screencastFrame" => {
                let (w, h) = (&p["metadata"]["deviceWidth"], &p["metadata"]["deviceHeight"]);
                let data: Option<Arc<str>> = p["data"].as_str().map(Arc::from);
                let (w, h) = (w.as_f64().unwrap_or(0.0) as u32, h.as_f64().unwrap_or(0.0) as u32);
                self.changed(|v| {
                    v.frame = data;
                    (v.width, v.height) = (w, h);
                });
                let id = p["sessionId"].clone();
                let me = self.clone();
                tokio::spawn(async move {
                    let _ = me.on_page("Page.screencastFrameAck", json!({"sessionId": id})).await;
                });
            }
            "Target.targetInfoChanged" => self.info(&p["targetInfo"]),
            "Page.frameStartedLoading" | "Page.loadEventFired" | "Page.domContentEventFired" => {
                let loading = method == "Page.frameStartedLoading";
                self.changed(|v| v.loading = loading);
                if method == "Page.loadEventFired" {
                    self.loaded.send_modify(|n| *n += 1);
                }
                // Its title is known by now.
                let me = self.clone();
                tokio::spawn(async move {
                    let id = lock(&me.target).clone();
                    let i = me.call("Target.getTargetInfo", json!({"targetId": id}), false).await;
                    if let Ok(i) = i {
                        me.info(&i["targetInfo"]);
                    }
                });
            }
            // Another page, as a popup: there is one page.
            "Target.targetCreated" => {
                let i = &p["targetInfo"];
                let id = i["targetId"].as_str().unwrap_or_default().to_owned();
                if i["type"] == "page" && id != *lock(&self.target) {
                    let me = self.clone();
                    tokio::spawn(async move {
                        let _ = me.call("Target.closeTarget", json!({"targetId": id}), false).await;
                    });
                }
            }
            // A page's question would hold it still: nobody answers yes.
            "Page.javascriptDialogOpening" => {
                let me = self.clone();
                tokio::spawn(async move {
                    let _ =
                        me.on_page("Page.handleJavaScriptDialog", json!({"accept": false})).await;
                });
            }
            "Target.detachedFromTarget" | "Inspector.detached" => {
                self.changed(|v| {
                    v.ended.get_or_insert_with(|| "the page was closed".into());
                });
            }
            _ => {}
        }
    }

    fn info(&self, i: &Value) {
        if i["targetId"].as_str() != Some(lock(&self.target).as_str()) {
            return;
        }
        let (url, title) = (i["url"].as_str(), i["title"].as_str());
        let (url, title) = (url.unwrap_or_default(), title.unwrap_or_default());
        let title: String = title.chars().filter(|c| !c.is_control()).take(200).collect();
        let url: String = url.chars().take(2000).collect();
        let same = {
            let v = self.view.borrow();
            v.url == url && v.title == title
        };
        if !same {
            self.changed(|v| (v.url, v.title) = (url, title));
        }
    }

    fn refused(&self, url: &str) {
        let url: String = url.chars().take(300).collect();
        self.changed(|v| {
            v.refused.retain(|r| *r != url);
            v.refused.insert(0, url);
            v.refused.truncate(REFUSED);
        });
    }
}

/// A folder of this browser's own in the system's temporary folder, which only its person reads.
fn private_tmp() -> Result<std::path::PathBuf, String> {
    use std::os::unix::fs::DirBuilderExt;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let base = std::fs::canonicalize(std::env::temp_dir()).map_err(|e| e.to_string())?;
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let d = base.join(format!("axiom-browser-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::DirBuilder::new().mode(0o700).create(&d).map_err(|e| e.to_string())?;
    Ok(d)
}

fn size(w: u32, h: u32) -> Result<(), String> {
    if SIZES.contains(&w) && SIZES.contains(&h) {
        Ok(())
    } else {
        Err(format!("a page is {} to {} pixels wide and high", SIZES.start(), SIZES.end()))
    }
}

/// The script that says what can be pressed, for the agent; numbered in that order.
const ITEMS: &str = r#"(() => {
  const out = [];
  const all = document.querySelectorAll('a[href], button, input, textarea, select, summary, [role=button], [role=link], [role=checkbox], [role=tab], [role=menuitem], [contenteditable=true], [onclick]');
  for (const e of all) {
    const r = e.getBoundingClientRect();
    if (r.width < 1 || r.height < 1 || r.bottom < 0 || r.right < 0 || r.top > innerHeight || r.left > innerWidth) continue;
    const s = getComputedStyle(e);
    if (s.visibility === 'hidden' || s.display === 'none') continue;
    const name = (e.getAttribute('aria-label') || e.innerText || e.value || e.placeholder || e.title || e.getAttribute('href') || '').trim().replace(/\s+/g, ' ').slice(0, 80);
    out.push([e.getAttribute('role') || e.tagName.toLowerCase() + (e.type ? ':' + e.type : ''), name, r.left + r.width / 2, r.top + r.height / 2]);
    if (out.length >= 200) break;
  }
  return JSON.stringify({text: (document.body ? document.body.innerText : '').slice(0, 20000), items: out});
})()"#;

/// The script that says what is at a point, for a person's draft.
const AT: &str = r#"((x, y) => {
  const e = document.elementFromPoint(x, y);
  if (!e) return null;
  const path = [];
  for (let n = e; n && n.nodeType === 1 && path.length < 6; n = n.parentElement) {
    let p = n.tagName.toLowerCase();
    if (n.id) { path.unshift(p + '#' + CSS.escape(n.id)); break; }
    const c = [...n.classList].slice(0, 2).map(c => '.' + CSS.escape(c)).join('');
    const same = n.parentElement ? [...n.parentElement.children].filter(s => s.tagName === n.tagName) : [];
    path.unshift(p + c + (same.length > 1 ? ':nth-of-type(' + (same.indexOf(n) + 1) + ')' : ''));
  }
  const s = getComputedStyle(e);
  const names = ['color', 'background-color', 'font-family', 'font-size', 'font-weight', 'line-height', 'margin', 'padding', 'border-radius', 'display', 'width', 'height'];
  const r = e.getBoundingClientRect();
  return JSON.stringify({selector: path.join(' > '), text: (e.innerText || e.value || '').trim().slice(0, 500), styles: names.map(n => [n, s.getPropertyValue(n)]), rect: [r.left, r.top, r.width, r.height]});
})"#;

impl Browser {
    /// Starts one on about:blank, `width` by `height`, light or dark.
    pub async fn start(s: Setup, width: u32, height: u32, dark: bool) -> Result<Self, String> {
        size(width, height)?;
        std::fs::create_dir_all(s.profile.as_path()).map_err(|e| e.to_string())?;
        let tmp_dir = private_tmp()?;
        let tmp = Place::new(&tmp_dir).map_err(|e| e.to_string())?;
        let env = Env::new(&s.path, &s.profile, &tmp);
        let p = axiom_spawn::spawn_browser(
            &s.chromium,
            &args(&s.profile, width, height),
            &s.profile,
            &env,
        )
        .map_err(|e| format!("the browser could not be started: {e}"))?;
        let mut p = p;
        let errors = p.running.stderr.take();
        let client = proxy::client()?;
        let inner = Arc::new(Inner {
            to: Mutex::new(p.to),
            next: AtomicU64::new(1),
            waiting: Mutex::default(),
            page: Mutex::default(),
            target: Mutex::default(),
            view: watch::Sender::new(View { dark, ..View::default() }),
            gate: s.gate,
            client,
            lanes: Semaphore::new(LANES),
            loaded: watch::Sender::new(0),
            said: Mutex::default(),
        });
        if let Some(mut e) = errors {
            let i = inner.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncReadExt;
                let mut b = vec![0; 4096];
                while let Ok(n) = e.read(&mut b).await {
                    if n == 0 {
                        break;
                    }
                    let mut said = lock(&i.said);
                    said.extend_from_slice(&b[..n]);
                    let over = said.len().saturating_sub(SAID);
                    said.drain(..over);
                }
            });
        }
        let b = Self {
            inner: inner.clone(),
            running: tokio::sync::Mutex::new(Some(p.running)),
            tmp: tmp_dir,
        };
        let (tx, mut rx) = mpsc::channel::<Value>(1024);
        let r = inner.clone();
        let from = p.from;
        std::thread::spawn(move || read(from, r, tx));
        let e = inner.clone();
        tokio::spawn(async move {
            while let Some(v) = rx.recv().await {
                e.event(v).await;
            }
        });
        match b.begin(width, height, dark).await {
            Ok(()) => Ok(b),
            Err(e) => {
                b.end().await;
                Err(e)
            }
        }
    }

    async fn begin(&self, width: u32, height: u32, dark: bool) -> Result<(), String> {
        let i = &self.inner;
        i.call("Browser.setDownloadBehavior", json!({"behavior": "deny"}), false).await?;
        let t = i.call("Target.createTarget", json!({"url": "about:blank"}), false).await?;
        let target = t["targetId"].as_str().ok_or("no page")?.to_owned();
        *lock(&i.target) = target.clone();
        // Whatever page there was besides is closed; others that come are closed as they come.
        i.call("Target.setDiscoverTargets", json!({"discover": true}), false).await?;
        if let Ok(all) = i.call("Target.getTargets", json!({}), false).await {
            for t in all["targetInfos"].as_array().into_iter().flatten() {
                if t["type"] == "page" && t["targetId"].as_str() != Some(target.as_str()) {
                    let id = t["targetId"].clone();
                    let _ = i.call("Target.closeTarget", json!({"targetId": id}), false).await;
                }
            }
        }
        let a = i
            .call("Target.attachToTarget", json!({"targetId": target, "flatten": true}), false)
            .await?;
        *lock(&i.page) = a["sessionId"].as_str().ok_or("no page")?.to_owned();
        i.on_page(
            "Fetch.enable",
            json!({"patterns": [{"urlPattern": "*", "requestStage": "Request"}]}),
        )
        .await?;
        i.on_page("Page.enable", json!({})).await?;
        self.size(width, height).await?;
        self.dark(dark).await?;
        self.screencast(width, height).await
    }

    async fn screencast(&self, width: u32, height: u32) -> Result<(), String> {
        let i = &self.inner;
        let _ = i.on_page("Page.stopScreencast", json!({})).await;
        i.on_page(
            "Page.startScreencast",
            json!({"format": "jpeg", "quality": 70, "maxWidth": width, "maxHeight": height}),
        )
        .await
        .map(drop)
    }

    pub fn view(&self) -> watch::Receiver<View> {
        self.inner.view.subscribe()
    }

    /// The view once it is past `seen`, or after `wait` as it is.
    pub async fn view_after(&self, seen: u64, wait: Duration) -> View {
        let mut v = self.view();
        let _ = tokio::time::timeout(wait, v.wait_for(|v| v.version > seen)).await;
        v.borrow().clone()
    }

    pub fn ended(&self) -> bool {
        self.inner.view.borrow().ended.is_some()
    }

    pub async fn size(&self, width: u32, height: u32) -> Result<(), String> {
        size(width, height)?;
        let m = json!({"width": width, "height": height, "deviceScaleFactor": 1, "mobile": width < 600});
        self.inner.on_page("Emulation.setDeviceMetricsOverride", m).await?;
        if self.inner.view.borrow().frame.is_some() {
            self.screencast(width, height).await?;
        }
        Ok(())
    }

    pub async fn dark(&self, dark: bool) -> Result<(), String> {
        let value = if dark { "dark" } else { "light" };
        let m = json!({"features": [{"name": "prefers-color-scheme", "value": value}]});
        self.inner.on_page("Emulation.setEmulatedMedia", m).await?;
        self.inner.changed(|v| v.dark = dark);
        Ok(())
    }

    /// Its person takes it, or gives it back.
    pub fn hold(&self, person: bool) {
        self.inner.changed(|v| v.person = person);
    }

    /// Opens `url` and waits until it has loaded. Only what [`proxy::decide`] lets through loads.
    pub async fn open(&self, url: &str) -> Result<(), String> {
        proxy::decide(url, &*self.inner.gate)?;
        let mut loaded = self.inner.loaded.subscribe();
        let before = *loaded.borrow_and_update();
        let r = self.inner.on_page("Page.navigate", json!({"url": url})).await?;
        if let Some(e) = r["errorText"].as_str() {
            return Err(format!("{url} did not load: {e}"));
        }
        let _ = tokio::time::timeout(LOAD, loaded.wait_for(|n| *n > before)).await;
        Ok(())
    }

    pub async fn back(&self) -> Result<(), String> {
        self.eval("history.back()").await.map(drop)
    }

    async fn eval(&self, expression: &str) -> Result<Value, String> {
        let r = self
            .inner
            .on_page("Runtime.evaluate", json!({"expression": expression, "returnByValue": true}))
            .await?;
        if let Some(e) = r.get("exceptionDetails") {
            return Err(format!("the page refused: {}", e["text"].as_str().unwrap_or("error")));
        }
        Ok(r["result"]["value"].clone())
    }

    /// The page, for the agent.
    pub async fn look(&self) -> Result<Look, String> {
        let v = self.eval(ITEMS).await?;
        let v: Value = serde_json::from_str(v.as_str().unwrap_or("{}")).unwrap_or_default();
        let items = v["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|i| {
                Some(Item {
                    role: i[0].as_str()?.to_owned(),
                    name: i[1].as_str()?.to_owned(),
                    x: i[2].as_f64()?,
                    y: i[3].as_f64()?,
                })
            })
            .collect();
        let view = self.inner.view.borrow().clone();
        Ok(Look {
            url: view.url,
            title: view.title,
            text: v["text"].as_str().unwrap_or_default().to_owned(),
            items,
        })
    }

    /// A press at a point of the page, in CSS pixels.
    pub async fn press(&self, x: f64, y: f64) -> Result<(), String> {
        for kind in ["mousePressed", "mouseReleased"] {
            let m = json!({"type": kind, "x": x, "y": y, "button": "left", "clickCount": 1});
            self.inner.on_page("Input.dispatchMouseEvent", m).await?;
        }
        Ok(())
    }

    pub async fn type_text(&self, text: &str) -> Result<(), String> {
        self.inner.on_page("Input.insertText", json!({"text": text})).await.map(drop)
    }

    /// One of the keys that are not text: Enter, Tab, Escape, Backspace, Delete, the arrows, Home,
    /// End, PageUp, PageDown.
    pub async fn key(&self, key: &str) -> Result<(), String> {
        let code = match key {
            "Enter" => 13,
            "Tab" => 9,
            "Escape" => 27,
            "Backspace" => 8,
            "Delete" => 46,
            "ArrowLeft" => 37,
            "ArrowUp" => 38,
            "ArrowRight" => 39,
            "ArrowDown" => 40,
            "Home" => 36,
            "End" => 35,
            "PageUp" => 33,
            "PageDown" => 34,
            _ => return Err(format!("no such key: {key}")),
        };
        let text = if key == "Enter" { "\r" } else { "" };
        for kind in ["keyDown", "keyUp"] {
            let m = json!({"type": kind, "key": key, "code": key, "windowsVirtualKeyCode": code, "text": if kind == "keyDown" { text } else { "" }});
            self.inner.on_page("Input.dispatchKeyEvent", m).await?;
        }
        Ok(())
    }

    pub async fn scroll(&self, dy: f64) -> Result<(), String> {
        let v = self.inner.view.borrow().clone();
        let (x, y) = (f64::from(v.width.max(2)) / 2.0, f64::from(v.height.max(2)) / 2.0);
        let m = json!({"type": "mouseWheel", "x": x, "y": y, "deltaX": 0, "deltaY": dy});
        self.inner.on_page("Input.dispatchMouseEvent", m).await.map(drop)
    }

    /// What a person does, only while they hold it.
    pub async fn input(&self, i: Input) -> Result<(), String> {
        if !self.inner.view.borrow().person {
            return Err("take the page first".into());
        }
        let (method, m) = match i {
            Input::Mouse { kind, x, y, button, clicks, modifiers } => {
                let kind = match kind.as_str() {
                    "pressed" => "mousePressed",
                    "released" => "mouseReleased",
                    "moved" => "mouseMoved",
                    _ => return Err("no such mouse event".into()),
                };
                if !["left", "middle", "right", "none"].contains(&button.as_str()) {
                    return Err("no such button".into());
                }
                let m = json!({"type": kind, "x": x, "y": y, "button": button, "clickCount": clicks.min(3), "modifiers": modifiers & 15});
                ("Input.dispatchMouseEvent", m)
            }
            Input::Wheel { x, y, dx, dy } => (
                "Input.dispatchMouseEvent",
                json!({"type": "mouseWheel", "x": x, "y": y, "deltaX": dx, "deltaY": dy}),
            ),
            Input::Key { kind, key, code, text, modifiers } => {
                let kind = match kind.as_str() {
                    "down" if text.is_empty() => "rawKeyDown",
                    "down" => "keyDown",
                    "up" => "keyUp",
                    _ => return Err("no such key event".into()),
                };
                if key.len() > 40 || code.len() > 40 || text.chars().count() > 4 {
                    return Err("no such key".into());
                }
                let m = json!({"type": kind, "key": key, "code": code, "text": text, "modifiers": modifiers & 15});
                ("Input.dispatchKeyEvent", m)
            }
            Input::Text(t) => {
                if t.len() > 64 * 1024 {
                    return Err("that is too much to type at once".into());
                }
                ("Input.insertText", json!({"text": t}))
            }
        };
        self.inner.on_page(method, m).await.map(drop)
    }

    /// Waits until its person is not holding the page; false if they still are after a while.
    pub async fn agents_turn(&self) -> bool {
        let mut v = self.view();
        tokio::time::timeout(TURN, v.wait_for(|v| !v.person)).await.is_ok_and(|r| r.is_ok())
    }

    /// What is at a point of the page, with a picture of it.
    pub async fn point(&self, x: f64, y: f64) -> Result<Pointed, String> {
        let v = self.eval(&format!("({AT})({x}, {y})")).await?;
        let v: Value =
            serde_json::from_str(v.as_str().ok_or("nothing is there")?).unwrap_or_default();
        let r = &v["rect"];
        let rect = (
            r[0].as_f64().unwrap_or(0.0),
            r[1].as_f64().unwrap_or(0.0),
            r[2].as_f64().unwrap_or(0.0),
            r[3].as_f64().unwrap_or(0.0),
        );
        let picture = if rect.2 >= 1.0 && rect.3 >= 1.0 {
            let clip = json!({"x": rect.0, "y": rect.1, "width": rect.2.min(4000.0), "height": rect.3.min(4000.0), "scale": 1});
            let p = self
                .inner
                .on_page("Page.captureScreenshot", json!({"format": "png", "clip": clip}))
                .await;
            p.ok().and_then(|p| p["data"].as_str().map(str::to_owned))
        } else {
            None
        };
        let styles = v["styles"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| Some((s[0].as_str()?.to_owned(), s[1].as_str()?.to_owned())))
            .collect();
        Ok(Pointed {
            selector: v["selector"].as_str().unwrap_or_default().chars().take(500).collect(),
            text: v["text"].as_str().unwrap_or_default().to_owned(),
            styles,
            rect,
            picture,
        })
    }

    /// It ends, with all it started.
    pub async fn end(&self) {
        let _ = tokio::time::timeout(
            Duration::from_secs(2),
            self.inner.call("Browser.close", json!({}), false),
        )
        .await;
        if let Some(mut r) = self.running.lock().await.take() {
            let _ = r.stop(Duration::from_secs(2)).await;
        }
        let _ = std::fs::remove_dir_all(&self.tmp);
        self.inner.changed(|v| {
            v.ended.get_or_insert_with(|| "the browser was closed".into());
        });
    }
}
