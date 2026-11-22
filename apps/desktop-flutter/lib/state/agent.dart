// Chat's state (FL-CHAT-001; a port of the Tauri shell's AgentStore, ADR-0017, ADR-0020 §6):
// providers and models (Axiom's own and the external agents), the conversations of the current
// scope, and the open conversation's transcript, built from the session's `agent/event` stream
// (replayed by `agent/sessionOpen`). Streamed text is coalesced per frame. Everything comes from
// `agent/*`: the shell never runs a model or a tool.

import 'dart:async';

import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';

import '../daemon/client.dart';
import '../daemon/protocol.g.dart';
import 'workspace.dart';

enum Scope { general, workspace }

final class ModelOption {
  const ModelOption({required this.id, required this.name, required this.images});
  final String id, name;
  final bool images;
}

final class ProviderOption {
  const ProviderOption({
    required this.providerId,
    required this.name,
    required this.kind,
    required this.models,
    required this.hasKey,
    required this.builtin,
    this.error,
    this.baseUrl,
    this.apiKeyEnv,
  });
  final String providerId, name;
  final ProviderKind kind;
  final List<ModelOption> models;
  final bool hasKey, builtin;

  /// Why it cannot be used now (one sentence), if so.
  final String? error;
  final String? baseUrl;

  /// The key comes from this environment variable (set in the provider's configuration).
  final String? apiKeyEnv;
  bool get isAgent => kind == ProviderKind.acp;
  bool get usable => error == null && models.isNotEmpty;
}

/// The external agents Axiom ships definitions for (ADR-0016) and can install (ADR-0020 §6).
const builtinAgents = ['claude-code', 'codex', 'gemini'];

List<ProviderOption> providerOptions(ModelsResult r) => [
  for (final p in r.providers)
    ProviderOption(
      providerId: p.config.id,
      name: p.config.name,
      kind: p.config.kind,
      models: [
        for (final m in p.models)
          ModelOption(id: m.id, name: m.displayName, images: m.capabilities.images != Support.no),
      ],
      hasKey: p.hasKey,
      builtin: p.config.kind == ProviderKind.acp && builtinAgents.contains(p.config.id),
      error: p.error,
      baseUrl: p.config.baseUrl,
      apiKeyEnv: p.config.apiKeyEnv,
    ),
];

/// Local model servers on their usual ports (ADR-0017 §5).
const localServers = [
  (id: 'ollama', name: 'Ollama', baseUrl: 'http://127.0.0.1:11434/v1'),
  (id: 'llama-cpp', name: 'llama.cpp', baseUrl: 'http://127.0.0.1:8080/v1'),
  (id: 'lm-studio', name: 'LM Studio', baseUrl: 'http://127.0.0.1:1234/v1'),
];

/// A provider id from a name or URL: `[a-z0-9-]`, 1–40 (the daemon's rule).
String providerIdFrom(String from) {
  var id = from
      .toLowerCase()
      .replaceFirst(RegExp(r'^https?://'), '')
      .replaceAll(RegExp(r'[^a-z0-9]+'), '-')
      .replaceAll(RegExp(r'^-+|-+$'), '');
  if (id.length > 40) id = id.substring(0, 40).replaceAll(RegExp(r'-+$'), '');
  return id.isEmpty ? 'provider' : id;
}

enum AddKind { anthropic, openai }

/// The configuration a filled add-a-provider form describes, or an error sentence.
Object formConfig(AddKind kind, String baseUrl, String name, List<String> existing) {
  final url = baseUrl.trim();
  if (kind == AddKind.anthropic) {
    final id = existing.contains('anthropic') && url.isNotEmpty ? providerIdFrom(url) : 'anthropic';
    return ProviderConfig(
      id: id,
      kind: ProviderKind.anthropic,
      name: name.trim().isEmpty ? 'Anthropic' : name.trim(),
      baseUrl: url.isEmpty ? null : url,
    );
  }
  if (url.isNotEmpty && !RegExp(r'^https?://', caseSensitive: false).hasMatch(url)) {
    return 'The server address starts with https:// (or http:// for this computer).';
  }
  final effective = url.isEmpty ? 'https://api.openai.com/v1' : url;
  final official = RegExp(r'^https://api\.openai\.com', caseSensitive: false).hasMatch(effective);
  final bare = effective.replaceFirst(RegExp(r'/v1/?$'), '');
  return ProviderConfig(
    id: official ? 'openai' : providerIdFrom(bare),
    kind: ProviderKind.openaiChat,
    name: name.trim().isNotEmpty
        ? name.trim()
        : (official ? 'OpenAI' : bare.replaceFirst(RegExp(r'^https?://'), '')),
    baseUrl: url.isEmpty ? null : url,
  );
}

