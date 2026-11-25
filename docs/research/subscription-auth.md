# Subscription-backed access: what Axiom may do

Research date 2026-11-24. Companion: `harness-integration.md`. Not legal advice; vendor wording changes monthly.
Evidence tags: **[D]** vendor page fetched 2026-11-24 · **[S]** source at listed commit · **[F]** vendor-forum reply ·
**[N]** press/blog/issue · **[U]** unverified. Quotes ≤15 words; pages were read through a summarising fetcher, except the
Claude Code legal page (returned in full). No credential store was read, no login or model request made.

**Binding constraints assumed**: no credential extraction, no forged client identity, no unauthorised private endpoint,
no pooling/rotation of accounts. Axiom is open source, runs locally on the user's machine, free.

## 1. Verdicts

| Plan | Verdict | Confidence | Authorised mechanism for Axiom |
|---|---|---|---|
| **ChatGPT** Go/Plus/Pro/Business (open-source local app: all users per help page) | **DIRECT-AUTHORISED** (preview) + delegate to Codex | High that it is authorised; Medium that it is stable | Sign in with ChatGPT (SIWC) "plan usage": public dynamic OAuth client → `api.openai.com/v1/responses`, or Codex app-server fed that token |
| **Claude** Pro/Max/Ultra/Team/Enterprise | **DELEGATE-TO-HARNESS** (user's own unmodified `claude`) | Medium; seek written approval | Spawn user-installed CLI; login only in Anthropic's flow. Direct inference: **prohibited**. Max/Ultra/Team API credits via user's Console key: API-KEY |
| **Google AI Pro/Ultra / Antigravity** | **DELEGATE-TO-HARNESS**, interactive only | Low–Medium | Unmodified `agy acp`, native OAuth, human drives. Never `agy -p`, never OAuth reuse. Gate behind a warning |
| **Gemini Code Assist Standard/Enterprise** | DELEGATE-TO-HARNESS (Gemini CLI `--acp`) | Medium | Gemini CLI own login; consumer plans ended 2026-06-18 |
| **GitHub Copilot** Free/Pro/Pro+/Max/Business/Ent | **DIRECT-AUTHORISED via official SDK** | High | Copilot SDK (Rust) with signed-in user, or Axiom's own GitHub OAuth/GitHub App |
| **Cursor** | **DELEGATE-TO-HARNESS** | High (SDK/ACP), no raw model access | `agent acp` or `@cursor/sdk`; user API key or `Cursor.auth.login()` |
| **SuperGrok / X Premium(+)** | DELEGATE (Grok Build ACP) ; direct = UNKNOWN | High / Low | `grok agent stdio`; direct OAuth exists for named partners only (OpenCode, Kilo) |
| **Z.ai GLM Coding Plan** | **API-KEY-ONLY for direct (pay-as-you-go)**; plan key only inside listed tools | High | Plan via Claude Code/OpenCode/etc. as the harness; get Axiom listed before any direct use |
| **Kimi (Moonshot) membership** | DELEGATE; plan API key in third-party agents tolerated with genuine UA | Medium | `kimi acp`; or user-supplied Kimi Code key with honest `User-Agent` |
| **MiniMax M/Token Plan** | **DIRECT-AUTHORISED (user-supplied plan key)** | Medium–High | Any OpenAI/Anthropic-compatible tool; interactive, individual |
| **Alibaba/Qwen Coding Plan** | Plan key in *interactive* coding tools; not scripts/backends | Medium | Qwen Code harness, or user key for interactive sessions only |
| **Mistral Vibe / Le Chat Pro** | DELEGATE (`mistral-vibe` ACP); key reuse UNKNOWN | Low | Vibe CLI browser sign-in; else La Plateforme API key |
| **Amp, Factory Droid, Kiro, Junie, Devin, Augment, Qoder** | DELEGATE-TO-HARNESS | Medium | Vendor CLI's own login / API key |
| **Meta Muse, DeepSeek, OpenRouter, others** | UNKNOWN / API-KEY | Low | API key |

**Default product rule**: three lanes, visibly distinct in UI — (A) *API key* (always allowed), (B) *Authorised plan sign-in*
(ChatGPT SIWC, Copilot), (C) *Use your installed agent* (Claude, Antigravity, Cursor, Grok, Kimi, …: Axiom never sees a token).

## 2. Anthropic / Claude

**Auth mechanisms** [D authentication]: claude.ai OAuth via `claude` `/login` or `claude auth login`; `claude setup-token`
(one-year token for `CLAUDE_CODE_OAUTH_TOKEN`, inference only, plan required); Console login; API key; `apiKeyHelper`;
Bedrock/Vertex/Foundry; profiles/WIF. Precedence: cloud → `ANTHROPIC_AUTH_TOKEN` → `ANTHROPIC_API_KEY` (in `-p` "always
used when present") → `apiKeyHelper` → `CLAUDE_CODE_OAUTH_TOKEN` → profile → `/login`. Storage: macOS Keychain, else
0600 file; per `CLAUDE_CONFIG_DIR`. Login has a hard end date: `claude auth status --text` → `Sign-in ends: …` (≥2.1.318);
"Using Claude Code doesn't extend the login."

**Policy (current wording)**
- Legal & compliance page [D, code.claude.com/docs/en/legal-and-compliance]: Anthropic "does not permit third-party
  developers to offer Claude.ai login into their own applications", "or to route requests through Free, Pro, Max, or Ultra
  plan credentials"; developers may not "collect, store, or intermediate Claude.ai credentials or session tokens".
- Same page: nothing prevents "an end user from signing in to the unmodified Claude Code binary" with their subscription,
  "including where a platform hosts Claude Code". Conditions for products that preinstall/run it: binary "must not be
  modified"; don't disable any auth method; no paying/reselling/intermediating usage; Commercial Terms; name/logo limits.
- Agent SDK overview [D]: "Unless previously approved", no "claude.ai login or rate limits for their products", including SDK agents.
- Support 15036540, update 2026-10-07 [D]: signed in with a plan, Agent SDK and `claude -p` "still draw from your subscription
  limits"; 2026-06-15: separate-credit plan paused, "nothing has changed", third-party apps still count against limits.
- Support 13189465 [D]: may "at its discretion allow paid subscribers who have enabled usage credits" to use certain tools, and
  may bill them to credits; tools that "misrepresent their identity" or "route third-party traffic against subscription limits" prohibited.
- Usage Policy effective 2026-11-12 [D anthropic.com/legal/aup]: no "services that route requests through consumer subscriptions
  or misrepresent the product or client".
- Max/Ultra/Team monthly **API credits** [D/N 17154008]: e.g. $100 (Max 5x), $200 (Max 20x), Team $20/$100 per seat; via a linked
  Console org and API key; cover API, Managed Agents, Agent SDK; not Claude Code subscription use. Linking is effectively permanent.

**Reading**: direct inference with plan tokens = forbidden, unambiguous. Delegation = the unmodified-binary carve-out + support
article say users may; "offer … rate limits for your product" and AUP language cut the other way for orchestrators that market
it. A local, free, open-source launcher of the user's own CLI is the safest position available, still grey. **Verdict
DELEGATE-TO-HARNESS, medium.** Ask Anthropic (sales contact on the legal page) before launch; keep an off switch.

**Rules for Axiom**
1. Locate the user-installed `claude`; do not bundle, patch or re-sign it; do not hide its auth methods.
2. Login = run `claude auth login` in an Axiom terminal tab; success read from `claude auth status`. Never ask for, accept, or
   read `CLAUDE_CODE_OAUTH_TOKEN`, Keychain, `.credentials.json`. Do not support "paste your setup-token".
3. Do not set `CLAUDE_CODE_ENTRYPOINT`. It is undocumented, changes metering and has broken API-key billing (T3 #29393, 2026-11-02);
   relabelling it to change metering invites the AUP misrepresentation / Consumer Terms circumvention clauses. Do set the documented
   `CLAUDE_AGENT_SDK_CLIENT_APP=axiom/<ver>`.
4. Warn users: `-p`/SDK traffic has been measured draining the 5-hour window ~1.3–1.8× faster than the TUI
   (claude-code #97074, 2026-09-25; #107106, 2026-11-04; **no Anthropic reply found**) [N,U].
5. Scrub inherited `ANTHROPIC_API_KEY`/`AUTH_TOKEN` from the child env when the user chose "subscription" (it silently wins in `-p`).
6. No multi-account rotation. A separate profile via `CLAUDE_CONFIG_DIR` is documented and fine: one folder per account, each with its own login; on macOS the Keychain entry is keyed to the folder too (code.claude.com/docs/en/authentication, read 2026-11-25). Two *Console* sign-ins without a key are not kept apart by it. `CODEX_HOME`: where `auth.json` is; nothing found on whether keyring entries depend on it.
7. Say "runs your installed Claude Code" in plain text; agent label "Claude"/"Claude Agent"; no Anthropic logos.

**Usage reporting through supported interfaces**: stream-json `rate_limit_event` (`status, resetsAt, utilization, errorCode,
overageEnabled`; SDK also models `rate_limit_type` five_hour/seven_day/…); `result.total_cost_usd/usage/modelUsage` (estimates);
`system/api_retry.error` (`rate_limit, billing_error, oauth_org_not_allowed, account_on_hold…`); `/usage` result carries
`usage_report` (`SDKUsageReport`, experimental: `rate_limits.limits[]{kind,percent,resets_at}`, `extra_usage`).
Do not call claude.ai usage endpoints directly.

**Entitlement limits**: 5-hour + weekly windows; `--bare` cannot use the plan and is planned as `-p` default.

## 3. OpenAI / ChatGPT

**Sign in with ChatGPT, plan usage** [D developers.openai.com/siwc/token-sharing-open-source*; announced DevDay 2026-09-29]:
- Scope: "These docs explain ChatGPT plan usage for open-source and locally hosted apps." Paid or remotely hosted → interest form.
  Help 20001542: "Supported open-source tools remain available to all ChatGPT users"; commercial apps: Go/Plus/Pro/Business.
- No secret, no pre-registration. Authorise `https://auth.openai.com/api/accounts/authorize` with first-time
  `client_id=dynamic_agent_client`, `agent_name_hint=Axiom`, per-host `ext_agent_host_id` (opaque; RFC 9278 JWK thumbprint URN
  recommended, or UUIDv4 URN), PKCE S256, `state`, `nonce`, `resource=https://api.openai.com/v1`,
  `scope=openid profile email offline_access resource.invoke chatgpt.tokens.use.direct`.
- Redirect: HTTP loopback on literal `127.0.0.1` (not `localhost`); only the port may vary. Callback returns an issued
  `client_id` (`oaiapp_…`): persist per account+workspace; reuse it forever after.
- Token: `POST https://auth.openai.com/api/accounts/oauth/token`. Access 1 h; refresh 30 d rotating (serialise refreshes, store
  atomically); `earliest_refresh_at`. Validate ID token (JWKS, `iss`,`aud`,`exp`,`nonce`). Check granted scopes before inference.
- Inference: `GET /v1/models` (`visibility=="list"`, keep order), `POST /v1/responses` with `store:false, stream:true`. Never
  `backend-api`. Unsupported: `previous_response_id`, `temperature`, `top_p`, `metadata`, file search, code interpreter, image
  gen, computer use, hosted MCP, `tool_search`. Success only on `response.completed`.
- Errors: 429 `subscription_sharing_usage_limit_exceeded` (don't infer reset), 503 `…usage_unavailable`, 403 `…user_not_eligible`,
  400 `…unsupported_capability`, 401/403 scope/identity. "OpenAI does not silently switch to another billing path" — Axiom must not either.
- Revocation: discovery `revocation_endpoint`; user can disconnect in ChatGPT Settings (no webhook: detect on refresh).
- **No usage-reporting API** on this path: link to Settings → Usage. Plus five-hour window is shared across apps.
- No device-code flow documented; remote VM: sign in locally, transfer file (host-specific revocation "not yet available").

**Codex app-server with that token** [D codex-app-server]: env `ACCESS_TOKEN` + `-c model_provider="openai_chatgpt_plan"`,
`base_url="https://api.openai.com/v1"`, `env_key="ACCESS_TOKEN"`, `wire_api="responses"`, `requires_openai_auth=false`,
`supports_websockets=false`. Renewal = restart process + `thread/resume` (T3 Code avoids restarts with a provider
`auth={command=…}` token broker [S CodexManagedRuntime.ts]). Env value must not leak to tool subprocesses: verify Codex scrubs it.

**Codex's own login** [D app-server]: `account/login/start` `chatgpt`/`chatgptDeviceCode`. But: "App-server authentication has never
been permitted for commercial or hosted services"; local/open-source apps are told to migrate to SIWC. Acceptable fallback when
the user already has Codex CLI logged in (we only spawn it). `chatgptAuthTokens`: "FOR OPENAI INTERNAL USE ONLY" → never.
**Do not reuse the Codex CLI client id** (`app_EMoamEEZ73f0CkXaXp7hrann`) as Zed, Cline, Goose and legacy OpenCode/Pi paths do [S];
it presents Axiom as Codex. Codex lead's reported position (Sept 8): third-party use of that login "not an approved use"
except pure-OSS/partners [N aicoder; original post not read].

**Usage (Codex path)**: `account/read`; `account/rateLimits/read` → `rateLimitsByLimitId{limitId}.primary/secondary
{usedPercent,windowDurationMins,resetsAt}`, `credits`, `planType`, `rateLimitReachedType`, `rateLimitResetCredits`;
push `account/rateLimits/updated` (sparse); `account/usage/read` (daily tokens); `thread/tokenUsage/updated`. Whether
`rateLimits/read` works when the provider is a SIWC env token is [U] (likely not: no Codex auth present).

**Session**: Codex refreshes automatically; `auth.json` plaintext or keyring (`cli_auth_credentials_store`) — never read it.
**Limits**: one pool with ChatGPT Work/Codex; per-app weekly cap set by user; Business admin cap; credits off by default.
**API credits**: Pro $100/$200/$500, Business $20/$100 per seat monthly, separate from plan use [N enjoy.dev 2026-11-15].

**Verdict**: DIRECT-AUTHORISED, preview. Best native-engine subscription path available from any vendor. If Axiom ever charges
money or hosts execution remotely, this flips to "needs approval": a licence/business-model decision with auth consequences.

## 4. Google

- Consumer Gemini CLI/Code Assist ended 2026-06-18 [D deprecations, gemini-cli README 03862c4]; replaced by Antigravity (`agy`).
- Gemini CLI FAQ [S]: piggybacking on its OAuth "is a direct violation" of terms; use Vertex or AI Studio key.
- Antigravity terms §6 [D]: third-party access (e.g. "OpenClaw with Antigravity OAuth") "is a breach of this Agreement"; may suspend.
- Antigravity FAQ [D]: `agy -p` is for scripting/CI; "Wrapping `agy` inside third-party coding agents or applications" not supported
  under consumer plans; nor account rotation.
- Yet Google ships `agy acp` + registry `antigravity-acp` 1.5.2 and documents "other ACP clients" [D]; non-listed clients get Gemini models only.
- Forum replies presenting as Google [F, roles not shown as staff]: 2026-10-26 (thread 192027): T3 Code OK for "your own
  single-user, interactive development" with the official binary and native `oauth-personal`. Other threads (181088, 184785, 187536,
  198889, 199753) split: human-driven ACP allowed; MCP bridges, `agy --print` under other agents, harness-as-model-provider not.
- Enforcement: Feb 2026 wave (Register/VentureBeat 2026-02-23), ongoing reports of bans through 2026-11-15 (oh-my-pi #12487) [N].

**Verdict**: DELEGATE via unmodified `agy acp`, **human-initiated turns only**. Axiom must *not* let another agent/automation
dispatch work to a consumer Antigravity session; mark the adapter `interactiveOnly` in the scheduler. Offer AI Studio/Vertex key or
Gemini Enterprise for orchestration. Usage reporting: none standard found (`/usage` in CLI) [U]. Auth over ACP: `oauth-personal`,
`oauth-business`, `gemini-api-key`, `agent-platform`, `terminal-login`; no creds → `-32000`.

## 5. GitHub Copilot, Cursor, xAI

**Copilot** — SDK GA 2026-06-02, "available to all existing GitHub Copilot subscribers, including Copilot Free for personal use" [D
github.blog changelog]. Auth [S docs/auth, docs/setup]: stored CLI login ("Desktop applications where users interact directly"), own
GitHub OAuth/GitHub App user token (`gho_`/`ghu_`) — "Copilot usage is billed to each user's subscription"; "Your app never
handles model API keys"; env tokens; org server-to-server (2026-10-26); BYOK. ISV/desktop use is an explicitly documented persona.
Usage: `account.getQuota`, `assistant.usage` (`quotaSnapshots`, `copilotUsage`), `session.usage.getMetrics`, `session.quota.get|refresh`,
`models.list` pricing. Billing = GitHub AI Credits since 2026-06-01. Direct calls to Copilot's HTTP API with a borrowed VS Code client id
= not authorised; OpenCode's is a named partnership (2026-01-16). Axiom: register own OAuth/GitHub App with device flow, or use stored login;
set `clientInfo` honestly. **DIRECT-AUTHORISED (through SDK runtime), high.** Check: runtime licence terms [U].

**Cursor** — "embedding … is a supported and explicitly intended use of the Cursor SDK" (staff, forum 159741, 2026-05-04) [F]; no reselling,
no competing-model training (ToS 2026-09-03 §1.5). SDK: user/service API key, or `Cursor.auth.login()`; same pools as IDE; `agent.getUsage()`,
`usage` events. ACP: `cursor_login`. No raw model endpoint; unofficial proxies warned against. OpenAI models in Cursor: contract wind-down,
temporarily extended 2026-11-12 [N]. **DELEGATE, high.** Prefer ACP (permission interception); SDK auto-approves.

**xAI** — Grok Build for SuperGrok and X Premium+ [N x.ai/cli]; `grok login`, `--device-auth`, `XAI_API_KEY`; ACP "in other apps" is
documented use [D]. OpenCode (2026-05-21) and Kilo have official OAuth plan sign-in; OpenCode's client id is embedded in its source [S
xai.ts]: **not ours to reuse**; no public registration found. T3 FAQ claims direct collaboration [S]. **DELEGATE, high; direct UNKNOWN → ask.**

## 6. Chinese and other plans

- **Z.ai GLM Coding Plan** [D]: plan "may only be used within officially supported tools and products"; terms §4.2 name "SDK-based access or
  other third-party integrations"; >3 violations "may be banned". Listed tools include ZCode, Claude Code, Codex, OpenCode, Pi, Cline, Droid,
  Goose, Kilo, Crush, OpenClaw, Hermes. Endpoints `api.z.ai/api/anthropic`, `/api/coding/paas/v4`, `/api/v1`. Axiom unlisted → route the
  plan *through* a listed harness the user configured, and apply to be listed. Windows: 5 h + weekly. No supported usage API found.
- **Kimi** [D, zh]: plan key (≤5) with Anthropic/OpenAI-compatible base URLs in third-party tools; documented: Claude Code, OpenCode,
  Codex, Hermes. "篡改客户端标识（User-Agent）将被视为违规" (tampering with UA is a violation; suspension). Older help text limited use to
  named agents [N, conflicting]. Kimi Code CLI (MIT) has OAuth + ACP. DELEGATE first.
- **MiniMax** [D]: any OpenAI/Anthropic-compatible tool may use the M Plan key; "individual, interactive developer use"; usage
  `GET https://www.minimax.io/v1/token_plan/remains`. Keys not interchangeable with PAYG.
- **Alibaba Coding Plan** [D]: `sk-sp-…`, "interactive use in programming tools"; not "automated scripts, application backends, or other
  non-interactive scenarios"; suspension/revocation. Orchestrated background runs are out; tag `interactiveOnly`. Qwen OAuth free tier ended 2026-04-15.
- **Mistral** [D]: Vibe CLI browser sign-in; key created under Code › Vibe CLI "works in Free mode, with a paid plan"; third-party reuse unstated.
- **Amp**: `amp login`/`AMP_API_KEY`; can itself spend ChatGPT plan (SIWC partner). **Droid**: `FACTORY_API_KEY`.

## 7. What others do, and enforcement

| Tool | Claude | ChatGPT | Others | Notes |
|---|---|---|---|---|
| **T3 Code** (f692811b) [S] | Agent SDK → user's Claude Code; sets `CLAUDE_CODE_ENTRYPOINT=t3-code` (PR #28410, 2026-11-02) after ~1.7× drain | SIWC `dynamic_agent_client`, managed Codex with token broker | `agy` ACP, `@cursor/sdk`, Grok, ACP registry | FAQ claims all within ToS; Anthropic confirmation only second-hand |
| **OpenCode** 2.x (678621a) [S] | Direct OAuth gone early 2026 [N]; since 2.2.3 (2026-10-30) runs user's `claude` via Agent SDK, own tools, `CLAUDE_CODE_ENTRYPOINT=opencode`; author says permitted, not public [N] | SIWC (`chatgpt.ts`) + legacy Codex client (`openai.ts`) | Copilot (partner), xAI (partner) | Also supports account folder lists: avoid copying |
| **Pi** (0ab23c3) [S] | **Claude Code's OAuth client id + `user-agent: claude-cli/…`** | SIWC + legacy Codex | Copilot, Kimi, xAI | The Anthropic path is precisely what the 2026-11-12 AUP bans |
| **Cline** (34c104e) [S,D] | Claude Code provider/API | Codex client id OAuth | — | Predates SIWC (Jan 2026) |
| **Zed** (2b248e6) [S,D] | Only Claude Agent (ACP) / CLI in terminal, no provider path | `openai_subscribed` uses Codex client id | Copilot Chat provider; ACP agents | Cleanest Claude posture |
| **Goose** (03a343f) [S,D] | claude-acp (CLI wrappers deprecated) | codex-acp; legacy `chatgpt_codex` client id | ACP | Closest Rust precedent |
| **oh-my-pi** [N #12487] | direct OAuth + `claude-code/` UA | — | Antigravity OAuth, bans reported | Cautionary example |

**Enforcement timeline**: 2026-01-09 Anthropic blocks spoofed harness clients; 2026-01-16 GitHub–OpenCode partnership (after warning e-mails
[N]); 2026-02-19/20 Anthropic docs clarify OAuth ban (Register); 2026-02-23 Google suspends OpenClaw/Antigravity users; 2026-04-04 Claude
plan limits stop covering third-party harnesses (TechCrunch), later reinstated with separate "programmatic" credit; 2026-05-14 credit split
announced for 06-15, **paused 06-15**; 2026-06-18 Gemini CLI consumer cut-off; 2026-09-29 SIWC plan usage; 2026-10-07 Claude API credits; 2026-10-30
OpenCode Claude provider returns; 2026-11-02 T3 entrypoint change; 2026-11-12 Anthropic AUP bars routing/misrepresentation.
Pattern: vendors punish **token reuse and client spoofing**, tolerate or bless **spawning the unmodified vendor binary**, and increasingly
offer **official OAuth/SDK** (OpenAI, GitHub, Cursor, xAI partners). Axiom's constraints match the safe side.

## 8. Engineering requirements

1. `AuthLane` enum in the daemon: `ApiKey | PlanOAuth{provider} | DelegatedCli{harness} | CloudProvider`. No lane converts to another silently;
   on limit, stop and ask ("plan usage exhausted → use API key?").
2. Secrets: only ApiKey and PlanOAuth live in Axiom, in the OS keychain via the daemon; redact from logs, crash dumps, child env. Delegated
   CLIs inherit the user's HOME; Axiom never opens their stores.
3. SIWC module: loopback listener before browser; PKCE/state/nonce; ID-token validation; per-host URN minted once; per-account records;
   single-flight refresh; `403/429/503` mapping above; revoke on sign-out; link to Settings → Usage.
4. Policy metadata per adapter, data-driven and dated: `interactiveOnly`, `allowsOrchestration`, `needsApproval`, `policyUrl`, `checkedAt`;
   show it in settings; ship a remote-updatable *restrict-only* kill switch per adapter.
5. Env hygiene table per harness: strip conflicting keys; set only documented identity variables.
6. Quota model: `{scope, window, usedPercent?, resetsAt?, source, confidence}`; feed from Codex/Claude/Copilot events; "unknown" is a valid state.
7. Never automate account switching on limit.
8. Branding: text references only; "runs your installed X"; no vendor logos without permission.

## 9. Sources (all fetched/read 2026-11-24 unless dated)

Anthropic: code.claude.com/docs/en/{legal-and-compliance,authentication,headless,agent-sdk/overview,agent-sdk/typescript};
support.claude.com 15036540 (2026-10-07), 13189465, 17154008 (search extract); anthropic.com/legal/aup (eff. 2026-11-12).
OpenAI: developers.openai.com/siwc/token-sharing-open-source and /sign-in, /profiles-and-sessions, /models-and-inference, /codex-app-server,
/errors-and-recovery, /preview-limitations, /token-reference, /self-hosted-vms; learn.chatgpt.com/docs/{auth,app-server,sign-in-with-chatgpt};
help.openai.com 20001542, 20001410; xenospectrum/The New Stack SIWC coverage (2026-09-29, search extract); enjoy.dev (2026-11-15).
Google: antigravity.google/{terms,docs/faq,docs/cli/acp,docs/ide/extensions,docs/ide/extensions/acp}; gemini-cli docs/resources/{faq,tos-privacy}.md;
developers.google.com/gemini-code-assist/docs/deprecations; discuss.ai.google.dev threads 192027, 199753 (+ search extracts).
GitHub: github.blog/changelog 2026-01-16, 2026-06-02, 2026-10-26; copilot-sdk docs @2dfe56f. Cursor: cursor.com/terms-of-service (2026-09-03),
/docs/cli/acp, /docs/sdk/typescript; forum 159741. xAI: docs.x.ai/build/{overview,enterprise,cli/headless-scripting}; x.ai/news/grok-opencode (2026-05-21).
Z.ai: docs.z.ai/{devpack/usage-policy,devpack/tool/others,devpack/faq,legal-agreement/subscription-terms}. Kimi: kimi.com/code/docs.
MiniMax: platform.minimax.io/docs/token-plan/faq. Alibaba: help.aliyun.com/en/model-studio/coding-plan; qwen-code docs @c56424fc.
Mistral: docs.mistral.ai/vibe/code/cli/api-keys-profiles. Factory/Amp docs as in companion file.
Community: anthropics/claude-code #107106 (2026-11-04), #97074 (2026-09-25, extract); pingdotgg/t3code #29393 (2026-11-02) + docs/user/faq.md;
can1357/oh-my-pi #12487 (2026-09-19…11-15); cellcog.ai OpenCode post (2026-11-14, interested party); zed.dev/blog/anthropic-subscription-changes
(2026-05-14, upd. 06-16); theregister 2026-02-20/23; techcrunch 2026-04-04; zed.dev/docs/ai/use-an-existing-subscription.

## 10. Unverified / open

- Whether Anthropic considers a free OSS orchestrator spawning the user's `claude` as "offering rate limits"; no public ruling; T3/OpenCode
  permissions are hearsay. Whether custom `CLAUDE_CODE_ENTRYPOINT` is tolerated: hearsay; we recommend against.
- Consumer Terms automation clause wording was not re-read from anthropic.com/legal/consumer-terms.
- Full-text SIWC developer terms/branding: none located (UI guideline page listed in llms.txt but 404 at guessed URL). "Open source" is not
  defined on the page; permissive-licence public repo assumed sufficient. SIWC Business eligibility on the OSS path unclear.
- Interaction of SIWC token with `account/rateLimits/read`; whether Codex strips `ACCESS_TOKEN` from tool children.
- Google replies' authority (no staff badge visible); Nov 14 follow-up "any ACP client?" unanswered in the captured page.
- Antigravity FAQ/terms carry no date; Kimi/Z.ai/MiniMax/Alibaba pages carry no reliable date; Kimi pages contradict each other.
- GitHub: terms of the native Copilot runtime; whether Copilot Free via SDK is limited to personal use for end users of OSS apps.
- Amounts for Claude Ultra credits; OpenAI API-credit amounts come from a vendor blog. xAI OAuth client registration route.
- Windsurf/Devin, JetBrains AI, Augment, Kiro, Meta Muse, Xiaomi MiMo, Tencent CodeBuddy plans not examined.
