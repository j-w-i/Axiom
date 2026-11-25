//! Pages of the web, for reading (SI-13). One request for one address that was judged: no redirect is
//! followed, nothing is sent but the address, and only addresses of the public internet are
//! connected to, as they were resolved for this request.

use std::{
    fmt::Write,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{Arc, OnceLock},
    time::Duration,
};

use axiom_policy::{Host, caps::BadHost};
use reqwest::{Url, dns, header, redirect};

/// Of an address, bytes.
const ADDRESS: usize = 2000;
/// Of an answer, as it comes.
const BODY: usize = 4 * 1024 * 1024;
const CONNECT: Duration = Duration::from_secs(10);
const LONGEST: Duration = Duration::from_secs(30);

/// An address that may be asked about: where it leads is known from itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    url: Url,
    pub host: Host,
}

impl Address {
    pub fn parse(s: &str) -> Result<Self, String> {
        let bad = |why: &str| format!("the address cannot be used: {why}");
        if s.len() > ADDRESS {
            return Err(bad("it is too long"));
        }
        let mut url = Url::parse(s).map_err(|e| bad(&e.to_string()))?;
        if url.scheme() != "https" {
            return Err(bad("only https is fetched"));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(bad("it holds a name or a password"));
        }
        let port = url.port_or_known_default().unwrap_or(443);
        let name = url.domain().ok_or_else(|| bad(&BadHost::Address.to_string()))?;
        let host = Host::parse(&format!("{name}:{port}")).map_err(|e| bad(&e.to_string()))?;
        url.set_fragment(None);
        Ok(Self { url, host })
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.url.as_str())
    }
}

/// Whether an address is one of the public internet. What is not certainly so is not.
pub fn public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => public4(a),
        IpAddr::V6(a) => public6(a),
    }
}

fn public4(a: Ipv4Addr) -> bool {
    let [x, y, z, _] = a.octets();
    !(x == 0
        || x == 10
        || x == 127
        || (x == 100 && (64..128).contains(&y))
        || (x == 169 && y == 254)
        || (x == 172 && (16..32).contains(&y))
        || (x == 192 && y == 168)
        || (x == 192 && y == 0 && (z == 0 || z == 2))
        || (x == 192 && y == 88 && z == 99)
        || (x == 198 && (y == 18 || y == 19))
        || (x == 198 && y == 51 && z == 100)
        || (x == 203 && y == 0 && z == 113)
        // Multicast, reserved, broadcast.
        || x >= 224)
}

fn public6(a: Ipv6Addr) -> bool {
    let s = a.segments();
    // Global unicast, without what is set aside in it or holds an IPv4 address.
    s[0] & 0xe000 == 0x2000
        && !(s[0] == 0x2001 && s[1] < 0x200)
        && !(s[0] == 0x2001 && s[1] == 0xdb8)
        && s[0] != 0x2002
        && !(s[0] == 0x3fff && s[1] < 0x1000)
}

/// Resolves a name, and gives only what may be connected to.
struct Resolver(fn(IpAddr) -> bool);

#[derive(Debug, thiserror::Error)]
#[error("its name leads to no address of the public internet")]
struct NotPublic;

impl dns::Resolve for Resolver {
    fn resolve(&self, name: dns::Name) -> dns::Resolving {
        let may = self.0;
        Box::pin(async move {
            let all = tokio::net::lookup_host((name.as_str(), 0)).await?;
            let all: Vec<SocketAddr> = all.filter(|a| may(a.ip())).take(16).collect();
            if all.is_empty() {
                return Err(Box::new(NotPublic).into());
            }
            Ok(Box::new(all.into_iter()) as dns::Addrs)
        })
    }
}

