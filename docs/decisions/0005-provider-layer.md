# 0005 Five codecs, transports, and quirk profiles as data
**Decision.** Codec (message shape) × transport (auth, URL, signing) × profile (data). Codecs: Anthropic Messages, OpenAI Responses, OpenAI Chat, Gemini, Bedrock Converse.
Blocks keep the provider's native JSON beside a normalised view, tagged with where it is valid; history is append-only and prefix-stable; a model switch yields an explicit lossy-conversion report.
**Evidence.** `research/providers.md` §5–6: rust-genai, Vercel AI SDK and models.dev agree on these families. Signed or encrypted reasoning must be replayed verbatim (Anthropic, OpenAI, Gemini, Bedrock, OpenRouter, DeepSeek V4).
**Alternatives.** Lowest common denominator (loses reasoning, caching); a gateway dependency such as LiteLLM (Python runtime, flattening); rust-genai/rig as base (audited only superficially: revisit in M2 before writing codec 2).
**Risk.** Facts age in weeks. Profiles seed from models.dev (MIT), carry provenance, and are checked by recorded fixtures.
