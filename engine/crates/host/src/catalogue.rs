//! What providers and programs have said of the models they have: the last of each, with when. It is
//! asked for by a person's act (a program is not started for it), and kept so that they can choose
//! without asking again. Nothing depends on it but what is offered, and how large a window is taken to
//! be where nothing else says.

use std::{
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, PoisonError},
};

use axiom_proto::{ModelInfo, ModelList, limits, now_ms};

use crate::paths::private_file;

/// Lists, and models of one.
const LISTS: usize = 100;
const MODELS: usize = 1000;
/// A longer file is not one of these.
const FILE: u64 = 64 * 1024 * 1024;
const WORDS: usize = 16;

pub struct Catalogue {
    file: PathBuf,
    all: Mutex<Vec<ModelList>>,
}

/// Claude Code's own names for models, where it has not said any itself (code.claude.com/docs/en/
/// model-config, read 2026-11-26).
fn documented() -> ModelList {
    let one = |id: &str, name: &str| {
        let efforts: Vec<String> =
            ["low", "medium", "high", "xhigh", "max"].map(str::to_owned).into();
        ModelInfo {
            id: id.into(),
            name: Some(name.into()),
            default_effort: axiom_harness::claude_default_effort(id, &efforts),
            efforts,
            effort: true,
            ..ModelInfo::default()
        }
    };
    ModelList {
        harness: "claude".into(),
        connection: None,
        at: 0,
        models: vec![
            one("fable", "Fable"),
            one("opus", "Opus"),
            one("sonnet", "Sonnet"),
            one("haiku", "Haiku"),
        ],
    }
}

impl Catalogue {
    /// What cannot be read is nothing: it is asked for again.
    pub fn open(file: &Path) -> Self {
        let fits = std::fs::metadata(file).is_ok_and(|m| m.len() <= FILE);
        let all = match fits.then(|| std::fs::read_to_string(file)) {
            Some(Ok(t)) => serde_json::from_str(&t).unwrap_or_default(),
            _ => Vec::new(),
        };
        Self { file: file.into(), all: Mutex::new(all) }
    }

    fn lock(&self) -> MutexGuard<'_, Vec<ModelList>> {
        self.all.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Takes what was said, in place of what had been. If it cannot be kept it is known as long as the
    /// host runs.
    pub fn put(&self, harness: &str, connection: Option<&str>, models: Vec<ModelInfo>) {
        let models = models.into_iter().take(MODELS).map(fit).collect();
        let connection = connection.map(str::to_owned);
        let next = ModelList { harness: harness.into(), connection, at: now_ms(), models };
        let mut all = self.lock();
        all.retain(|l| l.harness != next.harness || l.connection != next.connection);
        all.push(next);
        // The oldest go: lists of accounts and programs that are there no more are not told.
        all.sort_by_key(|l| l.at);
        let over = all.len().saturating_sub(LISTS);
        all.drain(..over);
        self.write(&all);
    }

    pub fn forget(&self, of: impl Fn(&ModelList) -> bool) {
        let mut all = self.lock();
        if all.iter().any(&of) {
            all.retain(|l| !of(l));
            self.write(&all);
        }
    }

    fn write(&self, all: &[ModelList]) {
        if let Ok(text) = serde_json::to_string(all) {
            let _ = private_file(&self.file, &text);
        }
    }

    pub fn list(&self) -> Vec<ModelList> {
        let mut all = self.lock().clone();
        if !all.iter().any(|l| l.harness == "claude") {
            all.push(documented());
        }
        all
    }

    pub fn find(&self, harness: &str, connection: Option<&str>, model: &str) -> Option<ModelInfo> {
        let all = self.lock();
        let l =
            all.iter().find(|l| l.harness == harness && l.connection.as_deref() == connection)?;
        l.models.iter().find(|m| m.id == model).cloned()
    }
}

/// What is known of a model of a connection.
pub fn about(p: &axiom_model::Profile, m: axiom_model::ModelInfo) -> ModelInfo {
    let q = p.quirks(&m.id);
    let efforts = match q.effort {
        None => Vec::new(),
        Some(_) => m.about.efforts.or(q.efforts).unwrap_or_else(|| {
            // Where it can be told, and nobody says what: what most know.
            ["low", "medium", "high"].map(str::to_owned).into()
        }),
    };
    // Only one that can be chosen.
    let default_effort =
        m.about.default_effort.or(q.default_effort).filter(|e| efforts.contains(e));
    ModelInfo {
        id: m.id,
        name: m.name,
        context: m.about.context.or(q.context),
        effort: !efforts.is_empty(),
        efforts,
        default_effort,
        adaptive: q.thinking.is_some() && m.about.adaptive == Some(true),
        speeds: q.speeds.unwrap_or_default(),
    }
}

