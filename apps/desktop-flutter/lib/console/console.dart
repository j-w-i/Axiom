// The console region (FL-EDIT-002 brings Problems; FL-CONSOLE-001 adds Output and the Terminal):
// a recessed well under the editor, its height and tab remembered, Mod+J toggles it.

import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../app/app_state.dart';
import '../daemon/protocol.g.dart';
import '../editor/lsp/lsp_editor.dart';
import '../state/problems.dart';
import 'output.dart';
import 'terminal_view.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/overlays.dart';
import '../ui/pressable.dart';
import '../ui/roving.dart';
import '../ui/surfaces.dart';
import '../ui/tabs.dart';
import '../ui/theme.dart';
import '../ui/tokens.dart';

class ConsoleRegion extends StatefulWidget {
  const ConsoleRegion({super.key});

  @override
  State<ConsoleRegion> createState() => _ConsoleRegionState();
}

class _ConsoleRegionState extends State<ConsoleRegion> {
  /// The views shown so far: each stays mounted once shown, so switching away keeps the run
  /// prompt's text, the output's scroll and selection and the terminal's state (AUD-074).
  final _shown = <String>{};

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final t = context.axiom;
    final c = t.colors;
    return ListenableBuilder(
      listenable: Listenable.merge([app.prefs, app.problems, app.tasks, app.terminals]),
      builder: (context, _) {
        final raw = app.prefs.string('console.tab', 'problems');
        final tab = const {'output', 'terminal'}.contains(raw) ? raw : 'problems';
        _shown.add(tab);
        final errors = app.problems.count(Severity.error);
        final warnings = app.problems.count(Severity.warning);
        Widget view(String id, Widget child) {
          if (!_shown.contains(id)) return const SizedBox.shrink();
          final on = id == tab;
          // As the side bar keeps its pages: hidden views take no focus, run no tickers and
          // leave the semantics tree.
          return Offstage(
            offstage: !on,
            child: TickerMode(
              enabled: on,
              child: ExcludeFocus(excluding: !on, child: child),
            ),
          );
        }

        return Semantics(
          label: 'Console',
          container: true,
          // Tauri's console: a flat panel with a 30 px head and a seam under it.
          child: ColoredBox(
            color: c.panel,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                SizedBox(
                  height: context.scaled(Metrics.headHeight, fontSize: 12, lineHeight: 16),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: Metrics.s2),
                    child: Row(
                      children: [
                        Flexible(
                          child: AxTabs(
                            label: 'Console views',
                            height: Metrics.headHeight,
                            tabs: [
                              TabItem(
                                id: 'output',
                                label: 'Output',
                                trailing: app.tasks.running
                                    ? const StatusDot(DotState.running)
                                    : null,
                              ),
                              TabItem(
                                id: 'problems',
                                label: 'Problems',
                                trailing: errors + warnings == 0
                                    ? null
                                    : AxBadge(errors + warnings, attention: errors > 0),
                              ),
                              TabItem(
                                id: 'terminal',
                                label: 'Terminal',
                                trailing: app.terminals.terminals.isEmpty
                                    ? null
                                    : AxBadge(app.terminals.terminals.length, attention: false),
                              ),
                            ],
                            selected: tab,
                            onSelect: (id) => app.prefs.set('console.tab', id),
                          ),
                        ),
                        const Spacer(),
                        AxIconButton(
                          icon: AxiomIcons.close,
                          label: 'Hide the console',
                          shortcut: 'Mod+J',
                          size: 26,
                          onPressed: () => app.consoleVisible = false,
                        ),
                      ],
                    ),
                  ),
                ),
                const Hairline(),
                Expanded(
                  child: Stack(
                    fit: StackFit.expand,
                    children: [
                      view('problems', const ProblemsView()),
                      view('output', const OutputView()),
                      view('terminal', const TerminalPane()),
                    ],
                  ),
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}

/// The problems of the open files and the last provider runs: one Tab stop; ↑/↓, Home and End
/// move between rows; Enter opens the file at the problem.
class ProblemsView extends StatefulWidget {
  const ProblemsView({super.key});

  @override
  State<ProblemsView> createState() => _ProblemsViewState();
}

class _ProblemsViewState extends State<ProblemsView> {
  final _scroll = ScrollController();
  final _nodes = RowFocusNodes(debugPrefix: 'problem');
  List<Diagnostic> _list = const [];
  int _current = 0;
  double _extent = 24;
  bool _focused = false;

  static String _id(Diagnostic d) =>
      '${d.severity.name}:${d.location?.file}:${d.location?.line}:${d.location?.column}:'
      '${d.source}:${d.message}';

  @override
  void dispose() {
    _scroll.dispose();
    _nodes.dispose();
    super.dispose();
  }

  void _focus(int i) {
    if (_list.isEmpty) return;
    i = i.clamp(0, _list.length - 1);
    setState(() => _current = i);
    if (_scroll.hasClients) {
      final top = i * _extent + Metrics.s1;
      final view = _scroll.position.viewportDimension;
      if (top < _scroll.offset) {
        _scroll.jumpTo(top);
      } else if (top + _extent > _scroll.offset + view) {
        _scroll.jumpTo(top + _extent - view);
      }
    }
    final id = _id(_list[i]);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _nodes[id].requestFocus();
    });
  }

  KeyEventResult _key(FocusNode _, KeyEvent e) {
    if (e is! KeyDownEvent && e is! KeyRepeatEvent) return KeyEventResult.ignored;
    final to = switch (e.logicalKey) {
      LogicalKeyboardKey.arrowDown => _current + 1,
      LogicalKeyboardKey.arrowUp => _current - 1,
      LogicalKeyboardKey.home => 0,
      LogicalKeyboardKey.end => _list.length - 1,
      _ => null,
    };
    if (to == null) return KeyEventResult.ignored;
    _focus(to);
    return KeyEventResult.handled;
  }

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final t = context.axiom;
    return ListenableBuilder(
      listenable: app.problems,
      builder: (context, _) {
        final list = _list = app.problems.all();
        if (_current >= list.length) _current = list.isEmpty ? 0 : list.length - 1;
        if (list.isEmpty) {
          return Padding(
            padding: const EdgeInsets.all(Metrics.s5),
            child: Text('No problems in the open files.', style: t.type.small),
          );
        }
        _extent = context.scaled(24, fontSize: 12.5);
        return Semantics(
          label: 'Problems, ${list.length}',
          container: true,
          child: RovingFocus(
            onEnter: () => _focus(_current),
            onFocusChange: (v) => setState(() => _focused = v),
            child: Focus(
              canRequestFocus: false,
              skipTraversal: true,
              onKeyEvent: _key,
              child: ListView.builder(
                controller: _scroll,
                itemExtent: _extent,
                padding: const EdgeInsets.symmetric(
                  horizontal: Metrics.sideListInset,
                  vertical: Metrics.s1,
                ),
                itemCount: list.length,
                itemBuilder: (context, i) => ProblemRow(
                  diagnostic: list[i],
                  height: 24,
                  focusNode: _nodes[_id(list[i])],
                  selected: _focused && i == _current,
                  listFocused: _focused,
                  onPressed: () {
                    setState(() => _current = i);
                    final loc = list[i].location;
                    if (loc != null) {
                      unawaited(
                        app.documents.open(loc.file, line: loc.line, column: loc.column ?? 1),
                      );
                    }
                  },
                ),
              ),
            ),
          ),
        );
      },
    );
  }
}

