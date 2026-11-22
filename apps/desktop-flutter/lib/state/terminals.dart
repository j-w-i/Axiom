// The folder's integrated terminals (FL-CONSOLE-001; ADR-0018 §4, ADR-0020 §3): the daemon runs
// them (`terminal/*`); this store lists them, keeps which one is shown, and owns one `xterm`
// screen per terminal, so switching console views keeps each screen. Keys typed into a screen go
// to `terminal/write`; its size goes to `terminal/resize`. `terminal/list` brings the daemon's
// terminals back with their backlog when the folder opens again.

import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:xterm/xterm.dart';

import '../daemon/client.dart';
import '../daemon/protocol.g.dart';
import 'workspace.dart';

final class TerminalTab {
  TerminalTab(this.info, this.screen);
  TerminalInfo info;
  final Terminal screen;
  int lastSeq = 0;
}

class TerminalsStore extends ChangeNotifier {
  TerminalsStore(this.client, this.workspace) {
    _offs = [
      client.on(Notifications.terminalOutput, _onOutput),
      client.on(Notifications.terminalExited, (p) {
        final t = _byId(p.terminalId);
        if (t == null) return;
        t.info = TerminalInfo(
          terminalId: t.info.terminalId,
          title: t.info.title,
          running: false,
          exitCode: p.code,
          pid: t.info.pid,
          workspaceId: t.info.workspaceId,
        );
        notifyListeners();
      }),
    ];
    workspace.addListener(_onWorkspace);
  }

  final DaemonClient client;
  final WorkspaceStore workspace;
  late final List<void Function()> _offs;

  final terminals = <TerminalTab>[];
  String? active;

  /// A create that failed, for the view to say.
  String error = '';

  /// Output of a terminal whose create has not answered yet (its first prompt).
  final _early = <String, List<OutputParams2>>{};
  String? _lastWs;

  TerminalTab? _byId(String id) => terminals.where((t) => t.info.terminalId == id).firstOrNull;
  TerminalTab? get activeTab => active == null ? null : _byId(active!);

  void _onWorkspace() {
    final id = workspace.isOpen ? workspace.id : null;
    if (id == _lastWs) return;
    _lastWs = id;
    terminals.clear();
    _early.clear();
    active = null;
    error = '';
    notifyListeners();
    if (id != null) unawaited(reattach());
  }

  void _onOutput(OutputParams2 p) {
    final t = _byId(p.terminalId);
    if (t == null) {
      (_early[p.terminalId] ??= []).add(p);
      return;
    }
    if (p.seq <= t.lastSeq) return; // already in the backlog
    t.lastSeq = p.seq;
    t.screen.write(p.data);
  }

  TerminalTab _tab(TerminalInfo info, {String backlog = '', int seq = 0}) {
    final id = info.terminalId;
    final screen = Terminal(
      maxLines: 10000,
      onOutput: (data) => write(id, data),
      onResize: (cols, rows, _, _) => resize(id, cols, rows),
    );
    final tab = TerminalTab(info, screen)..lastSeq = seq;
    if (backlog.isNotEmpty) screen.write(backlog);
    for (final e in _early.remove(id) ?? const <OutputParams2>[]) {
      if (e.seq > tab.lastSeq) {
        tab.lastSeq = e.seq;
        screen.write(e.data);
      }
    }
    return tab;
  }

  /// The daemon's terminals of this folder, with their screens.
  Future<void> reattach() async {
    if (!workspace.isOpen) return;
    try {
      final r = await client.call(Methods.terminalList, WorkspaceRef(workspaceId: workspace.id));
      for (final t in r.terminals) {
        if (_byId(t.terminalId) != null) continue;
        terminals.add(
          _tab(
            TerminalInfo(
              terminalId: t.terminalId,
              title: t.title,
              running: t.running,
              exitCode: t.exitCode,
              pid: t.pid,
              workspaceId: t.workspaceId,
            ),
            backlog: t.backlog,
            seq: t.seq,
          ),
        );
      }
      active ??= terminals.lastOrNull?.info.terminalId;
      notifyListeners();
    } on DaemonException {
      // No terminals.
    }
  }

  Future<String?> create({int cols = 80, int rows = 24}) async {
    if (!workspace.isOpen) return null;
    try {
      final t = await client.call(
        Methods.terminalCreate,
        CreateParams(
          workspaceId: workspace.id,
          cols: cols < 1 ? 1 : cols,
          rows: rows < 1 ? 1 : rows,
        ),
      );
      terminals.add(_tab(t));
      active = t.terminalId;
      error = '';
      notifyListeners();
      return t.terminalId;
    } on DaemonException catch (e) {
      error = e.message;
      notifyListeners();
      return null;
    }
  }

  /// Input waiting while a write is in flight, by terminal.
  final _queued = <String, StringBuffer>{};
  final _writing = <String>{};

  /// Keystrokes go one write at a time per terminal, in order; what is typed meanwhile is sent
  /// as one write. The daemon runs each request in a task of its own, so two writes in flight
  /// could reach the PTY in either order (AUD-087).
  void write(String id, String data) {
    (_queued[id] ??= StringBuffer()).write(data);
    if (_writing.contains(id)) return;
    unawaited(_drain(id));
  }

  Future<void> _drain(String id) async {
    _writing.add(id);
    try {
      while (true) {
        final buf = _queued[id];
        if (buf == null || buf.isEmpty) break;
        final data = buf.toString();
        buf.clear();
        try {
          await client.call(Methods.terminalWrite, WriteParams(terminalId: id, data: data));
        } on DaemonException {
          _queued.remove(id);
          break;
        }
      }
    } finally {
      _writing.remove(id);
    }
  }

  void resize(String id, int cols, int rows) {
    if (cols < 1 || rows < 1) return;
    unawaited(
      client
          .call(Methods.terminalResize, ResizeParams(terminalId: id, cols: cols, rows: rows))
          .then((_) {}, onError: (_) {}),
    );
  }

  void activate(String id) {
    if (_byId(id) == null) return;
    active = id;
    notifyListeners();
  }

  Future<void> close(String id) async {
    try {
      await client.call(Methods.terminalClose, TerminalRef(terminalId: id));
    } on DaemonException {
      // Already gone.
    }
    final i = terminals.indexWhere((t) => t.info.terminalId == id);
    if (i < 0) return;
    terminals.removeAt(i);
    if (active == id) {
      active = terminals.isEmpty
          ? null
          : terminals[i < terminals.length ? i : terminals.length - 1].info.terminalId;
    }
    notifyListeners();
  }

  /// The tab's name: the program, numbered ("zsh 2"), and "(ended)" after exit.
  String label(TerminalTab t) {
    final same = terminals.where((x) => x.info.title == t.info.title).toList();
    final n = same.length > 1 ? ' ${same.indexOf(t) + 1}' : '';
    return '${t.info.title}$n${t.info.running ? '' : ' (ended)'}';
  }

  @override
  void dispose() {
    for (final off in _offs) {
      off();
    }
    workspace.removeListener(_onWorkspace);
    super.dispose();
  }
}