fn client(may: fn(IpAddr) -> bool) -> Result<reqwest::Client, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = rustls_platform_verifier::Verifier::new(provider.clone());
    let tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .dangerous() // The system's own verifier is set by this door; there is no other.
        .with_custom_certificate_verifier(Arc::new(verifier.map_err(|e| e.to_string())?))
        .with_no_client_auth();
    reqwest::Client::builder()
        .user_agent(concat!("axiom/", env!("CARGO_PKG_VERSION")))
        .tls_backend_preconfigured(tls)
        .redirect(redirect::Policy::none())
        .retry(reqwest::retry::never())
        .referer(false)
        .http1_only()
        .no_proxy()
        .dns_resolver(Arc::new(Resolver(may)))
        .connect_timeout(CONNECT)
        .timeout(LONGEST)
        // Nothing is kept open that another request could use.
        .pool_max_idle_per_host(0)
        .build()
        .map_err(|e| e.to_string())
}

/// What came back: what is told of it, and its text.
#[derive(Debug, PartialEq, Eq)]
pub struct Page {
    pub ok: bool,
    /// About the answer: a status, a size, where it leads.
    pub notes: String,
    pub text: String,
}

pub async fn get(a: &Address) -> Result<Page, String> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    let c = CLIENT.get_or_init(|| client(public)).as_ref().map_err(Clone::clone)?;
    with(c, &a.url).await
}

async fn with(c: &reqwest::Client, url: &Url) -> Result<Page, String> {
    let mut r = c
        .get(url.clone())
        .header(header::ACCEPT, "text/html, text/plain, text/markdown, application/json, */*;q=0.1")
        .header(header::ACCEPT_ENCODING, "identity")
        .send()
        .await
        .map_err(|e| format!("not fetched: {}", innermost(&e)))?;
    let status = r.status();
    let head =
        |n: header::HeaderName| r.headers().get(n).and_then(|v| v.to_str().ok()).map(str::to_owned);
    if status.is_redirection() {
        let to = head(header::LOCATION).and_then(|l| url.join(&l).ok());
        let notes = match to {
            Some(u) if u.as_str().len() <= ADDRESS => {
                format!("({status}: it has moved to {u}; fetch that, if it is wanted)\n")
            }
            _ => format!("({status}: it has moved, but not to an address that can be used)\n"),
        };
        return Ok(Page { ok: false, notes, text: String::new() });
    }
    let kind = head(header::CONTENT_TYPE).unwrap_or_default().to_ascii_lowercase();
    let kind = Kind::of(&kind);
    let encoded = head(header::CONTENT_ENCODING).is_some_and(|e| e != "identity");
    let mut notes = String::new();
    if !status.is_success() {
        let _ = writeln!(notes, "({status})");
    }
    let Some(kind) = kind.filter(|_| !encoded) else {
        let _ = writeln!(notes, "(it is not text)");
        return Ok(Page { ok: false, notes, text: String::new() });
    };
    let mut body = Vec::new();
    loop {
        match r.chunk().await {
            Ok(Some(b)) => {
                body.extend_from_slice(&b[..b.len().min(BODY - body.len())]);
                if body.len() >= BODY {
                    let _ = writeln!(notes, "(only its first {} MiB were read)", BODY >> 20);
                    break;
                }
            }
            Ok(None) => break,
            Err(e) => {
                let _ = writeln!(notes, "(the connection was lost: {})", innermost(&e));
                break;
            }
        }
    }
    if body[..body.len().min(8192)].contains(&0) {
        let _ = writeln!(notes, "(it is not text)");
        return Ok(Page { ok: false, notes, text: String::new() });
    }
    let text = String::from_utf8_lossy(&body);
    let text = match kind {
        Kind::Html => html(&text, url),
        Kind::Plain => text.into_owned(),
    };
    Ok(Page { ok: status.is_success(), notes, text })
}

fn innermost(e: &dyn std::error::Error) -> String {
    let mut e = e;
    while let Some(source) = e.source() {
        e = source;
    }
    e.to_string()
}

