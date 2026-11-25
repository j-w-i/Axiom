# 0007 Subscriptions: three lanes, authorised routes only
**Decision.** Lanes: *API key*; *plan sign-in* where the vendor offers it to third parties (ChatGPT "Sign in with ChatGPT" preview; GitHub Copilot SDK); *your installed agent*, where
Axiom launches the vendor's unmodified binary and never sees a token (Claude, Google, Cursor, xAI). Never: reading another program's credentials, reusing its OAuth client id, forging a user agent, or setting undocumented identity variables.
**Evidence.** `research/subscription-auth.md`, dated quotations and verdicts. **Confidence.** Claude: medium; the delegation verdict rests on the published legal page.
**Open, owner's action.** Written confirmation from Anthropic before launch; OpenAI's interest form if Axiom is ever paid or hosted.
**Multiple accounts (PR-4).** Any number may be linked. Axiom does not rotate between them to evade a limit; a person or their own rule chooses, where the terms allow.
**Revisit.** Monthly; policies are data with a date and a restrict-only remote switch.
