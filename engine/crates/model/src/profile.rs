//! How providers and models differ, as data. A profile states what was read in a provider's
//! documentation or seen on its wire; what is not known is absent, and an absent control is an error
//! when asked for, not a guess. Profiles are the user's or built in, never a workspace's (SI-5).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    secret::{Auth, SetupError},
    types::Codec,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Names the provider in [`crate::Scope`].
    pub provider: String,
    pub codec: Codec,
    /// Where it is, if it is always there; up to and including the version (`…/v1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// How it takes a credential, if it is given one.
    pub auth: Auth,
    /// Sent with every request. Never a secret.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub quirks: Quirks,
    /// By model id. What a model does not say is said by `quirks`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub models: BTreeMap<String, Quirks>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    /// `output_config.effort`
    OutputConfig,
    /// `reasoning_effort`
    ReasoningEffort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaxTokens {
    MaxTokens,
    MaxCompletionTokens,
}

/// Where a model's own native blocks may go back to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Replay {
    SameModel,
    /// The provider itself drops what another of its models cannot read.
    SameProvider,
}

/// Reasoning written into the text between `<think>` and `</think>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Think {
    Off,
    /// Only if the text begins with it: a tag further in is prose or code about tags.
    Leading,
    /// The prompt's template has opened it already.
    Open,
}

/// Everything absent is the codec's own default, which is the plainest form of its protocol.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Quirks {
    /// Tokens the model takes, and gives, at most.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output: Option<u64>,
    /// Where the effort goes; nowhere if absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    /// Whether the prompt can be marked for caching (`cache_control`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_markers: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay: Option<Replay>,
    /// Send each tool's arguments as they are written, unchecked by the provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eager_tool_args: Option<bool>,
    /// Chat: the name of the limit on output. Default `max_tokens`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens_field: Option<MaxTokens>,
    /// Chat: ask for usage (`stream_options`). Default true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_usage: Option<bool>,
    /// Chat: send reasoning, and members this build does not know, back as they came. Default false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_reasoning: Option<bool>,
    /// Chat: reasoning inside the text. Default off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub think: Option<Think>,
    /// Chat: a call without arguments has the empty string for them. Default false: an empty
    /// string may be what is left of something.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty_args: Option<bool>,
    /// More members of the request body. None that the codec writes itself.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub body: BTreeMap<String, Value>,
}

impl Quirks {
    /// These where they say something, `base` elsewhere.
    fn over(&self, base: &Quirks) -> Quirks {
        let mut body = base.body.clone();
        body.extend(self.body.clone());
        Quirks {
            context: self.context.or(base.context),
            max_output: self.max_output.or(base.max_output),
            effort: self.effort.or(base.effort),
            cache_markers: self.cache_markers.or(base.cache_markers),
            replay: self.replay.or(base.replay),
            eager_tool_args: self.eager_tool_args.or(base.eager_tool_args),
            max_tokens_field: self.max_tokens_field.or(base.max_tokens_field),
            stream_usage: self.stream_usage.or(base.stream_usage),
            replay_reasoning: self.replay_reasoning.or(base.replay_reasoning),
            think: self.think.or(base.think),
            empty_args: self.empty_args.or(base.empty_args),
            body,
        }
    }
}

impl Profile {
    pub fn quirks(&self, model: &str) -> Quirks {
        match self.models.get(model) {
            Some(own) => own.over(&self.quirks),
            None => self.quirks.clone(),
        }
    }

    /// The Anthropic API with a key. From platform.claude.com/docs, read 2026-11-25: headers and
    /// version (api/versioning), effort (build-with-claude/effort), markers
    /// (build-with-claude/prompt-caching), and that a thinking block another model wrote is dropped
    /// by the API and is no error (build-with-claude/preserved-thinking).
    pub fn anthropic() -> Self {
        Self {
            provider: "anthropic".into(),
            codec: Codec::Messages,
            base_url: Some("https://api.anthropic.com/v1".into()),
            auth: Auth::Header("x-api-key".into()),
            headers: BTreeMap::from([("anthropic-version".into(), "2026-10-21".into())]),
            quirks: Quirks {
                effort: Some(Effort::OutputConfig),
                cache_markers: Some(true),
                replay: Some(Replay::SameProvider),
                ..Quirks::default()
            },
            models: BTreeMap::new(),
        }
    }

    /// OpenRouter with a key. From openrouter.ai/docs, read 2026-11-25: the limit and the effort (the
    /// reference of chat completions), and that a message goes back as it came, with its reasoning and
    /// the details of it, whose chunks are concatenated in order
    /// (guides/best-practices/reasoning-tokens#preserving-reasoning, #streaming-response).
    pub fn openrouter() -> Self {
        Self {
            quirks: Quirks {
                effort: Some(Effort::ReasoningEffort),
                max_tokens_field: Some(MaxTokens::MaxCompletionTokens),
                replay_reasoning: Some(true),
                ..Quirks::default()
            },
            ..Self::openai_compatible("openrouter", "https://openrouter.ai/api/v1")
        }
    }