typedef ModelChoice = ({String providerId, String model});

/// One thing in the transcript.
sealed class Item {
  const Item(this.key, this.turn);
  final String key;
  final int turn;
}

final class UserItem extends Item {
  const UserItem(super.key, super.turn, this.text, this.images);
  final String text;
  final List<ImageRef> images;
}

final class AssistantItem extends Item {
  const AssistantItem(super.key, super.turn, this.text, this.textKind);
  final String text;
  final TextKind? textKind;
}

enum ToolState { running, ok, error }

final class ToolItem extends Item {
  const ToolItem(
    super.key,
    super.turn, {
    required this.callId,
    required this.tool,
    required this.targets,
    required this.state,
    required this.summary,
    this.output,
  });
  final String callId, tool, summary;
  final List<String> targets;
  final ToolState state;
  final Object? output;

  ToolItem done(ToolState s, String summary, Object? output) => ToolItem(
    key,
    turn,
    callId: callId,
    tool: tool,
    targets: targets,
    state: s,
    summary: summary,
    output: output,
  );
}

final class CheckpointItem extends Item {
  const CheckpointItem(super.key, super.turn, this.number, this.label, this.createdMs);
  final int number;
  final String label;
  final int createdMs;
}

final class NoticeItem extends Item {
  const NoticeItem(super.key, super.turn, this.message, {required this.notice});
  final String message;

  /// A notice (grey) rather than an error (red).
  final bool notice;
}

/// The transcript of a session's events (rewinds applied). Pure.
List<Item> transcript(List<AgentEvent> events) {
  var items = <Item>[];
  final tools = <String, int>{}; // callId → index in items
  for (final e in events) {
    final key = '${e.seq}';
    switch (e.event) {
      case EventBodyUser(:final text, :final images):
        items.add(UserItem(key, e.turn, text, images ?? const []));
      case EventBodyRewind(:final fromTurn):
        items = [
          for (final i in items)
            if (i.turn < fromTurn) i,
        ];
        tools.clear();
        for (var j = 0; j < items.length; j++) {
          final it = items[j];
          if (it is ToolItem) tools[it.callId] = j;
        }
      case EventBodyAssistantText(:final text, :final kind):
        items.add(AssistantItem(key, e.turn, text, kind));
      case EventBodyToolCall(:final callId, :final tool, :final targets):
        tools[callId] = items.length;
        items.add(
          ToolItem(
            key,
            e.turn,
            callId: callId,
            tool: tool,
            targets: targets ?? const [],
            state: ToolState.running,
            summary: '',
          ),
        );
      case EventBodyToolResult(:final callId, :final isError, :final summary, :final output):
        final j = tools[callId];
        if (j != null && items[j] is ToolItem) {
          items[j] = (items[j] as ToolItem).done(
            isError ? ToolState.error : ToolState.ok,
            summary,
            output,
          );
        }
      case EventBodyInterrupted(:final callId):
        final j = tools[callId];
        if (j != null && items[j] is ToolItem) {
          items[j] = (items[j] as ToolItem).done(
            ToolState.error,
            'Interrupted: Axiom stopped while it ran',
            null,
          );
        }
      case EventBodyCheckpoint(:final checkpoint):
        items.add(
          CheckpointItem(key, e.turn, checkpoint.number, checkpoint.label, checkpoint.createdMs),
        );
      case EventBodyError(:final message, :final kind):
        items.add(NoticeItem(key, e.turn, message, notice: kind == 'notice'));
      case EventBodyRestored(:final result):
        final n = result.restored.length;
        final skipped = result.conflicts?.length ?? 0;
        items.add(
          NoticeItem(
            key,
            e.turn,
            'Restored checkpoint ${result.checkpoint}: $n file${n == 1 ? '' : 's'}${skipped > 0 ? ', $skipped left as they were' : ''}. Checkpoint ${result.undoCheckpoint} undoes this.',
            notice: true,
          ),
        );
      default:
        break; // usage, turnEnd, compaction, approvals: not items
    }
  }
  return items;
}

