//! Capabilities. The only operation is [`Caps::meet`]: what both allow. It is how a child, a
//! harness, a script and a shell get theirs, so nothing can have more than what started it (SI-1).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::paths::Place;

/// Whole trees of the file system. None: nothing (never "no limit").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Reach(Vec<Place>);

impl Reach {
    pub const fn none() -> Self {
        Self(Vec::new())
    }

    pub fn of(places: impl IntoIterator<Item = Place>) -> Self {
        Self(outermost(places, Place::covers))
    }

    pub fn roots(&self) -> &[Place] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn covers(&self, p: &Place) -> bool {
        self.0.iter().any(|r| r.covers(p))
    }

    pub fn within(&self, other: &Self) -> bool {
        self.0.iter().all(|r| other.covers(r))
    }

    pub fn meet(&self, other: &Self) -> Self {
        Self::of(meet(&self.0, &other.0, Place::covers))
    }
}

impl<'de> Deserialize<'de> for Reach {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Vec::deserialize(d).map(Self::of)
    }
}

/// Keeps what no other covers, in order.
fn outermost<T: Ord>(all: impl IntoIterator<Item = T>, covers: fn(&T, &T) -> bool) -> Vec<T> {
    let set: BTreeSet<T> = all.into_iter().collect();
    let mut out: Vec<T> = Vec::new();
    for x in set {
        if out.iter().any(|o| covers(o, &x)) {
            continue;
        }
        out.retain(|o| !covers(&x, o));
        out.push(x);
    }
    out.sort();
    out
}

/// Trees either contain each other or have nothing in common.
fn meet<T: Clone>(a: &[T], b: &[T], covers: fn(&T, &T) -> bool) -> Vec<T> {
    let mut out = Vec::new();
    for x in a {
        for y in b {
            if covers(x, y) {
                out.push(y.clone());
            } else if covers(y, x) {
                out.push(x.clone());
            }
        }
    }
    out
}

/// A host an agent may talk to, or a host and all below it, on one port.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(into = "String")]
pub struct Host {
    name: String,
    below: bool,
    port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadHost {
    #[error("not a host name")]
    Name,
    #[error("an address, not a name")]
    Address,
    #[error("a name of this machine or this network")]
    Local,
    #[error("too wide: at least two labels are needed")]
    Wide,
    #[error("not a port")]
    Port,
}

impl Host {
    /// `crates.io`, `*.crates.io` (and crates.io itself), `crates.io:8443`. Names only, in ASCII (an
    /// international name in its `xn--` form), so what is compared is what is resolved.
    pub fn parse(s: &str) -> Result<Self, BadHost> {
        let (name, port) = match s.rsplit_once(':') {
            Some((n, p)) => {
                let digits = !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
                (n, p.parse().ok().filter(|n| digits && *n != 0).ok_or(BadHost::Port)?)
            }
            None => (s, 443),
        };
        let (name, below) = match name.strip_prefix("*.") {
            Some(n) => (n, true),
            None => (name, false),
        };
        let name = name.strip_suffix('.').unwrap_or(name).to_ascii_lowercase();
        let labels: Vec<&str> = name.split('.').collect();
        let label = |l: &&str| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        };
        if name.len() > 253 || !labels.iter().all(label) {
            return Err(BadHost::Name);
        }
        let last = labels.last().copied().unwrap_or_default();
        // 127.1, 0x7f.1, 2130706433: whatever a resolver may take for an address.
        if last.starts_with(|c: char| c.is_ascii_digit()) {
            return Err(BadHost::Address);
        }
        if matches!(
            last,
            "localhost" | "local" | "internal" | "lan" | "home" | "localdomain" | "arpa" | "onion"
        ) {
            return Err(BadHost::Local);
        }
        if labels.len() < 2 {
            return Err(BadHost::Wide);
        }
        Ok(Self { name, below, port })
    }