/// One problem: its severity, message, source and place.
class ProblemRow extends StatelessWidget {
  const ProblemRow({
    super.key,
    required this.diagnostic,
    required this.onPressed,
    this.height = 24,
    this.focusNode,
    this.selected = false,
    this.listFocused = false,
  });
  final Diagnostic diagnostic;
  final VoidCallback onPressed;
  final double height;
  final FocusNode? focusNode;
  final bool selected;
  final bool listFocused;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final d = diagnostic;
    final loc = d.location;
    final dot = switch (d.severity) {
      Severity.error => DotState.error,
      Severity.warning => DotState.warn,
      _ => DotState.running,
    };
    return AxListRow(
      label: problemLabel(d),
      hint: loc == null ? null : 'Open ${loc.file}:${loc.line} — Enter',
      height: height,
      focusNode: focusNode,
      skipTraversal: true,
      selected: selected,
      listFocused: listFocused,
      onPressed: loc == null ? null : onPressed,
      child: Row(
        children: [
          const SizedBox(width: Metrics.s3),
          StatusDot(dot),
          const SizedBox(width: Metrics.s4),
          Expanded(
            child: Text(
              d.message,
              style: t.type.body.copyWith(fontSize: 12.5),
              overflow: TextOverflow.ellipsis,
              maxLines: 1,
            ),
          ),
          const SizedBox(width: Metrics.s4),
          Text(d.source, style: t.type.tiny),
          const SizedBox(width: Metrics.s4),
          if (loc != null)
            Flexible(
              flex: 0,
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 280),
                child: Text(
                  '${loc.file}:${loc.line}${loc.column == null ? '' : ':${loc.column}'}',
                  style: t.type.numeric.copyWith(fontSize: 11),
                  overflow: TextOverflow.ellipsis,
                  maxLines: 1,
                ),
              ),
            ),
          const SizedBox(width: Metrics.s3),
        ],
      ),
    );
  }
}