    /// Any server that speaks Chat Completions. Nothing beyond the protocol is taken for granted.
    pub fn openai_compatible(provider: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            codec: Codec::Chat,
            base_url: Some(base_url.into()),
            auth: Auth::Bearer,
            headers: BTreeMap::new(),
            quirks: Quirks::default(),
            models: BTreeMap::new(),
        }
    }

    /// LM Studio on this machine, at its usual port, as recorded in `fixtures/`.
    pub fn lm_studio() -> Self {
        Self::openai_compatible("lmstudio", "http://localhost:1234/v1")
    }

    /// The headers a profile may set. Those that carry credentials or frame the message are not
    /// among them.
    pub(crate) fn checked_headers(&self) -> Result<reqwest::header::HeaderMap, SetupError> {
        use reqwest::header::*;
        let reserved = [
            AUTHORIZATION,
            PROXY_AUTHORIZATION,
            COOKIE,
            HOST,
            CONTENT_LENGTH,
            CONTENT_TYPE,
            TRANSFER_ENCODING,
            CONNECTION,
            ACCEPT,
            ACCEPT_ENCODING,
            TE,
            UPGRADE,
            USER_AGENT,
            HeaderName::from_static("x-api-key"),
        ];
        let mut map = HeaderMap::new();
        for (name, value) in &self.headers {
            let bad = || SetupError::BadHeader(name.clone());
            let parsed = HeaderName::from_bytes(name.as_bytes()).map_err(|_| bad())?;
            let own = matches!(&self.auth, Auth::Header(own) if own.eq_ignore_ascii_case(name));
            if own || reserved.contains(&parsed) {
                return Err(bad());
            }
            map.insert(parsed, HeaderValue::from_str(value).map_err(|_| bad())?);
        }
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_says_what_its_provider_does_not() {
        let full = Quirks {
            context: Some(1),
            max_output: Some(2),
            effort: Some(Effort::OutputConfig),
            cache_markers: Some(true),
            replay: Some(Replay::SameModel),
            eager_tool_args: Some(true),
            max_tokens_field: Some(MaxTokens::MaxCompletionTokens),
            stream_usage: Some(false),
            replay_reasoning: Some(true),
            think: Some(Think::Open),
            empty_args: Some(true),
            body: BTreeMap::from([("a".into(), Value::from(1)), ("b".into(), Value::from(1))]),
        };
        let mut p = Profile::openai_compatible("p", "https://e.com/v1");
        p.quirks = full.clone();
        p.models.insert("silent".into(), Quirks::default());
        p.models.insert(
            "m".into(),
            Quirks {
                context: Some(9),
                think: Some(Think::Off),
                body: BTreeMap::from([("b".into(), Value::from(2))]),
                ..Quirks::default()
            },
        );
        assert_eq!(p.quirks("other"), full);
        assert_eq!(p.quirks("silent"), full);
        let m = p.quirks("m");
        assert_eq!((m.context, m.think, m.max_output), (Some(9), Some(Think::Off), Some(2)));
        assert_eq!((&m.body["a"], &m.body["b"]), (&Value::from(1), &Value::from(2)));
        let again: Profile = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(again, p);
        let mut own = full.clone();
        (own.effort, own.replay) = (Some(Effort::ReasoningEffort), Some(Replay::SameProvider));
        p.models.insert("full".into(), own.clone());
        assert_eq!(p.quirks("full"), own);
    }

    #[test]
    fn nothing_unknown_is_claimed() {
        for p in [
            Profile::anthropic(),
            Profile::openrouter(),
            Profile::lm_studio(),
            Profile::openai_compatible("p", "https://e.com"),
        ] {
            assert_eq!((p.quirks.context, p.quirks.max_output), (None, None));
            assert!(p.models.is_empty() && p.quirks.body.is_empty());
            let again: Profile = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
            assert_eq!(again, p);
        }
        assert!(serde_json::from_str::<Quirks>(r#"{"contxt":1}"#).is_err());
    }

    #[test]
    fn a_profile_cannot_carry_a_credential_or_reframe_a_message() {
        let mut p = Profile::openai_compatible("p", "https://e.com");
        p.auth = Auth::Header("X-Own".into());
        for bad in [
            "authorization",
            "Authorization",
            "x-api-key",
            "x-own",
            "cookie",
            "host",
            "content-length",
            "transfer-encoding",
            "proxy-authorization",
            "user-agent",
            "a b",
            "",
        ] {
            p.headers = BTreeMap::from([(bad.to_owned(), "v".to_owned())]);
            assert_eq!(p.checked_headers(), Err(SetupError::BadHeader(bad.into())));
        }
        p.headers = BTreeMap::from([("x-fine".into(), "a\r\nb: c".into())]);
        assert!(p.checked_headers().is_err());
        let set = Profile::anthropic().checked_headers().unwrap();
        assert_eq!(set["anthropic-version"], "2026-10-21");
    }
}