/// The last user message (↑ to edit, Regenerate).
UserItem? lastUser(List<Item> items) {
  for (var i = items.length - 1; i >= 0; i--) {
    final it = items[i];
    if (it is UserItem) return it;
  }
  return null;
}

class Conversation {
  Conversation({List<AgentEvent>? events, this.streaming = '', this.running = false})
    : events = events ?? [];
  final List<AgentEvent> events;

  /// Text streamed since the last complete block (not persisted).
  String streaming;
  bool running;
}

String describeAgentError(Object e) {
  if (e is! DaemonException) return '$e';
  if (e.kind == ErrorKind.conflict) return 'A reply is still being written. Stop it first.';
  return e.message;
}

class AgentStore extends ChangeNotifier {
  AgentStore(this.client, this.workspace) {
    _offs = [
      client.on(Notifications.agentEvent, _onEvent),
      client.on(Notifications.agentApprovalRequested, (r) {
        approvals = [...approvals.where((a) => a.requestId != r.requestId), r];
        notifyListeners();
      }),
      client.on(Notifications.agentApprovalResolved, (r) {
        approvals = [...approvals.where((a) => a.requestId != r.requestId)];
        notifyListeners();
      }),
    ];
    workspace.addListener(_onWorkspace);
    client.state.addListener(_onDaemonState);
  }

  bool _daemonLost = false;

  /// The daemon went away mid-reply: no turn is running any more, and nothing waits for an
  /// answer; when it is back, the open conversation is read again (AUD-025).
  void _onDaemonState() {
    final ready = client.state.value is DaemonReady;
    if (!ready) {
      if (_daemonLost) return;
      _daemonLost = true;
      for (final c in _cache.values) {
        c.running = false;
        c.streaming = '';
      }
      _pendingDelta.clear();
      approvals = const [];
      sessions = [
        for (final x in sessions)
          if (x.running) _withRunning(x, false) else x,
      ];
      if (activeId != null && active.running) active.running = false;
      notifyListeners();
      return;
    }
    if (!_daemonLost) return;
    _daemonLost = false;
    final id = activeId;
    if (id != null) unawaited(open(id));
  }

  SessionInfo _withRunning(SessionInfo s, bool running) => SessionInfo(
    sessionId: s.sessionId,
    title: s.title,
    providerId: s.providerId,
    model: s.model,
    createdMs: s.createdMs,
    updatedMs: s.updatedMs,
    pendingApprovals: s.pendingApprovals,
    running: running,
  );

  final DaemonClient client;
  final WorkspaceStore workspace;
  late final List<void Function()> _offs;

  String status = 'idle'; // idle, loading, ready, error
  String error = '';
  List<ProviderOption> providers = const [];
  Scope scope = Scope.general;
  List<SessionInfo> sessions = const [];
  String search = '';
  String? activeId;
  Conversation active = Conversation();
  ModelChoice? model;
  String sendError = '';
  final images = <String, ImageData>{};
  final _imagesLoading = <String>{};
  List<ApprovalRequest> approvals = const [];

  /// Context the person attached for the next message (the Inspector's element, M5-UI-001):
  /// source lines the daemon adds to the turn, each with its label for the composer.
  List<({Attachment attachment, String label})> context = const [];

  /// Text for an empty composer, taken once by the Chat view that shows it.
  String? composerSeed;

  void attach(Attachment a, String label, {String? seed}) {
    context = [...context.where((c) => c.label != label), (attachment: a, label: label)];
    if (seed != null) composerSeed = seed;
    notifyListeners();
  }

  void detach(int i) {
    if (i < 0 || i >= context.length) return;
    context = [...context]..removeAt(i);
    notifyListeners();
  }

  void seed(String text) {
    composerSeed = text;
    notifyListeners();
  }

  /// The seed, once.
  String? takeSeed() {
    final s = composerSeed;
    composerSeed = null;
    return s;
  }