/// Whether a model is asked to think as much as it needs: always, where its list says it takes that.
/// Where nothing is said, it thinks as its provider has it do.
pub fn adaptive(q: &axiom_model::Quirks, listed: Option<&ModelInfo>) -> bool {
    q.thinking.is_some() && listed.is_some_and(|m| m.adaptive)
}

pub fn fit(mut m: ModelInfo) -> ModelInfo {
    limits::clip(&mut m.id, limits::TITLE);
    if let Some(n) = &mut m.name {
        limits::clip(n, limits::TITLE);
    }
    for words in [&mut m.efforts, &mut m.speeds] {
        words.truncate(WORDS);
        for w in words {
            limits::clip(w, 100);
        }
    }
    // One of them, or not said.
    m.default_effort = m.default_effort.take().filter(|e| m.efforts.contains(e));
    m
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn m(id: &str) -> ModelInfo {
        ModelInfo { id: id.into(), ..ModelInfo::default() }
    }

    #[test]
    fn the_last_of_each_is_kept() {
        let d = tempfile::tempdir().unwrap();
        let file = d.path().join("models.json");
        let c = Catalogue::open(&file);
        // What it documents, until it says something.
        let l = c.list();
        assert_eq!((l.len(), l[0].at, l[0].models.len()), (1, 0, 4));
        c.put("axiom", Some("c1"), vec![m("a"), m("b")]);
        c.put("axiom", Some("c2"), vec![m("a")]);
        c.put("claude", None, vec![m("x")]);
        c.put("axiom", Some("c1"), vec![m("c")]);
        let again = Catalogue::open(&file);
        assert_eq!(again.list(), c.list());
        let names = |c: &Catalogue| -> Vec<(String, Option<String>, usize)> {
            c.list().into_iter().map(|l| (l.harness, l.connection, l.models.len())).collect()
        };
        assert_eq!(names(&again).len(), 3);
        assert!(again.list().iter().all(|l| l.at > 0));
        assert!(again.find("axiom", Some("c1"), "c").is_some());
        assert!(again.find("axiom", Some("c1"), "a").is_none(), "that was before");
        assert!(again.find("axiom", None, "c").is_none());
        again.forget(|l| l.connection.as_deref() == Some("c1"));
        assert_eq!(names(&Catalogue::open(&file)).len(), 2);
        // A file that cannot be read is asked for again, and not an error.
        std::fs::write(&file, "{").unwrap();
        assert_eq!(names(&Catalogue::open(&file)).len(), 1);
    }

    /// What its provider's list says, where the protocol has a place for it.
    #[test]
    fn what_a_model_can_be_told() {
        use axiom_model::Profile;
        let of = |p: &Profile, id: &str, list: axiom_model::About| {
            about(
                p,
                axiom_model::ModelInfo { about: list, ..axiom_model::ModelInfo::named(id.into()) },
            )
        };
        let said = || axiom_model::About {
            context: Some(9),
            efforts: Some(vec!["low".into(), "max".into()]),
            adaptive: Some(true),
            default_effort: None,
        };
        let a = Profile::anthropic();
        let m = of(&a, "claude-opus-5-5", said());
        assert_eq!((m.context, m.effort, m.adaptive), (Some(9), true, true));
        assert_eq!(
            (m.efforts, m.speeds),
            (vec!["low".to_owned(), "max".into()], vec!["fast".to_owned()])
        );
        // One that is in no list: what most take, and nothing that is not written down.
        let m = of(&a, "claude-x", axiom_model::About::default());
        assert_eq!((m.efforts.len(), m.adaptive, m.speeds.len()), (3, false, 0));
        let m = of(&a, "claude-x", axiom_model::About { efforts: Some(vec![]), ..said() });
        assert!(!m.effort);
        // Where the list says nothing: what the model's page says, else what every model there takes.
        let words = |m: &ModelInfo| m.efforts.join(" ");
        let o = Profile::openai();
        let none = axiom_model::About::default;
        assert_eq!(words(&of(&o, "gpt-6.1-sol", none())), "low medium high xhigh max");
        assert_eq!(words(&of(&o, "gpt-6-sol", none())), "none low medium high xhigh max");
        assert_eq!(words(&of(&o, "gpt-5.5", none())), "none low medium high xhigh");
        assert_eq!(words(&of(&o, "gpt-9", none())), "low medium high xhigh");
        assert_eq!(
            words(&of(&Profile::chatgpt(), "gpt-6-luna", none())),
            "none low medium high xhigh max"
        );
        // Which it tries at when told none, where its page says, and only one it can be told.
        let by_default = |p: &Profile, id: &str| {
            let every = ["none", "low", "medium", "high", "xhigh", "max"].map(str::to_owned);
            let all = axiom_model::About { efforts: Some(every.into()), ..none() };
            of(p, id, all).default_effort
        };
        assert_eq!(by_default(&a, "claude-opus-5-5").as_deref(), Some("medium"));
        assert_eq!(by_default(&a, "claude-haiku-5-5").as_deref(), Some("medium"));
        assert_eq!(by_default(&a, "claude-sonnet-5-5").as_deref(), Some("high"));
        assert_eq!(of(&a, "claude-opus-5-5", said()).default_effort, None, "it takes no medium");
        assert_eq!(by_default(&o, "gpt-6.1-sol").as_deref(), Some("medium"));
        assert_eq!(by_default(&o, "gpt-5.4").as_deref(), Some("none"));
        assert_eq!(by_default(&o, "gpt-6.2-sol"), None, "its page does not say");
        let listed = axiom_model::About { default_effort: Some("low".into()), ..said() };
        assert_eq!(of(&a, "claude-opus-5-5", listed).default_effort.as_deref(), Some("low"));
        let b = Profile::bedrock();
        assert_eq!(by_default(&b, "anthropic.claude-opus-5-5").as_deref(), Some("medium"));
        assert_eq!(by_default(&b, "anthropic.claude-sonnet-4-6").as_deref(), Some("high"));
        assert_eq!(
            words(&of(&b, "anthropic.claude-opus-5-5", none())),
            "low medium high xhigh max"
        );
        assert_eq!(words(&of(&b, "anthropic.claude-sonnet-4-6", none())), "low medium high max");
        // Where there is no place for it, it is not offered, whatever is said.
        let m = of(&Profile::openrouter(), "a/b", said());
        assert_eq!((m.efforts.len(), m.adaptive), (2, false));
        let m = of(&Profile::lm_studio(), "a", said());
        assert_eq!((m.effort, m.efforts.len(), m.adaptive), (false, 0, false));
    }

    #[test]
    fn it_thinks_where_it_can() {
        use axiom_model::Profile;
        let (a, o) = (Profile::anthropic(), Profile::openrouter());
        let listed = |adaptive| ModelInfo { id: "m".into(), adaptive, ..ModelInfo::default() };
        assert!(adaptive(&a.quirks("m"), Some(&listed(true))));
        assert!(!adaptive(&a.quirks("m"), Some(&listed(false))));
        assert!(!adaptive(&a.quirks("m"), None), "nothing is said of it");
        assert!(!adaptive(&o.quirks("m"), Some(&listed(true))), "there is no place for it");
    }

    #[test]
    fn it_has_an_end() {
        let d = tempfile::tempdir().unwrap();
        let c = Catalogue::open(&d.path().join("models.json"));
        let long = ModelInfo {
            id: "x".repeat(9000),
            name: Some("y".repeat(9000)),
            efforts: vec!["z".repeat(9000); 100],
            ..ModelInfo::default()
        };
        c.put("axiom", Some("c"), vec![long; MODELS + 1]);
        for n in 0..LISTS + 5 {
            c.put("goose", Some(&format!("{n}")), vec![m("a")]);
        }
        let all = c.list();
        assert_eq!(all.iter().filter(|l| l.harness != "claude").count(), LISTS);
        c.put(
            "axiom",
            Some("c"),
            vec![ModelInfo { efforts: vec!["z".repeat(9000); 100], ..m("a") }],
        );
        let one = c.find("axiom", Some("c"), "a").unwrap();
        assert!(one.efforts.len() == WORDS && one.efforts[0].len() <= 100);
    }
}
