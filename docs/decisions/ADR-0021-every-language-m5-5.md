# ADR-0021: Every language (M5.5)

- **Status:** accepted (owner, 2026-11-22)
- **Context milestone:** M5.5 (`docs/roadmap/milestones.md`)
- **Amends:** ADR-0018 §2 (when a language server applies), `docs/protocols/providers.md`
  §Language servers (additive)

## Context

The owner asked for "syntax highlighting, autocomplete and knowledge of every language's SDK,
like VS Code". In M4 the shell highlights 17 languages from a hand-written extension list, two
servers are defined (rust-analyzer, typescript-language-server), a server applies only to a
workspace its provider detects (rust-analyzer needs a `Cargo.toml`), a missing server shows an
install hint, and completion opens on Ctrl+Space or a trigger character.

The owner decided (2026-11-22): the tier-1 list in the milestone is right; Axiom asks before
installing a server, with a setting that installs automatically instead.

## Decisions

### 1. A language registry in the shell, from the grammars `re_highlight` ships

`apps/desktop-flutter/lib/editor/languages.dart` lists every language: its id (the LSP language
id where LSP defines one, else the highlight.js name), its display name, its highlight.js
grammar, and how files are recognised: extensions, exact file names (`Dockerfile`, `Makefile`,
`CMakeLists.txt`), name patterns (`*.d.ts`, `.env.*`), shebang interpreters (`python3`, `node`,
`bash`) and a first-line pattern (`<?xml`). All 195 grammars `re_highlight` 0.0.3 ships are
registered; a file no rule matches is plain text. The status bar names the language and opens a
picker; a choice is kept per document for the session. The registry replaces `languageOf` and
`lspLanguageId`; the LSP id of a document is the registry's id when a server serves it.

Grammars load lazily: the registry holds a function per language, so the shell does not build
197 grammar objects at start.

### 2. Semantic tokens over the grammar

Where a ready server advertises `semanticTokensProvider` (full or range), the editor asks for the
visible range after an edit settles (250 ms) and colours the token types it maps
(namespace, type, class, enum, interface, struct, typeParameter, parameter, variable, property,
enumMember, function, method, macro, keyword, comment, string, number, regexp, operator) over the
grammar's spans, with the `readonly`, `static`, `deprecated` and `defaultLibrary` modifiers as
styles. Files over 1 MiB keep the grammar alone.

### 3. A server applies by language, not by project (amends ADR-0018 §2)

`lsp/servers` lists the `[[lsp]]` entries of every active provider, whether or not the provider
detects the workspace; the shell starts a server when a document of one of its languages opens
(unchanged). Providers whose only purpose is a server need no `[project]` table. Two servers may
claim a language; the one from the more specific source is used (workspace, then user, then
built-in; then by id), as a provider in a later source overrides an earlier one, and the others
are listed as `standby`.

### 4. A server is a provider: its install is the provider's install

Each server is a built-in provider (`providers/lsp-<name>/provider.toml`) with `[[tools]]` for
the server's executable, install methods, and its `[[lsp]]` entry. Servers therefore install,
update and remove exactly like toolchains: `env/planInstall` → the plan dialog → `env/install`,
checksummed archives (`archive`) or a delegate (`npm`, `go`, `dotnet`) into
`<data dir>/toolchains/<provider>/<version>`; nothing else on the machine is written.

When a server's command is not found in the workspace's environment, the daemon looks in the
newest managed install of the server's provider (its `bin_dirs`, its method `env`) before it
reports `missing`. The workspace's own selection (`.axiom/project.toml`) still wins.

Delegate installs pin what they install: npm packages by exact version
(`npm install --prefix ${root} --no-fund --no-audit <pkg>@<version>`), Go modules by version,
.NET tools by version. Archive installs carry a sha256 per artifact (from the release's published
digest). The node, go and dotnet delegates come from their providers (installed through the same
flow when absent).

### 5. Asking, and the switch that stops asking

A document whose language has a `missing` server shows one line above the editor:
"Python has a language server: basedpyright. [Install…] [Not now]". Install opens the ordinary
plan dialog (its steps, size, effects); approving installs it; the server starts when the install
ends. "Not now" hides the line for that language for the session.

Settings → Editor → "Install language servers automatically" (setting
`lsp.autoInstall`, boolean, default `false`). When on, the shell plans and approves the install
itself, shows it in the line ("Installing basedpyright…") and in Environments, and reports a
failure in the same line. Delegate prerequisites (Node for an npm server) are installed the same
way when the switch is on, and are part of the plan when it is off.

### 6. SDK awareness

