//! A credential and the one origin it may be sent to (SI-3, SI-4; T25, T44).

use std::{fmt, net::IpAddr};

use reqwest::{
    Url,
    header::{HeaderName, HeaderValue},
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Why a connection cannot be set up. Nothing has been sent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SetupError {
    #[error("the address is not a URL of the form https://host/path")]
    BadUrl,
    #[error("the address must not contain a user, a password, a query or a fragment")]
    ExtraInUrl,
    #[error("plain http is only for this machine (localhost, 127.0.0.0/8, ::1)")]
    PlainHttp,
    #[error("the credential is for {made_for} and cannot be used at {used_at}")]
    OtherOrigin { made_for: Origin, used_at: Origin },
    #[error("the credential is empty or has characters a header cannot carry")]
    BadSecret,
    #[error("the header {0:?} cannot be set by a profile")]
    BadHeader(String),
    #[error("a proxy cannot be used for this machine, nor have a user or a password")]
    BadProxy,
    #[error("the HTTP client could not be made: {0}")]
    Http(String),
}

/// Scheme, host and port.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Origin {
    scheme: String,
    host: String,
    port: u16,
}
impl Origin {
    fn of(url: &Url) -> Result<Self, SetupError> {
        let (host, port) =
            url.host_str().zip(url.port_or_known_default()).ok_or(SetupError::BadUrl)?;
        Ok(Self { scheme: url.scheme().to_owned(), host: host.to_owned(), port })
    }
}
impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}://{}:{}", self.scheme, self.host, self.port)
    }
}

/// Where a provider listens: https, or plain http on this machine. It comes from the user's own
/// configuration, never from a workspace (SI-5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    base: Url,
    origin: Origin,
}
impl Endpoint {
    pub fn parse(text: &str) -> Result<Self, SetupError> {
        let mut base = Url::parse(text).map_err(|_| SetupError::BadUrl)?;
        if base.query().is_some()
            || base.fragment().is_some()
            || !base.username().is_empty()
            || base.password().is_some()
        {
            return Err(SetupError::ExtraInUrl);
        }
        let origin = Origin::of(&base)?;
        match base.scheme() {
            "https" => {}
            "http" if here(&base) => {}
            "http" => return Err(SetupError::PlainHttp),
            _ => return Err(SetupError::BadUrl),
        }
        if !base.path().ends_with('/') {
            let path = format!("{}/", base.path());
            base.set_path(&path);
        }
        Ok(Self { base, origin })
    }
    pub fn origin(&self) -> &Origin {
        &self.origin
    }
    /// An address under the base. A path that leads to another origin is none.
    pub(crate) fn at(&self, path: &str) -> Result<Url, SetupError> {
        let url = self.base.join(path).map_err(|_| SetupError::BadUrl)?;
        if Origin::of(&url)? != self.origin {
            return Err(SetupError::BadUrl);
        }
        Ok(url)
    }
    pub(crate) fn is_plain(&self) -> bool {
        self.base.scheme() == "http"
    }
    /// Nothing but an origin.
    pub(crate) fn is_root(&self) -> bool {
        self.base.path() == "/"
    }
    pub(crate) fn host(&self) -> (&str, u16) {
        (&self.origin.host, self.origin.port)
    }
    /// What [`Endpoint::parse`] refuses.
    #[cfg(test)]
    pub(crate) fn unchecked(text: &str) -> Self {
        let base = Url::parse(text).unwrap();
        Self { origin: Origin::of(&base).unwrap(), base }
    }
}

fn here(url: &Url) -> bool {
    let host = url.host_str().unwrap_or("");
    let host = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host);
    // The client pins `localhost` to the loopback addresses, whatever a resolver says.
    host == "localhost" || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// Text that nothing shows, stores or prints. It is wiped when dropped; copies the HTTP client
/// makes of the header it ends up in are not.
pub struct Secret(Zeroizing<String>);
impl Secret {
    pub fn new(text: String) -> Self {
        Self(Zeroizing::new(text))
    }

