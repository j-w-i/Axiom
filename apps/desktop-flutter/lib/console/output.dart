// Output (FL-CONSOLE-001): the run prompt (Enter runs, ↑↓ walk the history), the task's output
// with stdout and stderr told apart, its exit status, Stop, Rerun and Clear; long lines wrap
// (a toggle; AUD-058); and the title row's transport (what runs, for how long, how fast it
// writes, Stop; Rerun when idle).

import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../app/app_state.dart';
import '../daemon/protocol.g.dart';
import '../state/tasks.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/text_input.dart';
import '../ui/theme.dart';
import '../ui/tokens.dart';

class OutputView extends StatefulWidget {
  const OutputView({super.key});

  @override
  State<OutputView> createState() => OutputViewState();
}

class OutputViewState extends State<OutputView> {
  late final AppState app = AppScope.of(context);
  final command = TextEditingController();
  final commandFocus = FocusNode(debugLabel: 'run prompt');
  final _scroll = ScrollController();
  final _selection = FocusNode(skipTraversal: true, canRequestFocus: false);
  bool _stick = true;
  int _historyAt = -1;

  @override
  void initState() {
    super.initState();
    app.focusRunPrompt = () => WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) commandFocus.requestFocus();
    });
    _scroll.addListener(() {
      if (_scroll.hasClients) {
        _stick = _scroll.position.pixels >= _scroll.position.maxScrollExtent - 24;
      }
    });
    app.tasks.addListener(_follow);
  }

  @override
  void dispose() {
    app.tasks.removeListener(_follow);
    app.focusRunPrompt = null;
    command.dispose();
    commandFocus.dispose();
    _scroll.dispose();
    _selection.dispose();
    super.dispose();
  }

  void _follow() {
    if (!_stick) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scroll.hasClients) _scroll.jumpTo(_scroll.position.maxScrollExtent);
    });
  }

  void _run() {
    final text = command.text;
    if (text.trim().isEmpty || !app.tasks.canRun) return;
    _stick = true;
    _historyAt = -1;
    app.tasks.run(text);
  }

  KeyEventResult _key(FocusNode _, KeyEvent e) {
    if (e is! KeyDownEvent) return KeyEventResult.ignored;
    final h = app.tasks.history;
    switch (e.logicalKey) {
      case LogicalKeyboardKey.enter || LogicalKeyboardKey.numpadEnter:
        _run();
        return KeyEventResult.handled;
      case LogicalKeyboardKey.arrowUp when h.isNotEmpty:
        _historyAt = (_historyAt + 1).clamp(0, h.length - 1);
        _show(h[_historyAt]);
        return KeyEventResult.handled;
      case LogicalKeyboardKey.arrowDown when _historyAt >= 0:
        _historyAt--;
        _show(_historyAt < 0 ? '' : h[_historyAt]);
        return KeyEventResult.handled;
      default:
        return KeyEventResult.ignored;
    }
  }

  void _show(String s) {
    command.text = s;
    command.selection = TextSelection.collapsed(offset: s.length);
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final tasks = app.tasks;
    return ListenableBuilder(
      listenable: Listenable.merge([tasks, app.workspace, app.prefs]),
      builder: (context, _) {
        final rows = tasks.rows;
        final wrap = app.prefs.flag(wrapKey, true);
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(Metrics.s4, Metrics.s3, Metrics.s4, Metrics.s3),
              child: Row(
                children: [
                  Expanded(
                    child: Focus(
                      canRequestFocus: false,
                      skipTraversal: true,
                      onKeyEvent: _key,
                      child: AxTextInput(
                        label: 'Command',
                        controller: command,
                        focusNode: commandFocus,
                        mono: true,
                        placeholder: app.workspace.isOpen
                            ? 'A command to run in ${app.workspace.name} (Enter; ↑ for history)'
                            : 'Open a folder to run commands',
                        leading: Text('›', style: t.type.code.copyWith(color: c.accentText)),
                      ),
                    ),
                  ),
                  const SizedBox(width: Metrics.s3),
                  if (tasks.running)
                    AxButton(
                      label: 'Stop',
                      icon: AxiomIcons.stop,
                      hint: 'Stop the running command — Shift+F5',
                      onPressed: tasks.stop,
                    )
                  else
                    AxButton(
                      label: 'Run',
                      icon: AxiomIcons.play,
                      hint: 'Run the command — Enter',
                      onPressed: tasks.canRun ? _run : null,
                    ),
                  const SizedBox(width: Metrics.s2),
                  AxIconButton(
                    icon: AxiomIcons.wrap,
                    label: wrap ? 'Wrap long lines: on' : 'Wrap long lines: off',
                    selected: wrap,
                    onPressed: () => app.prefs.set(wrapKey, !wrap),
                  ),
                  const SizedBox(width: Metrics.s2),
                  AxButton(
                    label: 'Clear',
                    kind: ButtonKind.quiet,
                    hint: 'Clear the output',
                    onPressed: rows.isEmpty ? null : tasks.clear,
                  ),
                ],
              ),
            ),
            Expanded(
              child: Semantics(
                container: true,
                explicitChildNodes: true,
                label: 'Output',
                child: rows.isEmpty
                    ? Padding(
                        padding: const EdgeInsets.all(Metrics.s4),
                        child: Text(
                          tasks.statusText.isEmpty ? 'No output yet.' : tasks.statusText,
                          style: t.type.small,
                        ),
                      )
                    : SelectableRegion(
                        selectionControls: emptyTextSelectionControls,
                        focusNode: _selection,
                        child: ListView.builder(
                          controller: _scroll,
                          // Wrapped rows vary in height; unwrapped ones are one line each.
                          itemExtent: wrap
                              ? null
                              : context.scaled(19, fontSize: 12, lineHeight: 17.4),
                          padding: const EdgeInsets.fromLTRB(Metrics.s4, 0, Metrics.s4, Metrics.s4),
                          itemCount: rows.length + (tasks.dropped > 0 ? 1 : 0),
                          itemBuilder: (context, i) {
                            if (tasks.dropped > 0 && i == 0) {
                              return Text(
                                '… ${tasks.dropped} earlier lines dropped',
                                style: t.type.codeSmall.copyWith(color: c.ink3),
                              );
                            }
                            final r = rows[i - (tasks.dropped > 0 ? 1 : 0)];
                            return OutputLine(row: r, wrap: wrap);
                          },
                        ),
                      ),
              ),
            ),
          ],
        );
      },
    );
  }
}