    pub fn covers(&self, o: &Host) -> bool {
        self.port == o.port
            && if self.below {
                self.name == o.name
                    || o.name.strip_suffix(&self.name).is_some_and(|head| head.ends_with('.'))
            } else {
                self.name == o.name && !o.below
            }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl From<Host> for String {
    fn from(h: Host) -> Self {
        h.to_string()
    }
}

impl std::fmt::Display for Host {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", if self.below { "*." } else { "" }, self.name)?;
        if self.port != 443 {
            write!(f, ":{}", self.port)?;
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for Host {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::parse(&String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

/// Hosts. None: no network at all.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Net(Vec<Host>);

impl Net {
    pub const fn none() -> Self {
        Self(Vec::new())
    }

    pub fn of(hosts: impl IntoIterator<Item = Host>) -> Self {
        Self(outermost(hosts, Host::covers))
    }

    pub fn hosts(&self) -> &[Host] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn covers(&self, h: &Host) -> bool {
        self.0.iter().any(|x| x.covers(h))
    }

    pub fn within(&self, other: &Self) -> bool {
        self.0.iter().all(|h| other.covers(h))
    }

    pub fn meet(&self, other: &Self) -> Self {
        Self::of(meet(&self.0, &other.0, Host::covers))
    }
}

impl<'de> Deserialize<'de> for Net {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Vec::deserialize(d).map(Self::of)
    }
}

/// Which tools there are for an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tools {
    All,
    Only(BTreeSet<String>),
}

impl Tools {
    pub fn none() -> Self {
        Self::Only(BTreeSet::new())
    }

    pub fn has(&self, tool: &str) -> bool {
        match self {
            Self::All => true,
            Self::Only(s) => s.contains(tool),
        }
    }

    pub fn within(&self, other: &Self) -> bool {
        match (self, other) {
            (_, Self::All) => true,
            (Self::All, Self::Only(_)) => false,
            (Self::Only(a), Self::Only(b)) => a.is_subset(b),
        }
    }

    pub fn meet(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::All, x) | (x, Self::All) => x.clone(),
            (Self::Only(a), Self::Only(b)) => Self::Only(a & b),
        }
    }
}

/// How much (SI-24). `None` is no limit of its own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Budget {
    /// Generations of agents below this one.
    pub depth: Option<u32>,
    /// Agents below this one at a time.
    pub agents: Option<u32>,
    pub tokens: Option<u64>,
    pub millis: Option<u64>,
    /// Millionths of a US dollar.
    pub spend: Option<u64>,
}

fn least<T: Ord + Copy>(a: Option<T>, b: Option<T>) -> Option<T> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

fn under<T: Ord>(a: Option<T>, b: Option<T>) -> bool {
    match (a, b) {
        (_, None) => true,
        (None, Some(_)) => false,
        (Some(a), Some(b)) => a <= b,
    }
}

impl Budget {
    pub fn meet(&self, o: &Self) -> Self {
        Self {
            depth: least(self.depth, o.depth),
            agents: least(self.agents, o.agents),
            tokens: least(self.tokens, o.tokens),
            millis: least(self.millis, o.millis),
            spend: least(self.spend, o.spend),
        }
    }

    pub fn within(&self, o: &Self) -> bool {
        under(self.depth, o.depth)
            && under(self.agents, o.agents)
            && under(self.tokens, o.tokens)
            && under(self.millis, o.millis)
            && under(self.spend, o.spend)
    }
}

/// All an agent may do. The default is nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Caps {
    pub read: Reach,
    pub write: Reach,
    pub net: Net,
    /// Whether it may ask to fetch from hosts beside these, each address anew.
    pub web: bool,
    /// Whether it may start processes (always contained).
    pub run: bool,
    pub tools: Tools,
    pub budget: Budget,
}