    /// `text` without it, also as JSON would write it.
    pub(crate) fn strip(&self, text: &str) -> String {
        if self.0.is_empty() {
            return text.to_owned();
        }
        let plain = text.replace(self.0.as_str(), GONE);
        let quoted = Zeroizing::new(serde_json::Value::from(self.0.as_str()).to_string());
        let inner = &quoted[1..quoted.len() - 1];
        if inner == self.0.as_str() { plain } else { plain.replace(inner, GONE) }
    }
}
const GONE: &str = "[secret]";
impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(GONE)
    }
}

/// How a provider takes its credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Auth {
    /// `Authorization: Bearer …`
    Bearer,
    /// A header of its own.
    Header(String),
}

/// A secret that can be attached to requests to one origin and to nothing else.
#[derive(Debug)]
pub struct Credential {
    origin: Origin,
    name: HeaderName,
    secret: Secret,
    auth: Auth,
}
impl Credential {
    pub fn new(origin: &Origin, auth: &Auth, secret: Secret) -> Result<Self, SetupError> {
        let text = &secret.0;
        // Surrounding space would be a different key, and is nearly always a slip of the pointer.
        if text.is_empty() || !text.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(SetupError::BadSecret);
        }
        let name = match auth {
            Auth::Bearer => reqwest::header::AUTHORIZATION,
            Auth::Header(name) => HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| SetupError::BadHeader(name.clone()))?,
        };
        Ok(Self { origin: origin.clone(), name, secret, auth: auth.clone() })
    }

    /// The only way to what is inside: a header, for an address at the credential's own origin.
    pub(crate) fn header(&self, url: &Url) -> Result<(HeaderName, HeaderValue), SetupError> {
        let used_at = Origin::of(url)?;
        if used_at != self.origin {
            return Err(SetupError::OtherOrigin { made_for: self.origin.clone(), used_at });
        }
        let text = match self.auth {
            Auth::Bearer => Zeroizing::new(format!("Bearer {}", *self.secret.0)),
            Auth::Header(_) => Zeroizing::new(self.secret.0.to_string()),
        };
        let mut value = HeaderValue::from_str(&text).map_err(|_| SetupError::BadSecret)?;
        value.set_sensitive(true);
        Ok((self.name.clone(), value))
    }
    pub fn origin(&self) -> &Origin {
        &self.origin
    }
    pub(crate) fn secret(&self) -> &Secret {
        &self.secret
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "sk-canary-0123456789";

    fn origin(url: &str) -> Origin {
        Endpoint::parse(url).unwrap().origin
    }

    #[test]
    fn plain_http_is_for_this_machine() {
        for ok in [
            "https://api.example.com",
            "https://api.example.com:8443/v1",
            "https://10.0.0.1/v1",
            "http://localhost:1234/v1",
            "http://LOCALHOST/v1",
            "http://127.0.0.1:1",
            "http://127.1.2.3",
            "http://2130706433/", // 127.0.0.1
            "http://[::1]:1234/v1",
        ] {
            assert!(Endpoint::parse(ok).is_ok(), "{ok}");
        }
        for bad in [
            "http://api.example.com",
            "http://192.168.1.2:1234/v1",
            "http://0.0.0.0:1234",
            "http://128.0.0.1",
            "http://[::]",
            "http://[::ffff:127.0.0.1]",
            "http://localhost.example.com",
            "http://a.localhost",
            "http://notlocalhost",
            "http://127.0.0.1.example.com",
        ] {
            assert_eq!(Endpoint::parse(bad), Err(SetupError::PlainHttp), "{bad}");
        }
        for bad in ["", "api.example.com", "ftp://localhost/", "file:///etc", "https://", "data:x"]
        {
            assert_eq!(Endpoint::parse(bad), Err(SetupError::BadUrl), "{bad}");
        }
        for bad in [
            "https://u@example.com",
            "https://u:p@example.com",
            "https://:p@example.com",
            "https://example.com/?k=v",
            "https://example.com/?",
            "https://example.com/#f",
        ] {
            assert_eq!(Endpoint::parse(bad), Err(SetupError::ExtraInUrl), "{bad}");
        }
    }

    #[test]
    fn paths_stay_at_the_origin() {
        let e = Endpoint::parse("https://example.com/v1").unwrap();
        assert_eq!(e.at("messages").unwrap().as_str(), "https://example.com/v1/messages");
        let e = Endpoint::parse("https://example.com/v1/").unwrap();
        assert_eq!(e.at("messages").unwrap().as_str(), "https://example.com/v1/messages");
        assert_eq!(Endpoint::parse("https://e.com").unwrap().at("m").unwrap().path(), "/m");
        for bad in ["//evil.example/x", "https://evil.example/", "http://example.com/v1/x"] {
            assert_eq!(e.at(bad), Err(SetupError::BadUrl), "{bad}");
        }
    }

    #[test]
    fn a_credential_is_for_one_origin() {
        let made_for = origin("https://api.example.com/v1");
        assert_eq!(made_for, origin("https://API.example.com:443/other"));
        let c = Credential::new(&made_for, &Auth::Bearer, Secret::new(KEY.into())).unwrap();
        let same = Url::parse("https://api.example.com/v1/messages").unwrap();
        let (name, value) = c.header(&same).unwrap();
        assert_eq!(
            (name.as_str(), value.to_str().unwrap()),
            ("authorization", "Bearer sk-canary-0123456789")
        );
        assert!(value.is_sensitive());
        for other in [
            "http://api.example.com/v1/messages",
            "https://api.example.com:444/v1/messages",
            "https://example.com/v1/messages",
            "https://api.example.com.evil.example/",
        ] {
            let e = c.header(&Url::parse(other).unwrap()).unwrap_err();
            assert!(matches!(e, SetupError::OtherOrigin { .. }), "{other}");
            assert!(!format!("{e} {e:?}").contains("canary"));
        }
        let c =
            Credential::new(&made_for, &Auth::Header("x-api-key".into()), Secret::new(KEY.into()))
                .unwrap();
        let (name, value) = c.header(&same).unwrap();
        assert_eq!((name.as_str(), value.to_str().unwrap()), ("x-api-key", KEY));
        assert!(value.is_sensitive());
    }

    #[test]
    fn what_a_header_cannot_carry_is_refused() {
        let o = origin("https://example.com");
        for bad in ["", "a b", " a", "a\n", "a\r\nx: y", "a\0", "ключ"] {
            let e = Credential::new(&o, &Auth::Bearer, Secret::new(bad.into())).unwrap_err();
            assert_eq!(e, SetupError::BadSecret, "{bad:?}");
        }
        let bad = Auth::Header("x y".into());
        assert!(Credential::new(&o, &bad, Secret::new("k".into())).is_err());
    }

    #[test]
    fn a_secret_shows_nothing() {
        let o = origin("https://example.com");
        let secret = Secret::new(KEY.into());
        assert_eq!(format!("{secret:?} {secret:#?}"), "[secret] [secret]");
        let c = Credential::new(&o, &Auth::Bearer, secret).unwrap();
        assert!(!format!("{c:?} {c:#?}").contains("canary"));
        // And has no other way out. Each line fails to compile if the trait is implemented.
        struct Yes;
        macro_rules! lacks {
            ($name:ident, $($bound:tt)+) => {
                trait $name<A> { fn lacks() {} }
                impl<T: ?Sized> $name<()> for T {}
                impl<T: ?Sized + $($bound)+> $name<Yes> for T {}
                <Secret as $name<_>>::lacks();
                <Credential as $name<_>>::lacks();
            };
        }
        lacks!(NoDisplay, fmt::Display);
        lacks!(NoSerialize, Serialize);
        lacks!(NoClone, Clone);
        lacks!(NoStr, AsRef<str>);
        let wiped: &Zeroizing<String> = &c.secret.0;
        assert_eq!(wiped.len(), KEY.len());
    }

    #[test]
    fn strip_finds_it_plain_and_quoted() {
        let s = Secret::new("a\"b\\c".into());
        assert_eq!(s.strip(r#"x a"b\c y"#), "x [secret] y");
        assert_eq!(s.strip(r#"{"k":"a\"b\\c"}"#), r#"{"k":"[secret]"}"#);
        assert_eq!(Secret::new(KEY.into()).strip(&format!("{KEY}{KEY}.")), "[secret][secret].");
        assert_eq!(Secret::new(String::new()).strip("text"), "text");
    }
}