/// The preference that wraps Output's long lines (on unless turned off).
const wrapKey = 'output.wrap';

class OutputLine extends StatelessWidget {
  const OutputLine({super.key, required this.row, this.wrap = true});
  final OutputRow row;

  /// Long lines wrap (all of the text shows); off, a line is clipped at the view's edge.
  final bool wrap;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final style = t.type.codeSmall.copyWith(
      height: 1.45,
      color: switch (row.kind) {
        RowKind.status => c.okText,
        RowKind.statusFailed || RowKind.error => c.dangerText,
        RowKind.line => row.stream == StreamKind.stderr ? c.warnText : c.ink,
      },
      fontWeight: row.kind == RowKind.line ? null : FontWeight.w600,
    );
    final prefix = row.stream == StreamKind.stderr ? 'stderr: ' : '';
    return Semantics(
      label: '$prefix${row.text}',
      excludeSemantics: true,
      child: Text(
        row.text,
        style: style,
        maxLines: wrap ? null : 1,
        softWrap: wrap,
        overflow: wrap ? null : TextOverflow.clip,
      ),
    );
  }
}

/// The title row's transport: what runs, its elapsed time, a 16-bar rate meter of its output,
/// and Stop; when idle, Rerun with the last command's name; the last outcome as a lamp.
class Transport extends StatefulWidget {
  const Transport({super.key});

  @override
  State<Transport> createState() => TransportState();
}

class TransportState extends State<Transport> {
  late final AppState app = AppScope.of(context);
  Timer? _tick;
  final _rates = List<int>.filled(16, 0, growable: true);
  int _lastSeq = 0;

  /// `run`, `on` (last run succeeded), `err` (failed), or `idle`.
  String get state => app.tasks.running
      ? 'run'
      : switch (app.tasks.lastOutcome) {
          Outcome.succeeded => 'on',
          Outcome.failed => 'err',
          _ => 'idle',
        };

  /// mm:ss while running.
  String? get elapsed {
    final s = app.tasks.startedAt;
    if (!app.tasks.running || s == null) return null;
    final d = DateTime.now().difference(DateTime.fromMillisecondsSinceEpoch(s));
    final secs = d.inSeconds < 0 ? 0 : d.inSeconds;
    return '${(secs ~/ 60).toString().padLeft(2, '0')}:${(secs % 60).toString().padLeft(2, '0')}';
  }