impl Default for Caps {
    fn default() -> Self {
        Self {
            read: Reach::none(),
            write: Reach::none(),
            net: Net::none(),
            web: false,
            run: false,
            tools: Tools::none(),
            budget: Budget {
                depth: Some(0),
                agents: Some(0),
                tokens: Some(0),
                millis: Some(0),
                spend: Some(0),
            },
        }
    }
}

impl Caps {
    pub fn meet(&self, o: &Self) -> Self {
        Self {
            read: self.read.meet(&o.read),
            write: self.write.meet(&o.write),
            net: self.net.meet(&o.net),
            web: self.web && o.web,
            run: self.run && o.run,
            tools: self.tools.meet(&o.tools),
            budget: self.budget.meet(&o.budget),
        }
    }

    pub fn within(&self, o: &Self) -> bool {
        self.read.within(&o.read)
            && self.write.within(&o.write)
            && self.net.within(&o.net)
            && (!self.web || o.web)
            && (!self.run || o.run)
            && self.tools.within(&o.tools)
            && self.budget.within(&o.budget)
    }

    /// For a child: what it asks for, as far as its parent has it, one generation further down.
    /// `None`: the parent may have none.
    pub fn child(&self, asks: &Caps) -> Option<Caps> {
        if self.budget.depth == Some(0) || self.budget.agents == Some(0) {
            return None;
        }
        let mut c = self.meet(asks);
        let left = self.budget.depth.map(|d| d.saturating_sub(1));
        c.budget.depth = least(c.budget.depth, left);
        Some(c)
    }
}