#[derive(Debug, PartialEq, Eq)]
enum Kind {
    Html,
    Plain,
}
impl Kind {
    fn of(content_type: &str) -> Option<Self> {
        let t = content_type.split(';').next().unwrap_or_default().trim();
        match t {
            "text/html" | "application/xhtml+xml" => Some(Self::Html),
            // Said nothing: whether it is text is seen.
            "" => Some(Self::Plain),
            _ if t.starts_with("text/") => Some(Self::Plain),
            _ if ["json", "xml", "javascript", "yaml", "toml"].iter().any(|x| t.ends_with(x)) => {
                Some(Self::Plain)
            }
            _ => None,
        }
    }
}

/// What a page says, with its headings, lists and links. Reads whatever it is given.
pub fn html(s: &str, base: &Url) -> String {
    let mut w = Words { out: String::new(), pre: 0, space: false, lines: 0, marked: false };
    let mut links: Vec<Option<String>> = Vec::new();
    let mut rest = s;
    while !rest.is_empty() {
        let Some(i) = rest.find('<') else {
            w.text(&entities(rest));
            break;
        };
        if i > 0 {
            w.text(&entities(&rest[..i]));
        }
        rest = &rest[i..];
        if let Some(r) = rest.strip_prefix("<!--") {
            rest = r.find("-->").map_or("", |i| &r[i + 3..]);
            continue;
        }
        let Some((tag, after)) = tag(rest) else {
            // A lone bracket.
            w.text("<");
            rest = &rest[1..];
            continue;
        };
        rest = after;
        let name = tag.name.as_str();
        match (name, tag.end) {
            // What is not shown.
            (
                "script" | "style" | "noscript" | "template" | "svg" | "iframe" | "object" | "math"
                | "select" | "textarea",
                false,
            ) if !tag.empty => {
                rest = skip(rest, name);
            }
            ("title", false) => {
                w.block(0);
                w.mark("# ");
            }
            ("title", true) => w.block(2),
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", false) => {
                w.block(2);
                let n = usize::from(name.as_bytes()[1] - b'0');
                w.mark(&format!("{} ", "#".repeat(n)));
            }
            ("li", false) => {
                w.block(1);
                w.mark("- ");
            }
            ("td" | "th", true) => w.text(" | "),
            ("br", _) => w.line(),
            ("pre", false) => {
                w.block(2);
                w.mark("```\n");
                w.pre += 1;
            }
            ("pre", true) if w.pre > 0 => {
                w.pre -= 1;
                if !w.out.ends_with('\n') {
                    w.out.push('\n');
                }
                w.out.push_str("```");
                w.block(2);
            }
            ("a", false) => links.push(tag.href.and_then(|h| link(&h, base))),
            ("a", true) => {
                if let Some(Some(l)) = links.pop() {
                    w.text(&format!(" ({l})"));
                }
            }
            ("img", _) => {
                if let Some(a) = tag.alt.filter(|a| !a.trim().is_empty()) {
                    w.text(&format!("[image: {}]", a.trim()));
                }
            }
            (
                "p" | "div" | "section" | "article" | "header" | "footer" | "nav" | "main" | "ul"
                | "ol" | "table" | "blockquote" | "dl" | "figure" | "aside" | "form" | "h1" | "h2"
                | "h3" | "h4" | "h5" | "h6",
                _,
            ) => w.block(2),
            ("tr" | "li" | "dt" | "dd" | "hr" | "option", _) => w.block(1),
            _ => {}
        }
    }
    let mut out = w.out.trim().to_owned();
    out.push('\n');
    out
}

/// Text as it is laid out: spaces are one, lines are few.
struct Words {
    out: String,
    pre: usize,
    space: bool,
    /// Line ends owed before the next word.
    lines: usize,
    /// What stands before a word has been said, and not the word.
    marked: bool,
}