  @override
  void initState() {
    super.initState();
    app.tasks.addListener(_changed);
    app.environments.addListener(_changed);
    _window.addListener(_changed);
  }

  final _window = WindowActivity.instance.active;
  bool _tickActive = true;

  /// The meter samples at 4 Hz while the window is active; in the background only the elapsed
  /// time moves, once a second, and the meter rests (performance.md P9; AUD-080).
  void _changed() {
    final active = _window.value;
    if (app.tasks.running && (_tick == null || _tickActive != active)) {
      _tick?.cancel();
      if (_tick == null) {
        _rates.fillRange(0, _rates.length, 0);
        _lastSeq = app.tasks.outputSeq;
      }
      _tickActive = active;
      _tick = Timer.periodic(Duration(milliseconds: active ? 250 : 1000), (_) {
        if (!mounted) return;
        final seq = app.tasks.outputSeq;
        setState(() {
          if (_tickActive) {
            _rates
              ..removeAt(0)
              ..add(seq - _lastSeq);
          }
          _lastSeq = seq;
        });
      });
    } else if (!app.tasks.running) {
      _tick?.cancel();
      _tick = null;
    }
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    _tick?.cancel();
    _window.removeListener(_changed);
    app.tasks.removeListener(_changed);
    app.environments.removeListener(_changed);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final tasks = app.tasks;
    final st = state;
    final install = app.environments.activeInstall;
    if (install != null && !tasks.running) {
      // A toolchain install: its name, its step, and Cancel.
      return Semantics(
        container: true,
        explicitChildNodes: true,
        label: 'Installing ${install.name}',
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            StatusDot(DotState.running, ring: c.bar),
            const SizedBox(width: Metrics.s3),
            Text('Installing ${install.name}', style: t.type.small.copyWith(color: c.ink2)),
            if (install.progress != null) ...[
              const SizedBox(width: Metrics.s3),
              Text(
                '${install.progress!.step + 1}/${install.progress!.steps}',
                style: t.type.numeric,
              ),
            ],
            AxIconButton(
              icon: AxiomIcons.stop,
              label: 'Cancel installing ${install.name}',
              size: 24,
              onPressed: () => app.environments.cancelInstall(install.providerId),
            ),
          ],
        ),
      );
    }
    if (!tasks.running && tasks.lastLabel.isEmpty) return const SizedBox.shrink();
    final lamp = switch (st) {
      'run' => DotState.running,
      'on' => DotState.ok,
      'err' => DotState.error,
      _ => DotState.off,
    };
    final top = _rates.fold(1, (a, b) => a > b ? a : b);
    final time = elapsed;
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: tasks.running ? 'Running ${tasks.label}${time == null ? '' : ', $time'}' : 'Tasks',
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          StatusDot(lamp, ring: c.bar),
          const SizedBox(width: Metrics.s3),
          ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 220),
            child: Text(
              tasks.running ? tasks.label : tasks.statusText,
              style: t.type.small.copyWith(color: c.ink2),
              overflow: TextOverflow.ellipsis,
            ),
          ),
          if (time != null) ...[
            const SizedBox(width: Metrics.s3),
            Text(time, style: t.type.numeric),
          ],
          if (tasks.running) ...[
            const SizedBox(width: Metrics.s3),
            ExcludeSemantics(
              child: SizedBox(
                width: 16 * 3.0,
                height: 14,
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.end,
                  children: [
                    for (final r in _rates)
                      Padding(
                        padding: const EdgeInsets.only(right: 1),
                        child: SizedBox(
                          width: 2,
                          height: 2 + 12 * r / top,
                          child: ColoredBox(color: c.accent),
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ],
          const SizedBox(width: Metrics.s2),
          if (tasks.running)
            AxIconButton(
              icon: AxiomIcons.stop,
              label: 'Stop',
              shortcut: 'Shift+F5',
              size: 24,
              onPressed: tasks.stop,
            )
          else
            AxIconButton(
              icon: AxiomIcons.refresh,
              label: 'Rerun: ${tasks.lastLabel}',
              shortcut: 'F5',
              size: 24,
              onPressed: tasks.canRun ? tasks.rerunLast : null,
            ),
        ],
      ),
    );
  }
}