  ({int checkpoint, List<RestoreConflict> conflicts})? restoreConflict;

  /// Agents being installed (the spinner shows), and how to stop each.
  final installing = <String>{};
  final _installCancels = <String, CancelToken>{};

  /// Stops an install (the daemon stops npm).
  void cancelInstall(String providerId) => _installCancels[providerId]?.cancel();
  final _cache = <String, Conversation>{};
  final _pendingDelta = <String, String>{};
  bool _flushScheduled = false;
  int _loadSeq = 0;
  String? _lastWs;

  String? get _wsId => scope == Scope.workspace && workspace.isOpen ? workspace.id : null;

  void _onWorkspace() {
    final id = workspace.isOpen ? workspace.id : null;
    // A folder switch passes through "no folder" while the next one opens: that is not a reason
    // to leave the folder's scope (AUD-092).
    if (id == null && workspace.opening) return;
    if (id == _lastWs) return;
    _lastWs = id;
    context = const [];
    if (id == null && scope == Scope.workspace) {
      newChat(Scope.general);
    } else if (status != 'idle') {
      unawaited(refreshSessions());
    }
  }

  /// Loads providers and conversations (Chat's first open, or after a change).
  Future<void> load() async {
    status = status == 'ready' ? 'ready' : 'loading';
    error = '';
    notifyListeners();
    try {
      final m = await client.call(Methods.agentModels, const ModelsParams());
      providers = providerOptions(m);
      bool usable(ModelChoice? c) =>
          c != null &&
          providers.any(
            (p) => p.providerId == c.providerId && p.usable && p.models.any((x) => x.id == c.model),
          );
      if (!usable(model)) {
        final first =
            providers.where((p) => p.usable && !p.isAgent).firstOrNull ??
            providers.where((p) => p.usable).firstOrNull;
        model = first == null ? null : (providerId: first.providerId, model: first.models.first.id);
      }
      status = 'ready';
      notifyListeners();
      await refreshSessions();
    } on DaemonException catch (e) {
      status = 'error';
      error = describeAgentError(e);
      notifyListeners();
    }
  }

  Future<void> refreshSessions() async {
    final seq = ++_loadSeq;
    try {
      final r = await client.call(Methods.agentSessionList, SessionListParams(workspaceId: _wsId));
      if (seq != _loadSeq) return;
      sessions = r.sessions;
      notifyListeners();
    } on DaemonException catch (e) {
      if (seq == _loadSeq) {
        error = describeAgentError(e);
        notifyListeners();
      }
    }
  }

  void setSearch(String s) {
    search = s;
    notifyListeners();
  }

  List<SessionInfo> filtered() {
    final q = search.trim().toLowerCase();
    return q.isEmpty
        ? sessions
        : [
            for (final s in sessions)
              if (s.title.toLowerCase().contains(q)) s,
          ];
  }

  void setScope(Scope s) {
    if (s == scope) return;
    scope = s;
    sessions = const [];
    newChat(s);
    unawaited(load());
  }

  void setModel(ModelChoice m) {
    model = m;
    notifyListeners();
  }

  /// A new, empty chat (created on its first message).
  void newChat([Scope? s]) {
    scope = s ?? scope;
    activeId = null;
    active = Conversation();
    sendError = '';
    notifyListeners();
  }

  Future<void> open(String id) async {
    activeId = id;
    // Cached before the call, so events the daemon sends ahead of its answer are kept and merged
    // (AUD-088).
    active = _cache.putIfAbsent(id, Conversation.new);
    sendError = '';
    notifyListeners();
    try {
      final r = await client.call(
        Methods.agentSessionOpen,
        SessionRef(workspaceId: _wsId, sessionId: id),
      );
      final prev = _cache[id];
      final last = r.events.isEmpty ? 0 : r.events.last.seq;
      final extra = [
        for (final e in prev?.events ?? const <AgentEvent>[])
          if (e.seq > last) e,
      ];
      final conv = Conversation(
        events: [...r.events, ...extra],
        streaming: prev?.streaming ?? '',
        running: r.session.running,
      );
      _cache[id] = conv;
      approvals = [...approvals.where((a) => a.sessionId != id), ...?r.pending];
      if (activeId == id) {
        active = conv;
        notifyListeners();
      }
    } on DaemonException catch (e) {
      if (activeId == id) {
        sendError = describeAgentError(e);
        notifyListeners();
      }
    }
  }