impl Words {
    fn text(&mut self, s: &str) {
        if self.pre > 0 {
            self.owed();
            self.marked = false;
            self.out.push_str(s);
            return;
        }
        for c in s.chars() {
            if c.is_whitespace() {
                self.space = true;
                continue;
            }
            if c.is_control() {
                continue;
            }
            self.owed();
            self.marked = false;
            self.out.push(c);
        }
    }
    fn owed(&mut self) {
        if self.lines > 0 {
            let has = self.out.len() - self.out.trim_end_matches('\n').len();
            if !self.out.is_empty() {
                self.out.push_str(&"\n".repeat(self.lines.saturating_sub(has)));
            }
        } else if self.space
            && !self.out.is_empty()
            && !self.out.ends_with([' ', '\n'])
            && self.pre == 0
        {
            self.out.push(' ');
        }
        self.lines = 0;
        self.space = false;
    }
    fn mark(&mut self, m: &str) {
        self.owed();
        self.out.push_str(m);
        self.marked = true;
    }
    /// What comes next begins a line of its own, `n` line ends away. A mark is not left alone.
    fn block(&mut self, n: usize) {
        if !self.marked {
            self.lines = self.lines.max(n);
            self.space = false;
        }
    }
    fn line(&mut self) {
        if self.pre > 0 {
            self.out.push('\n');
        } else {
            self.block(1);
        }
    }
}

struct Tag {
    name: String,
    end: bool,
    empty: bool,
    href: Option<String>,
    alt: Option<String>,
}

/// A tag at the beginning of `s`, and what follows it.
fn tag(s: &str) -> Option<(Tag, &str)> {
    let mut r = s.strip_prefix('<')?;
    if let Some(x) = r.strip_prefix(['!', '?']) {
        let after = x.find('>').map_or("", |i| &x[i + 1..]);
        let t = Tag { name: String::new(), end: false, empty: true, href: None, alt: None };
        return Some((t, after));
    }
    let end = r.starts_with('/');
    if end {
        r = &r[1..];
    }
    if !r.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    let n = r.find(|c: char| c.is_whitespace() || c == '>' || c == '/').unwrap_or(r.len());
    let mut t = Tag { name: r[..n].to_ascii_lowercase(), end, empty: false, href: None, alt: None };
    r = &r[n..];
    loop {
        r = r.trim_start();
        match r.chars().next() {
            None => return Some((t, "")),
            Some('>') => return Some((t, &r[1..])),
            Some('/') => {
                t.empty = r[1..].starts_with('>');
                r = &r[1..];
                continue;
            }
            _ => {}
        }
        let n = r
            .char_indices()
            .skip(1)
            .find(|(_, c)| c.is_whitespace() || matches!(c, '=' | '>' | '/'))
            .map_or(r.len(), |x| x.0);
        let key = r[..n].to_ascii_lowercase();
        r = r[n..].trim_start();
        let Some(v) = r.strip_prefix('=') else { continue };
        let v = v.trim_start();
        let (value, after) = match v.chars().next() {
            Some(q @ ('"' | '\'')) => match v[1..].find(q) {
                Some(i) => (&v[1..=i], &v[i + 2..]),
                None => (&v[1..], ""),
            },
            _ => {
                let n = v.find(|c: char| c.is_whitespace() || c == '>').unwrap_or(v.len());
                v.split_at(n)
            }
        };
        r = after;
        match key.as_str() {
            "href" => t.href = Some(entities(value)),
            "alt" => t.alt = Some(entities(value)),
            _ => {}
        }
    }
}

/// Behind the end of an element whose inside is not read.
fn skip<'a>(s: &'a str, name: &str) -> &'a str {
    let b = s.as_bytes();
    let mut i = 0;
    while let Some(n) = s[i..].find("</") {
        let at = i + n + 2;
        let end = at + name.len();
        if b.get(at..end).is_some_and(|x| x.eq_ignore_ascii_case(name.as_bytes())) {
            return s[end..].find('>').map_or("", |j| &s[end + j + 1..]);
        }
        i = at;
    }
    ""
}