A server runs in the workspace's environment (M2 resolution), so it sees the project's SDK. M5.5
adds SDK providers where none exists: Go, Java (Eclipse Temurin JDK 21), .NET, Dart. Amended at
implementation (M5.5-PROV-002): .NET 8 reached end of life on 2026-11-10 and csharp-ls 0.29 needs
.NET 11; Microsoft publishes SHA-512 digests only and Axiom's installer verifies SHA-256, so the
`dotnet` provider detects an installed SDK and names where to get one, and does not install it
in M5.5. Amended again (2026-11-22, the owner: C# and Swift must work):
- **.NET:** manifests and plans gain SHA-512 checksums (additive: manifest checksum kind
  `sha512`, plan `Sha256Source.sha512Digest`), so `dotnet` installs the .NET 11 SDK, verified by
  Microsoft's digests and exporting `DOTNET_ROOT`.
- **Swift on Linux:** a `swift` provider installs a pinned toolchain through swiftly (Swift's
  own installer, which checks each toolchain's PGP signature). swiftly is bootstrapped into the
  install root with a SHA-256 pinned after checking swift.org's signature. Archive methods gain
  `linux_names` (`${linux}` in `setup`) for swiftly's `--platform`. SourceKit-LSP `requires`
  Swift.
- **On macOS**, Xcode brings Swift.
- **Kotlin** (kotlin-lsp, Alpha) gives no completion and is left not working for now.

A server
that needs an SDK names it in `[[lsp]]`:

```toml
requires = ["java"]   # tool ids that must resolve in the environment; else `missing` with the
                      # provider that supplies them
```

Python servers get the workspace's virtual environment (`.venv`, `venv`, or the interpreter
`.axiom/project.toml` selects) through their settings.

### 7. Completion like VS Code

- Opens as you type an identifier (after one character, debounced 30 ms) and on the server's
  trigger characters; Ctrl+Space still opens it; a word-based list (the document's words) when no
  server serves the language.
- Filtering and ranking: fuzzy match on `filterText` (subsequence, word starts and case weighed),
  then the server's `sortText`; `preselect` honoured.
- Each row: the kind's icon, the label, `labelDetails`/`detail` faint at the right.
- Documentation beside the list for the selected item (`completionItem/resolve` when the item has
  none), Markdown rendered as in Chat.
- Accepting applies `textEdit` (or `insertText`) and `additionalTextEdits` (auto-imports);
  snippets (`insertTextFormat` 2) insert with tab stops: Tab and Shift+Tab move between them,
  Escape leaves. Commit characters accept.
- The client capabilities declare `snippetSupport`, `resolveSupport` (documentation, detail,
  additionalTextEdits), `labelDetailsSupport`, `insertReplaceSupport` and the documentation formats.

### 8. More features

Inlay hints (toggle in Settings, default on), document symbols as an outline (side bar section
and Mod+Shift+O), workspace symbols (Mod+T), folding ranges, format on save (setting, default
off), organize imports (a command), call hierarchy (a picker). Each is used only when the server
advertises it.

Amended at implementation (M5.5-EDIT-002): built are semantic tokens, workspace symbols
(Mod+T; Ctrl+T no longer transposes characters off macOS), format on save, organize imports
(Shift+Alt+O), call hierarchy (Shift+Alt+H, the callers) and document symbols (Ctrl+Shift+O, the
M4 picker). Deferred: **inlay hints** (re_editor maps the caret through each line's text, so text
inserted for display would move the caret and hit-testing; they need a re_editor change), **server
folding ranges** (re_editor runs its folding analyser in an isolate, fixed when the editor is
built; the bracket-based folding stays), and the **outline side bar section**. The
`editor.inlayHints` setting is not added.

### 9. Protocol and contracts

IPC stays 0.5: `lsp/*` relays any method, `env/*` installs. Additive and documented:
`ServerState::standby`; `[[lsp]].requires`; providers without `[project]`; settings
`lsp.autoInstall`, `editor.formatOnSave`. The schemas are regenerated.

## Consequences

- Highlighting covers every grammar `re_highlight` has; accuracy where a server runs comes from
  semantic tokens, not from a better grammar engine (tree-sitter stays a later option).
- Servers are external programs downloaded on the person's request; they are not linked into
  Axiom, so ADR-0011's licence rules for shipped code do not apply to them. Their licences are
  shown in the plan dialog.
- Each tier-1 server has a fixture project and an end-to-end test; a server release is pinned in
  its manifest and updated deliberately.

## Alternatives rejected

- **Running VS Code extensions** (Open VSX + a Node extension host): M7's question, not this one.
- **Installing every server up front:** gigabytes for languages the person may never open.
- **Installing silently by default:** the owner chose asking, with a switch.