  /// Sends a message, creating the conversation first for a new chat.
  Future<bool> send(String text, {List<ImageData> images = const [], int? fromTurn}) async {
    if (text.trim().isEmpty && images.isEmpty) return false;
    sendError = '';
    var id = activeId;
    try {
      if (id == null) {
        final m = model;
        if (m == null) {
          sendError = 'Choose a model or an agent first.';
          notifyListeners();
          return false;
        }
        final s = await client.call(
          Methods.agentSessionCreate,
          SessionCreateParams(workspaceId: _wsId, providerId: m.providerId, model: m.model),
        );
        id = s.sessionId;
        _cache[id] = Conversation();
        activeId = id;
        active = _cache[id]!;
        sessions = [s, ...sessions];
      }
      _patch(id, running: true);
      final attached = context;
      await client.call(
        Methods.agentSend,
        SendParams(
          workspaceId: _wsId,
          sessionId: id,
          text: text,
          attachments: attached.isEmpty ? null : [for (final c in attached) c.attachment],
          images: images.isEmpty ? null : images,
          fromTurn: fromTurn,
        ),
      );
      if (identical(context, attached)) {
        context = const [];
        notifyListeners();
      }
      return true;
    } on DaemonException catch (e) {
      if (id != null) _patch(id, running: false);
      sendError = describeAgentError(e);
      notifyListeners();
      return false;
    }
  }

  void stop() {
    final id = activeId;
    if (id == null) return;
    unawaited(
      client
          .call(Methods.agentCancel, SessionRef(workspaceId: _wsId, sessionId: id))
          .catchError((Object _) => const Empty()),
    );
  }

  Future<bool> edit(int turn, String text) => send(text, fromTurn: turn);

  Future<bool> regenerate() {
    final u = lastUser(transcript(active.events));
    return u == null ? Future.value(false) : send(u.text, fromTurn: u.turn);
  }

  Future<void> rename(String id, String title) async {
    try {
      final s = await client.call(
        Methods.agentSessionRename,
        SessionRenameParams(workspaceId: _wsId, sessionId: id, title: title),
      );
      sessions = [for (final x in sessions) x.sessionId == id ? s : x];
      notifyListeners();
    } on DaemonException catch (e) {
      error = describeAgentError(e);
      notifyListeners();
    }
  }

  Future<void> remove(String id) async {
    try {
      await client.call(Methods.agentSessionDelete, SessionRef(workspaceId: _wsId, sessionId: id));
      _cache.remove(id);
      sessions = [
        for (final s in sessions)
          if (s.sessionId != id) s,
      ];
      if (activeId == id) {
        newChat();
      } else {
        notifyListeners();
      }
    } on DaemonException catch (e) {
      error = describeAgentError(e);
      notifyListeners();
    }
  }

  /// Adds or replaces a provider; the key goes to the OS credential store through the daemon.
  /// Returns an error sentence, or '' on success.
  Future<String> configure(ProviderConfig config, {String? apiKey}) async {
    try {
      await client.call(
        Methods.agentConfigureProvider,
        ConfigureProviderParams(
          id: config.id,
          config: config,
          apiKey: apiKey == null || apiKey.isEmpty ? null : apiKey,
        ),
      );
      await load();
      final p = providers.where((x) => x.providerId == config.id).firstOrNull;
      if (p != null && p.usable && activeId == null) {
        setModel((providerId: p.providerId, model: p.models.first.id));
      }
      return p?.error ?? '';
    } on DaemonException catch (e) {
      return describeAgentError(e);
    }
  }

  /// Sets or clears ([key] null) a provider's stored key. Returns an error sentence, or ''.
  Future<String> setKey(String id, String? key) async {
    try {
      final m = await client.call(Methods.agentModels, ModelsParams(providerId: id));
      final config = m.providers.firstOrNull?.config;
      if (config == null) return 'That provider is gone.';
      await client.call(
        Methods.agentConfigureProvider,
        ConfigureProviderParams(
          id: id,
          config: config,
          apiKey: key,
          clearKey: key == null ? true : null,
        ),
      );
      await load();
      return '';
    } on DaemonException catch (e) {
      return describeAgentError(e);
    }
  }

