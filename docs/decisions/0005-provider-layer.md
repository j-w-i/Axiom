# 0005 Five codecs, transports, and quirk profiles as data
**Decision.** Codec (message shape) × transport (auth, URL, signing) × profile (data). Codecs: Anthropic Messages, OpenAI Responses, OpenAI Chat, Gemini, Bedrock Converse.
Blocks keep the provider's native JSON beside a normalised view, tagged with where it is valid; history is append-only and prefix-stable; a model switch yields an explicit lossy-conversion report.
**Evidence.** `research/providers.md` §5–6: rust-genai, Vercel AI SDK and models.dev agree on these families. Signed or encrypted reasoning must be replayed verbatim (Anthropic, OpenAI, Gemini, Bedrock, OpenRouter, DeepSeek V4).
**Alternatives.** Lowest common denominator (loses reasoning, caching); a gateway dependency such as LiteLLM (Python runtime, flattening); rust-genai/rig as base (audited only superficially: revisit in M2 before writing codec 2).
**Bedrock, 2026-11-25.** Converse streaming with a Bedrock API key (bearer). Three things were decided against:
- *Listing models.* `ListFoundationModels` and `ListInferenceProfiles` are at `bedrock.{region}.amazonaws.com`, the models at `bedrock-runtime.{region}.amazonaws.com`. A key may be sent to one origin (SI-4); a second one declared in a profile would be an
  exception to an invariant that the security review has not seen (K32), for a list that does not say which models speak Converse and needs a second call to be usable (many models answer only to an inference profile's id). Models are named by hand, as Anthropic's are.
  If the owner wants the list: a second credential, made of the same key for that one host, never derived from an answer.
- *SigV4.* An AWS access key opens whatever its identity may do in the account, a Bedrock key only Bedrock; temporary ones last hours and cannot be renewed without reading what belongs to other programs. It would also need a clock and the region in the transport. Not built, not begun.
- *Any address.* A key goes to `bedrock-runtime[-fips].{region}.amazonaws.com`, or to this machine, and the host refuses another. Endpoints of a VPC work by their private DNS, under the same name; by their own name they do not.
**Risk.** Facts age in weeks. Profiles seed from models.dev (MIT), carry provenance, and are checked by recorded fixtures.