/// Where a link leads, if it leads somewhere that can be fetched.
fn link(href: &str, base: &Url) -> Option<String> {
    let u = base.join(href.trim()).ok()?;
    let here = u.as_str().split('#').next() == Some(base.as_str());
    (matches!(u.scheme(), "http" | "https") && !here && u.as_str().len() <= 300)
        .then(|| u.to_string())
}

fn entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest.bytes().take(12).position(|b| b == b';');
        let c = end.and_then(|e| entity(&rest[1..e]));
        match (c, end) {
            (Some(c), Some(e)) => {
                out.push_str(c.encode_utf8(&mut [0; 4]));
                rest = &rest[e + 1..];
            }
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(n: &str) -> Option<char> {
    if let Some(d) = n.strip_prefix('#') {
        let n = match d.strip_prefix(['x', 'X']) {
            Some(h) => u32::from_str_radix(h, 16).ok()?,
            None => d.parse().ok()?,
        };
        return char::from_u32(n).filter(|c| !c.is_control() || *c == '\n');
    }
    Some(match n {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        "ndash" => '–',
        "mdash" => '—',
        "hellip" => '…',
        "lsquo" => '‘',
        "rsquo" => '’',
        "ldquo" => '“',
        "rdquo" => '”',
        "bull" => '•',
        "middot" => '·',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "times" => '×',
        "rarr" => '→',
        "larr" => '←',
        "deg" => '°',
        "eacute" => 'é',
        "para" => '¶',
        "sect" => '§',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;
    use proptest::prelude::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn an_address_is_one_that_leads_where_it_says() {
        let a = Address::parse("https://Docs.RS:443/a/../b?q=1#part").unwrap();
        assert_eq!(a.to_string(), "https://docs.rs/b?q=1");
        assert_eq!(a.host, Host::parse("docs.rs").unwrap());
        assert_eq!(Address::parse("https://a.io:8443/").unwrap().host.port(), 8443);
        // What it looks like is what it is.
        assert_eq!(
            Address::parse("https://exämple.com/").unwrap().to_string(),
            "https://xn--exmple-cua.com/"
        );
        for s in [
            "http://a.io/",
            "ftp://a.io/",
            "file:///etc/passwd",
            "data:text/plain,x",
            "https://user@a.io/",
            "https://user:pw@a.io/",
            "https://127.0.0.1/",
            "https://2130706433/",
            "https://0x7f.1/",
            "https://0177.0.0.1/",
            "https://[::1]/",
            "https://[::ffff:127.0.0.1]/",
            "https://localhost/",
            "https://printer.local/",
            "https://metadata.google.internal/",
            "https://intranet/",
            "https://router.lan/",
            "a.io",
            "",
        ] {
            assert!(Address::parse(s).is_err(), "{s}");
        }
        assert!(Address::parse(&format!("https://a.io/{}", "x".repeat(ADDRESS))).is_err());
    }

    #[test]
    fn only_the_public_internet() {
        for s in [
            "0.0.0.0",
            "0.1.2.3",
            "10.0.0.1",
            "100.64.0.1",
            "100.127.255.255",
            "127.0.0.1",
            "127.1.2.3",
            "169.254.169.254",
            "172.16.0.1",
            "172.31.255.255",
            "192.0.0.8",
            "192.0.2.1",
            "192.88.99.1",
            "192.168.1.1",
            "198.18.0.1",
            "198.19.255.255",
            "198.51.100.7",
            "203.0.113.9",
            "224.0.0.1",
            "239.1.1.1",
            "240.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "::ffff:8.8.8.8",
            "::8.8.8.8",
            "64:ff9b::808:808",
            "64:ff9b:1::1",
            "100::1",
            "2001::1",
            "2001:2::1",
            "2001:10::1",
            "2001:db8::1",
            "2002:7f00:1::",
            "3fff::1",
            "5f00::1",
            "fc00::1",
            "fd12::1",
            "fe80::1",
            "fec0::1",
            "ff02::1",
        ] {
            assert!(!public(s.parse().unwrap()), "{s}");
        }
        for s in [
            "1.1.1.1",
            "8.8.8.8",
            "9.255.0.1",
            "11.0.0.1",
            "100.63.0.1",
            "100.128.0.1",
            "172.15.0.1",
            "172.32.0.1",
            "192.0.1.1",
            "192.167.1.1",
            "198.17.0.1",
            "198.20.0.1",
            "223.255.255.255",
            "2606:4700::1111",
            "2a00:1450::1",
            "2001:200::1",
            "2001:4860::1",
            "3fff:1000::1",
        ] {
            assert!(public(s.parse().unwrap()), "{s}");
        }
    }

    #[test]
    fn a_page_is_read_as_it_is_seen() {
        let base = Url::parse("https://a.io/docs/x").unwrap();
        let page = r##"<!DOCTYPE html><html><head><title>The &amp; title</title>
            <style>p { color: red }</style><script>if (a < b) { "</p>" }</script></head>
            <body><!-- <p>not</p> --><nav><ul><li><a href="/a">First</a><li><a href='y?b=1&amp;c=2'>Second</a></ul></nav>
            <h2 class="big">A
               heading</h2><p>Some <b>bold</b>   words,&nbsp;and 1 < 2 &#8212; &#x2014; &unknown; &copy;</p>
            <pre><code>fn main() {
    let x = 1 &lt;&lt; 2;
}</code></pre><p>After.<br>Next line.</p><img src=x alt="A picture"><img src=y>
            <table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></table>
            <a href="javascript:alert(1)">bad</a> <a href="#here">here</a> <a>none</a>
            <TEXTAREA>hidden</textarea><svg><text>hidden</text></svg>end</body></html>"##;
        assert_eq!(
            html(page, &base),
            "# The & title\n\n- First (https://a.io/a)\n- Second (https://a.io/docs/y?b=1&c=2)\n\n\
             ## A heading\n\nSome bold words, and 1 < 2 — — &unknown; ©\n\n```\nfn main() {\n    let x = 1 << 2;\n}\n```\n\n\
             After.\nNext line.\n\n[image: A picture]\n\nA | B |\n1 | 2 |\n\nbad here none end\n"
        );
        assert_eq!(html("", &base), "\n");
        assert_eq!(html("<script>never ends", &base), "\n");
        assert_eq!(html("a <b", &base), "a\n");
        assert_eq!(html("<p>x<a href=", &base), "x\n");
    }

    #[test]
    fn what_is_text() {
        for (t, k) in [
            ("text/html; charset=utf-8", Some(Kind::Html)),
            ("application/xhtml+xml", Some(Kind::Html)),
            ("text/plain", Some(Kind::Plain)),
            ("application/json", Some(Kind::Plain)),
            ("application/ld+json; x", Some(Kind::Plain)),
            ("", Some(Kind::Plain)),
            ("image/png", None),
            ("application/octet-stream", None),
            ("application/pdf", None),
        ] {
            assert_eq!(Kind::of(t), k, "{t}");
        }
    }

    fn anyone(_: IpAddr) -> bool {
        true
    }

    /// Answers one request, and says what it was.
    async fn once(answer: impl Into<Vec<u8>>) -> (Url, tokio::task::JoinHandle<String>) {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = Url::parse(&format!("http://localhost:{}/p?q", l.local_addr().unwrap().port()));
        let answer = answer.into();
        let h = tokio::spawn(async move {
            let (mut s, _) = l.accept().await.unwrap();
            let mut b = vec![0; 8192];
            let mut n = 0;
            while !b[..n].ends_with(b"\r\n\r\n") {
                n += s.read(&mut b[n..]).await.unwrap();
            }
            let _ = s.write_all(&answer).await;
            let _ = s.shutdown().await;
            String::from_utf8_lossy(&b[..n]).to_lowercase()
        });
        (url.unwrap(), h)
    }
    fn answer(head: &str, body: &str) -> String {
        format!("HTTP/1.1 {head}\r\nContent-Length: {}\r\n\r\n{body}", body.len())
    }

    #[tokio::test]
    async fn one_request_with_nothing_of_the_person_in_it() {
        let c = client(anyone).unwrap();
        let a = answer("200 OK\r\nSet-Cookie: a=b\r\nContent-Type: text/html", "<h1>Hi</h1>there");
        let (url, asked) = once(a).await;
        let p = with(&c, &url).await.unwrap();
        assert_eq!(p, Page { ok: true, notes: String::new(), text: "# Hi\n\nthere\n".into() });
        let asked = asked.await.unwrap();
        assert!(asked.starts_with("get /p?q http/1.1\r\n"), "{asked}");
        let mut names: Vec<&str> =
            asked.lines().skip(1).filter_map(|l| Some(l.split_once(':')?.0)).collect();
        names.sort();
        assert_eq!(names, ["accept", "accept-encoding", "host", "user-agent"]);
    }

    #[tokio::test]
    async fn a_redirect_is_told_and_not_followed() {
        let c = client(anyone).unwrap();
        for (to, says) in [
            ("https://evil.io/x", "moved to https://evil.io/x;"),
            ("/next", "/next;"),
            ("http://[", "not to an address"),
        ] {
            let (url, _) = once(answer(&format!("302 Found\r\nLocation: {to}"), "body")).await;
            let p = with(&c, &url).await.unwrap();
            assert!(!p.ok && p.notes.contains(says) && p.text.is_empty(), "{p:?}");
        }
    }

    #[tokio::test]
    async fn what_is_refused_wrong_or_endless() {
        let c = client(anyone).unwrap();
        let (url, _) = once(answer("404 Not Found\r\nContent-Type: text/plain", "gone")).await;
        let p = with(&c, &url).await.unwrap();
        assert_eq!((p.ok, p.notes.as_str(), p.text.as_str()), (false, "(404 Not Found)\n", "gone"));
        for head in [
            "200 OK\r\nContent-Type: image/png",
            "200 OK\r\nContent-Type: text/plain\r\nContent-Encoding: gzip",
        ] {
            let (url, _) = once(answer(head, "x")).await;
            assert_eq!(with(&c, &url).await.unwrap().notes, "(it is not text)\n");
        }
        let (url, _) = once(answer("200 OK\r\nContent-Type: text/plain", "a\0b")).await;
        assert!(!with(&c, &url).await.unwrap().ok);
        let big = "x".repeat(BODY + 10);
        let (url, _) = once(answer("200 OK\r\nContent-Type: text/plain", &big)).await;
        let p = with(&c, &url).await.unwrap();
        assert_eq!(p.text.len(), BODY);
        assert!(p.notes.contains("only its first 4 MiB"));
        let (url, _) = once("HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\ncut").await;
        let p = with(&c, &url).await.unwrap();
        assert!(p.notes.contains("lost") && p.text == "cut", "{p:?}");
        let (url, _) = once("nonsense").await;
        assert!(with(&c, &url).await.is_err());
    }

    /// SI-13: what a name is resolved to decides, at the moment it is connected to.
    #[tokio::test]
    async fn a_name_that_leads_to_this_machine_is_not_followed_there() {
        let c = client(public).unwrap();
        let (url, asked) = once(answer("200 OK", "secret")).await;
        let e = with(&c, &url).await.unwrap_err();
        assert!(e.contains("no address of the public internet"), "{e}");
        assert!(!asked.is_finished(), "nobody connected");
        asked.abort();
    }

    proptest! {
        #[test]
        fn no_page_makes_it_panic(s in "(<(/?[a-z]{1,5}|!--|a href=|img alt='|pre|script)|>|-->|'|\"|&(amp|#x?[0-9a-f]{0,9}|lt);?|é|[ a\n\0]){0,40}") {
            let out = html(&s, &Url::parse("https://a.io/").unwrap());
            prop_assert!(out.len() <= s.len() * 8 + 100);
        }
    }
}
