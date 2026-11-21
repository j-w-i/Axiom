# ADR-0017: Chat is a first-class surface, not only a dock beside the editor

Status: **Accepted** (2026-11-21). Requested by the owner during M3 ("there should be a first
rate chat ui … half of people will never touch the code so we need an excellent prompt chat
ui", 2026-11-21). Amends ADR-0015 §10 (UI scope) and `workspace-spec.md` §5.2.

## Context
ADR-0015 planned the AI workspace as a right-hand dock next to the editor (`workspace-spec.md`
§5.2). That serves people working in code. The owner states that about half of Axiom's users
will never open the code view, and that prompting a model must be a first-rate experience on its
own: a full chat application inside Axiom.

## Decisions

### 1. Two surfaces, one conversation model
- **Chat** (new): a full-window destination on the activity rail and on the Welcome screen,
  available **with or without an open folder**:
  - the conversation list in the side bar (search, rename, delete, newest first);
  - the conversation in the main area at a comfortable reading width;
  - the composer at the bottom.
  It is a complete product surface in its own right.
- **Dock** (ADR-0015 §10): the same conversation, beside the editor, for code work.
- Both render the same components from the same store and the same `agent/*` events. A
  conversation can be opened in either.

### 2. Conversations without a workspace
- **Optional workspace.** `workspaceId` becomes optional in `SessionCreateParams`,
  `SessionRef`, `SendParams` and `RestoreParams`.
- **A session without a workspace** is stored under `sessions/_chat/` and is offered no
  workspace tools; it is a plain conversation (MCP tools from `agent.mcpServers` still apply).
  External ACP agents need a working directory, so they are offered only with a workspace.
- **Compatibility.** Making a field optional keeps every message the contract accepted valid.
  This is the one non-additive-looking change; it is safe because no client or daemon serves
  sessions yet. It is made by task M3-ARCH-002 with wire tests, and `agent.rs` stays frozen
  apart from it.
- **`agent/sessionList`** takes an optional workspace: absent lists the workspace-free
  conversations.

### 3. What a first-rate chat needs (protocol 0.3, additive, M3-ARCH-002)
- **`agent/sessionRename`, `agent/sessionDelete`:** delete removes the journal and the index
  row; checkpoints stay until their workspace's are pruned.
- **Edit and regenerate.**
  - `SendParams.fromTurn` (optional): the new message replaces turn N and everything after it
    in the *context*. The journal appends a `rewind` event (append-only stays true), and replay
    shows the latest branch.
  - Regenerate is `fromTurn = last` with the same text.
- **Images.** `SendParams.images` (`{mediaType, data}` base64, ≤ 5 MiB each, PNG/JPEG/GIF/WebP)
  for pasted or dropped images.
  - Stored as content-addressed blobs next to the journal.
  - Sent as image blocks to models with `images: yes|unknown`.
  - A model with `images: no` gets a text note instead, and the composer says so before
    sending.
- **Titles.** A new conversation's title is set from the first user message (first line,
  ≤ 60 characters); `agent/sessionRename` overrides it. No extra model call.

### 4. Rendering
- **Assistant text is Markdown** (CommonMark + GFM tables, task lists, strikethrough,
  autolinks), rendered by `react-markdown` + `remark-gfm` (MIT; leaf libraries, allowed by
  governance rule 1, recorded here).
- **Raw HTML is never rendered.** Links open in the system browser through the host, never in
  the webview.
- **Code blocks:** language label, Copy, and syntax colouring through Monaco's own `colorize`
  (no second highlighter). With a workspace, a block can be inserted into the active editor.
- **Streaming** appends per frame with no typewriter delay (`motion-system.md`); the view
  follows the end unless the user has scrolled up, and a "Jump to latest" control appears.
- **Per message:** Copy, Edit (user), Regenerate (last assistant message); tool rows and
  approvals as in the dock (§5.2).

### 5. First run
With no model configured, Chat shows an inline setup panel, not an empty state:
- **API key:** Anthropic or OpenAI-compatible, with the key stored in the keychain.
- **Local server:** Ollama / llama.cpp / LM Studio are probed on their default ports.
- **Signed-in agent:** Claude Code / Codex / Gemini when installed (ADR-0016).
The Models pane in Settings stays the full form.

### 6. Design
`workspace-spec.md` gains §5.4 "Chat": layout, states, keyboard model, accessibility and the
reading measure (≈ 72 characters at the default size). It is built in the M1.6 design system's
material grammar and tokens, with no new colours. Shots and a11y suites cover it.

## Consequences
- M3's UI work grows. Task M3-UI-001 becomes the shared conversation components plus the Chat
  surface; M3-UI-002 adds the dock, approvals, diffs and checkpoints; M3-UI-003 adds the first
  run and the Models pane. M3-ARCH-002 adds the protocol pieces, before M3-LOOP-001.
- The exit criterion gains: a person with no folder open can configure a model and hold a
  conversation with Markdown, code, edit/regenerate and images, entirely by keyboard.