  Future<String> removeProvider(String id) async {
    try {
      await client.call(Methods.agentConfigureProvider, ConfigureProviderParams(id: id));
      if (model?.providerId == id) model = null;
      await load();
      return '';
    } on DaemonException catch (e) {
      return describeAgentError(e);
    }
  }

  /// Looks for the local servers on their usual ports: each is added, and removed again if it
  /// does not answer. Returns what was found ("Ollama: 3 models") and what was not.
  Future<List<({String name, bool found, String detail})>> detectLocal() async {
    final out = <({String name, bool found, String detail})>[];
    for (final srv in localServers) {
      final existing = providers.where((p) => p.providerId == srv.id).firstOrNull;
      if (existing != null && existing.error == null) {
        out.add((
          name: srv.name,
          found: true,
          detail: '${existing.models.length} models (already added)',
        ));
        continue;
      }
      try {
        await client.call(
          Methods.agentConfigureProvider,
          ConfigureProviderParams(
            id: srv.id,
            config: ProviderConfig(
              id: srv.id,
              kind: ProviderKind.openaiChat,
              name: srv.name,
              baseUrl: srv.baseUrl,
            ),
          ),
        );
        final m = await client.call(Methods.agentModels, ModelsParams(providerId: srv.id));
        final p = m.providers.firstOrNull;
        if (p != null && p.error == null && p.models.isNotEmpty) {
          out.add((name: srv.name, found: true, detail: '${p.models.length} models'));
        } else {
          await client.call(Methods.agentConfigureProvider, ConfigureProviderParams(id: srv.id));
          out.add((
            name: srv.name,
            found: false,
            detail: p?.error != null ? 'not running' : 'no models',
          ));
        }
      } on DaemonException catch (e) {
        out.add((name: srv.name, found: false, detail: describeAgentError(e)));
      }
    }
    await load();
    return out;
  }

  /// Installs a built-in agent (ADR-0020 §6). Returns an error sentence, or '' on success.
  Future<String> installAgent(String providerId) async {
    installing.add(providerId);
    final cancel = _installCancels[providerId] = CancelToken();
    notifyListeners();
    try {
      await client.call(
        Methods.agentInstallAgent,
        InstallAgentParams(providerId: providerId),
        timeout: const Duration(minutes: 11),
        cancel: cancel,
      );
      await load();
      final p = providers.where((x) => x.providerId == providerId).firstOrNull;
      if (p != null && p.usable) setModel((providerId: p.providerId, model: p.models.first.id));
      return '';
    } on DaemonException catch (e) {
      return cancel.isCancelled ? 'Cancelled.' : describeAgentError(e);
    } finally {
      _installCancels.remove(providerId);
      installing.remove(providerId);
      notifyListeners();
    }
  }

  /// Answers an approval (the first answer wins; a late one is ignored).
  Future<void> approve(String requestId, ApprovalDecision decision) async {
    approvals = [
      for (final a in approvals)
        if (a.requestId != requestId) a,
    ];
    notifyListeners();
    try {
      await client.call(
        Methods.agentApprove,
        ApproveParams(requestId: requestId, decision: decision),
      );
    } on DaemonException {
      // Already answered elsewhere, or withdrawn.
    }
  }

  /// Restores a checkpoint of the open conversation; conflicts wait for the person's choice.
  Future<RestoreResult?> restore(int checkpoint, {bool skipConflicts = false}) async {
    final id = activeId;
    if (id == null) return null;
    try {
      final r = await client.call(
        Methods.agentRestore,
        RestoreParams(
          workspaceId: _wsId,
          sessionId: id,
          checkpoint: checkpoint,
          skipConflicts: skipConflicts,
        ),
      );
      restoreConflict = null;
      notifyListeners();
      return r;
    } on DaemonException catch (e) {
      if (e.kind == ErrorKind.checkpointConflict) {
        final raw = e.details is Map ? (e.details! as Map)['conflicts'] : null;
        restoreConflict = (
          checkpoint: checkpoint,
          conflicts: [
            for (final c in raw is List ? raw : const <Object?>[]) RestoreConflict.fromJson(c),
          ],
        );
      } else {
        error = describeAgentError(e);
        restoreConflict = null;
      }
      notifyListeners();
      return null;
    }
  }