#[cfg(all(test, unix))]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;
    use proptest::prelude::*;

    fn place() -> impl Strategy<Value = Place> {
        prop::collection::vec("[ab]", 0..4)
            .prop_map(|v| Place::new(format!("/{}", v.join("/"))).unwrap())
    }
    fn host() -> impl Strategy<Value = Host> {
        (any::<bool>(), prop::collection::vec("[ab]", 1..4), prop::sample::select(vec![443u16, 80]))
            .prop_map(|(w, v, p)| {
                let star = if w { "*." } else { "" };
                Host::parse(&format!("{star}{}.io:{p}", v.join("."))).unwrap()
            })
    }
    fn limit() -> impl Strategy<Value = Option<u64>> {
        prop::option::of(0u64..4)
    }
    fn caps() -> impl Strategy<Value = Caps> {
        (
            prop::collection::vec(place(), 0..4),
            prop::collection::vec(place(), 0..4),
            prop::collection::vec(host(), 0..4),
            any::<bool>(),
            any::<bool>(),
            prop::option::of(prop::collection::btree_set("[a-c]", 0..3)),
            (limit(), limit(), limit(), limit(), limit()),
        )
            .prop_map(|(r, w, n, web, run, t, b)| Caps {
                read: Reach::of(r),
                write: Reach::of(w),
                net: Net::of(n),
                web,
                run,
                tools: t.map_or(Tools::All, Tools::Only),
                budget: Budget {
                    depth: b.0.map(|x| x as u32),
                    agents: b.1.map(|x| x as u32),
                    tokens: b.2,
                    millis: b.3,
                    spend: b.4,
                },
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(600))]

        /// SI-1: a lattice. The meet is the greatest of what is within both.
        #[test]
        fn meet_is_the_greatest_lower_bound(a in caps(), b in caps(), c in caps()) {
            let m = a.meet(&b);
            prop_assert!(m.within(&a) && m.within(&b));
            prop_assert_eq!(&m, &b.meet(&a));
            prop_assert_eq!(&a.meet(&a), &a);
            prop_assert_eq!(a.meet(&b).meet(&c), a.meet(&b.meet(&c)));
            if c.within(&a) && c.within(&b) {
                prop_assert!(c.within(&m));
            }
            prop_assert_eq!(a.within(&b), a.meet(&b) == a);
            if a.within(&b) && b.within(&a) {
                prop_assert_eq!(&a, &b);
            }
            if a.within(&b) && b.within(&c) {
                prop_assert!(a.within(&c));
            }
            prop_assert!(Caps::default().within(&a));
        }

        /// SI-1, SI-24: however a tree of agents is made, nobody has more than any ancestor, and it
        /// ends.
        #[test]
        fn no_descendant_has_more(root in caps(), asks in prop::collection::vec(caps(), 1..6)) {
            let mut line = vec![root];
            for a in &asks {
                let Some(c) = line.last().unwrap().child(a) else { break };
                for up in &line {
                    prop_assert!(c.within(up));
                }
                prop_assert!(c.within(a));
                if let Some(d) = line.last().unwrap().budget.depth {
                    prop_assert!(c.budget.depth.unwrap() < d);
                }
                line.push(c);
            }
        }

        /// The meet of two sets of trees holds exactly the places both hold.
        #[test]
        fn a_reach_holds_what_both_hold(a in prop::collection::vec(place(), 0..4), b in prop::collection::vec(place(), 0..4), x in place()) {
            let (a, b) = (Reach::of(a), Reach::of(b));
            prop_assert_eq!(a.meet(&b).covers(&x), a.covers(&x) && b.covers(&x));
        }

        #[test]
        fn a_net_holds_what_both_hold(a in prop::collection::vec(host(), 0..4), b in prop::collection::vec(host(), 0..4), x in host()) {
            let (a, b) = (Net::of(a), Net::of(b));
            prop_assert_eq!(a.meet(&b).covers(&x), a.covers(&x) && b.covers(&x));
        }

        #[test]
        fn what_is_written_is_read_back(a in caps()) {
            let s = serde_json::to_string(&a).unwrap();
            prop_assert_eq!(serde_json::from_str::<Caps>(&s).unwrap(), a);
        }
    }

    #[test]
    fn nothing_is_the_default_and_means_nothing() {
        let c = Caps::default();
        assert!(!c.read.covers(&Place::new("/").unwrap()));
        assert!(!c.net.covers(&Host::parse("a.io").unwrap()));
        assert!(!c.run && !c.tools.has("read"));
        assert_eq!(c.child(&c), None);
        assert!(serde_json::from_str::<Caps>("{}").is_err(), "a missing part is not a wide one");
    }

    #[test]
    fn hosts_are_names_and_never_everything() {
        for (s, why) in [
            ("*", BadHost::Name),
            ("*.", BadHost::Name),
            ("*.io", BadHost::Wide),
            ("io", BadHost::Wide),
            ("a.*.io", BadHost::Name),
            ("127.0.0.1", BadHost::Address),
            ("0x7f.1", BadHost::Address),
            ("2130706433", BadHost::Address),
            ("[::1]", BadHost::Port),
            ("::1", BadHost::Name),
            ("localhost", BadHost::Local),
            ("a.localhost", BadHost::Local),
            ("metadata.google.internal", BadHost::Local),
            ("printer.local", BadHost::Local),
            ("a.io:0", BadHost::Port),
            ("a.io:", BadHost::Port),
            ("a.io:+80", BadHost::Port),
            ("a.io:99999", BadHost::Port),
            ("a.io/x", BadHost::Name),
            ("u@a.io", BadHost::Name),
            ("a..io", BadHost::Name),
            ("-a.io", BadHost::Name),
            ("bücher.de", BadHost::Name),
            ("", BadHost::Name),
        ] {
            assert_eq!(Host::parse(s), Err(why), "{s}");
        }
        let h = |s| Host::parse(s).unwrap();
        assert_eq!(h("Crates.IO.").to_string(), "crates.io");
        assert!(h("*.crates.io").covers(&h("static.crates.io")));
        assert!(h("*.crates.io").covers(&h("crates.io")));
        assert!(!h("*.crates.io").covers(&h("evilcrates.io")));
        assert!(!h("crates.io").covers(&h("static.crates.io")));
        assert!(!h("crates.io").covers(&h("*.crates.io")));
        assert!(!h("crates.io").covers(&h("crates.io:80")));
    }
}
