// The workspace's language servers as the UI sees them (FL-EDIT-002, a port of the Tauri shell's
// LspStore; ADR-0018 §2): their state (the status bar), their diagnostics (Problems and the
// editor's underlines), progress, and requests through the daemon (`lsp/*`).

import 'dart:async';

import 'package:flutter/foundation.dart';

import '../daemon/client.dart';
import '../daemon/protocol.g.dart';
import 'workspace.dart';

/// An LSP position (0-based line and UTF-16 character).
typedef LspPos = ({int line, int character});

/// An LSP range.
typedef LspRange = ({LspPos start, LspPos end});

LspPos lspPos(Object? json) {
  final m = json! as Map;
  return (line: (m['line'] as num).toInt(), character: (m['character'] as num).toInt());
}

LspRange lspRange(Object? json) {
  final m = json! as Map;
  return (start: lspPos(m['start']), end: lspPos(m['end']));
}

Map<String, Object?> posJson(LspPos p) => {'line': p.line, 'character': p.character};

/// One published diagnostic, as the editor draws it.
final class LspDiagnostic {
  const LspDiagnostic({
    required this.range,
    required this.severity,
    required this.message,
    this.source,
    this.code,
  });
  final LspRange range;

  /// 1 error, 2 warning, 3 information, 4 hint (LSP).
  final int severity;
  final String message;
  final String? source;
  final String? code;

  static LspDiagnostic fromJson(Object? json) {
    final m = json! as Map;
    final c = m['code'];
    return LspDiagnostic(
      range: lspRange(m['range']),
      severity: (m['severity'] as num?)?.toInt() ?? 1,
      message: m['message'] as String? ?? '',
      source: m['source'] as String?,
      code: c == null ? null : (c is Map ? '${c['value']}' : '$c'),
    );
  }
}