  void dismissRestoreConflict() {
    restoreConflict = null;
    notifyListeners();
  }

  /// Approvals of other conversations and of external principals (MCP).
  List<ApprovalRequest> otherApprovals() => [
    for (final a in approvals)
      if (a.sessionId == null || a.sessionId != activeId) a,
  ];

  Future<void> loadImage(ImageRef ref) async {
    final id = activeId;
    if (id == null || images.containsKey(ref.sha256) || !_imagesLoading.add(ref.sha256)) return;
    try {
      images[ref.sha256] = await client.call(
        Methods.agentImage,
        ImageParams(workspaceId: _wsId, sessionId: id, sha256: ref.sha256),
      );
      notifyListeners();
    } on DaemonException {
      // Gone: the thumbnail shows its placeholder.
    } finally {
      _imagesLoading.remove(ref.sha256);
    }
  }

  void dismissError() {
    error = '';
    sendError = '';
    notifyListeners();
  }

  /// The open conversation's transcript (rebuilt only when its events change: a streamed delta
  /// does not).
  List<Item> items() {
    final c = active;
    final memo = _itemsMemo;
    if (memo != null && identical(memo.$1, c) && memo.$2 == c.events.length) return memo.$3;
    final items = transcript(c.events);
    _itemsMemo = (c, c.events.length, items);
    return items;
  }

  (Conversation, int, List<Item>)? _itemsMemo;

  void _patch(String id, {List<AgentEvent>? events, String? streaming, bool? running}) {
    final c = _cache.putIfAbsent(id, Conversation.new);
    if (events != null) {
      c.events
        ..clear()
        ..addAll(events);
    }
    if (streaming != null) c.streaming = streaming;
    if (running != null) {
      c.running = running;
      sessions = [
        for (final s in sessions)
          s.sessionId == id
              ? SessionInfo(
                  sessionId: s.sessionId,
                  title: s.title,
                  providerId: s.providerId,
                  model: s.model,
                  createdMs: s.createdMs,
                  updatedMs: s.updatedMs,
                  pendingApprovals: s.pendingApprovals,
                  running: running,
                )
              : s,
      ];
    }
    if (activeId == id) active = c;
    notifyListeners();
  }

  void _onEvent(AgentEvent e) {
    final id = e.sessionId;
    final conv = _cache[id];
    if (conv == null) return; // a session this window never opened
    if (e.event is EventBodyTextDelta) {
      _pendingDelta[id] = (_pendingDelta[id] ?? '') + (e.event as EventBodyTextDelta).text;
      if (!_flushScheduled) {
        _flushScheduled = true;
        SchedulerBinding.instance.scheduleFrameCallback((_) => _flushDeltas());
        SchedulerBinding.instance.ensureVisualUpdate();
      }
      return;
    }
    final last = conv.events.isEmpty ? 0 : conv.events.last.seq;
    if (e.seq <= last) return; // replay overlap
    _flushDeltas();
    var streaming = conv.streaming;
    var running = conv.running;
    final b = e.event;
    if (b is EventBodyAssistantText || b is EventBodyToolCall) streaming = '';
    if (b is EventBodyUser) running = true;
    if (b is EventBodyTurnEnd) {
      streaming = '';
      running = false;
    }
    conv.events.add(e);
    _patch(id, streaming: streaming, running: running);
    // The daemon titles a conversation from its first message.
    if (b is EventBodyTurnEnd && e.turn == 1) unawaited(refreshSessions());
  }

  void _flushDeltas() {
    _flushScheduled = false;
    if (_pendingDelta.isEmpty) return;
    final pending = Map.of(_pendingDelta);
    _pendingDelta.clear();
    for (final MapEntry(key: id, value: text) in pending.entries) {
      final c = _cache[id];
      if (c != null) _patch(id, streaming: c.streaming + text);
    }
  }

  @override
  void dispose() {
    for (final off in _offs) {
      off();
    }
    workspace.removeListener(_onWorkspace);
    client.state.removeListener(_onDaemonState);
    super.dispose();
  }
}