/// The status bar's language-server segment for the active document: its state, and a menu of
/// what can be done about it (restart it; install it when it is missing; AUD-056).
class LspStatus extends StatelessWidget {
  const LspStatus({super.key});

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    return ListenableBuilder(
      listenable: Listenable.merge([app.lsp, app.documents, app.environments]),
      builder: (context, _) {
        final doc = app.documents.active;
        if (doc == null) return const SizedBox.shrink();
        final server = app.lsp.serverFor(lspLanguageId(doc));
        if (server == null) return const SizedBox.shrink();
        final si = app.serverInstalls;
        final env = app.environments;
        final pid = server.installProvider ?? server.providerId;
        final pre = si.prerequisiteOf(pid);
        final target = pre ?? pid;
        final busy = env.installs.containsKey(target) || env.planning == target;
        return LspStatusView(
          serverId: server.serverId,
          state: server.state,
          progress: app.lsp.progress[server.serverId],
          installHint: server.installHint,
          installLabel: busy ? 'Installing ${si.nameOf(target)}…' : 'Install ${si.nameOf(target)}…',
          onRestart: () => unawaited(app.lsp.restart(server.serverId)),
          onInstall: busy ? null : () => unawaited(si.install(target)),
        );
      },
    );
  }
}

/// [LspStatus]'s look and menu, from the server's state.
class LspStatusView extends StatelessWidget {
  const LspStatusView({
    super.key,
    required this.serverId,
    required this.state,
    required this.onRestart,
    this.progress,
    this.installHint,
    this.installLabel,
    this.onInstall,
  });
  final String serverId;
  final ServerState state;
  final String? progress;
  final String? installHint;
  final String? installLabel;
  final VoidCallback onRestart;

  /// Null while an install of it runs (the item shows, disabled).
  final VoidCallback? onInstall;

  static (DotState, String) look(ServerState s) => switch (s) {
    ServerState.ready => (DotState.ok, 'ready'),
    ServerState.starting => (DotState.running, 'starting'),
    ServerState.idle => (DotState.off, 'idle'),
    ServerState.failed => (DotState.error, 'failed'),
    ServerState.missing => (DotState.warn, 'not installed'),
    _ => (DotState.off, s.wire),
  };

  List<MenuEntry> entries() {
    final (_, word) = look(state);
    return [
      MenuEntry.heading('$serverId: $word${progress == null ? '' : ' · $progress'}'),
      if (state == ServerState.missing) ...[
        MenuEntry.item(installLabel ?? 'Install…', onInstall, enabled: onInstall != null),
      ] else
        MenuEntry.item('Restart $serverId', onRestart),
    ];
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final (dot, word) = look(state);
    final label = '$serverId, $word${progress == null ? '' : ', $progress'}';
    return AxMenu(
      label: label,
      hint: state == ServerState.missing
          ? (installHint ?? '$serverId is not installed — Install…')
          : 'Restart $serverId',
      entries: entries,
      trigger: (context, s, open) => FocusRing(
        visible: s.focused,
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: open || s.hovered ? c.hover : null,
            borderRadius: BorderRadius.circular(Metrics.radiusControl),
          ),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: Metrics.s2),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                StatusDot(dot),
                const SizedBox(width: Metrics.s2),
                Text(serverId, style: t.type.tiny),
                if (progress != null) ...[
                  const SizedBox(width: Metrics.s3),
                  ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 200),
                    child: Text(progress!, style: t.type.tiny, overflow: TextOverflow.ellipsis),
                  ),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}