/// `file://` URI of an absolute path, each segment percent-encoded.
String fileUri(String absPath) {
  var p = absPath.replaceAll(r'\', '/');
  if (RegExp(r'^[A-Za-z]:/').hasMatch(p)) p = '/$p';
  final parts = p.split('/');
  return 'file://${[for (var i = 0; i < parts.length; i++) (i == 1 && RegExp(r'^[A-Za-z]:$').hasMatch(parts[i])) ? parts[i] : Uri.encodeComponent(parts[i])].join('/')}';
}

/// The workspace path of [uri] under [rootUri], or null outside it.
String? relativeTo(String uri, String rootUri) {
  String dec(String u) {
    try {
      return Uri.decodeComponent(u);
    } on Object {
      return u;
    }
  }

  final root = dec(rootUri.replaceAll(RegExp(r'/+$'), ''));
  final a = dec(uri);
  if (!a.startsWith('$root/')) return null;
  return a.substring(root.length + 1);
}

Severity severityOf(int lsp) => switch (lsp) {
  2 => Severity.warning,
  3 => Severity.info,
  4 => Severity.hint,
  _ => Severity.error,
};

class LspStore extends ChangeNotifier {
  LspStore(this.client, this.workspace) {
    _offs = [
      workspace.onOpened(() => unawaited(load())),
      workspace.onClosed(_clear),
      client.on(Notifications.lspServerState, _onServerState),
      client.on(Notifications.lspNotification, _onNotification),
    ];
  }

  final DaemonClient client;
  final WorkspaceStore workspace;
  late final List<void Function()> _offs;

  List<ServerInfo> servers = const [];

  /// serverId → workspace path → its diagnostics.
  final diagnostics = <String, Map<String, List<LspDiagnostic>>>{};

  /// serverId → the running `$/progress` title ("Indexing…").
  final progress = <String, String>{};
  int _loadSeq = 0;

  /// Bumped when any diagnostics change (the editor redraws its underlines).
  int diagnosticsVersion = 0;

  String get rootUri => fileUri(workspace.root);
  String uriOf(String path) => '$rootUri/${path.split('/').map(Uri.encodeComponent).join('/')}';
  String? pathOf(String uri) => relativeTo(uri, rootUri);

  void _clear() {
    servers = const [];
    diagnostics.clear();
    progress.clear();
    diagnosticsVersion++;
    notifyListeners();
  }

  Future<void> load() async {
    final seq = ++_loadSeq;
    if (!workspace.isOpen) return;
    try {
      final r = await client.call(Methods.lspServers, WorkspaceRef(workspaceId: workspace.id));
      if (seq != _loadSeq) return;
      servers = r.servers;
      notifyListeners();
    } on DaemonException {
      // No language servers.
    }
  }

  /// The server for an LSP language id, if any.
  ServerInfo? serverFor(String? languageId) {
    if (languageId == null) return null;
    final base = switch (languageId) {
      'typescriptreact' => 'typescript',
      'javascriptreact' => 'javascript',
      _ => languageId,
    };
    for (final s in servers) {
      // A standby server (another serves the language, ADR-0021 §3) is never used.
      if (s.state == ServerState.standby) continue;
      if (s.languages.contains(languageId) || s.languages.contains(base)) return s;
    }
    return null;
  }

  /// One request through the daemon; [cancel] cancels it (`$/cancel`).
  Future<Object?> request(
    String serverId,
    String method,
    Object? params, {
    CancelToken? cancel,
  }) async {
    final r = await client.call(
      Methods.lspRequest,
      RequestParams(workspaceId: workspace.id, serverId: serverId, method: method, params: params),
      cancel: cancel,
      timeout: const Duration(seconds: 30),
    );
    return r.result;
  }

  Future<void> notify(String serverId, String method, Object? params) => client.call(
    Methods.lspNotify,
    NotifyParams(workspaceId: workspace.id, serverId: serverId, method: method, params: params),
  );

  /// After an install (a server, or a runtime or SDK one needs): every `missing` server tries
  /// again, then the list is read anew (M5.5-UI-001).
  Future<void> retryMissing() async {
    if (!workspace.isOpen) return;
    for (final s in servers.where((s) => s.state == ServerState.missing)) {
      try {
        await restart(s.serverId);
      } on DaemonException {
        // Still missing: its line says so.
      }
    }
    await load();
  }

  Future<void> restart(String serverId) =>
      client.call(Methods.lspRestart, ServerRef(workspaceId: workspace.id, serverId: serverId));

  /// Every server's diagnostics as Problems rows.
  List<Diagnostic> problems() => [
    for (final e in diagnostics.entries)
      for (final f in e.value.entries)
        for (final d in f.value)
          Diagnostic(
            severity: severityOf(d.severity),
            message: d.message,
            // "rustc (rust-analyzer)": what the server says produced it, then the server.
            source: d.source == null ? e.key : '${d.source} (${e.key})',
            code: d.code,
            location: Location(
              file: f.key,
              line: d.range.start.line + 1,
              column: d.range.start.character + 1,
              endLine: d.range.end.line + 1,
              endColumn: d.range.end.character + 1,
            ),
          ),
  ];

  /// The diagnostics on [path], from every server.
  List<LspDiagnostic> diagnosticsOn(String path) => [
    for (final byFile in diagnostics.values) ...?byFile[path],
  ];

  void _onServerState(ServerStateParams p) {
    if (p.workspaceId != workspace.id) return;
    final i = servers.indexWhere((s) => s.serverId == p.server.serverId);
    servers = i < 0
        ? [...servers, p.server]
        : [for (var j = 0; j < servers.length; j++) j == i ? p.server : servers[j]];
    final running = p.server.state == ServerState.ready || p.server.state == ServerState.starting;
    if (!running) {
      // A server that is not running has no diagnostics or progress.
      if (diagnostics.remove(p.server.serverId) != null) diagnosticsVersion++;
      progress.remove(p.server.serverId);
    }
    notifyListeners();
  }

  void _onNotification(LspNotificationParams n) {
    if (n.workspaceId != workspace.id) return;
    if (n.method == 'textDocument/publishDiagnostics') {
      final p = n.params! as Map;
      final path = pathOf(p['uri'] as String);
      if (path == null) return;
      final list = [
        for (final d in (p['diagnostics'] as List? ?? const [])) LspDiagnostic.fromJson(d),
      ];
      final mine = diagnostics.putIfAbsent(n.serverId, () => {});
      if (list.isEmpty) {
        mine.remove(path);
      } else {
        mine[path] = list;
      }
      diagnosticsVersion++;
      notifyListeners();
    } else if (n.method == r'$/progress') {
      final v = (n.params as Map?)?['value'];
      if (v is! Map) return;
      if (v['kind'] == 'begin' && v['title'] is String) {
        progress[n.serverId] = v['title'] as String;
        notifyListeners();
      } else if (v['kind'] == 'end' && progress.remove(n.serverId) != null) {
        notifyListeners();
      }
    }
  }

  @override
  void dispose() {
    for (final off in _offs) {
      off();
    }
    super.dispose();
  }
}
