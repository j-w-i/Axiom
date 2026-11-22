// Source Control (FL-GIT-001; the Tauri shell's SourceControl.tsx and DiffHost.tsx): the side
// bar's branch and changes grouped as Conflicts, Staged, Changes and Untracked (Enter opens a
// row's diff, Shift+Enter the file; ↑/↓ move); and the diff view in the editor area, HEAD on the
// left and the working tree on the right, Esc closes it. Read-only: no staging or commits yet.

import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../app/app_state.dart';
import '../daemon/client.dart';
import '../daemon/protocol.g.dart';
import '../state/git.dart';
import '../ui/controls.dart';
import '../ui/icon_data.dart';
import '../ui/icons.dart';
import '../ui/surfaces.dart';
import '../ui/theme.dart';
import '../ui/tokens.dart';

class ScmSide extends StatefulWidget {
  const ScmSide({super.key});

  @override
  State<ScmSide> createState() => _ScmSideState();
}

class _ScmSideState extends State<ScmSide> {
  final _nodes = <String, FocusNode>{};

  FocusNode _node(String key) => _nodes.putIfAbsent(key, () => FocusNode(debugLabel: key));

  @override
  void dispose() {
    for (final n in _nodes.values) {
      n.dispose();
    }
    super.dispose();
  }

  static const _labels = {
    'conflicts': 'Conflicts',
    'staged': 'Staged',
    'changes': 'Changes',
    'untracked': 'Untracked',
  };

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final t = context.axiom;
    final c = t.colors;
    return ListenableBuilder(
      listenable: Listenable.merge([app.git, app.workspace]),
      builder: (context, _) {
        final s = app.git.status;
        Widget note(String text) => Padding(
          padding: const EdgeInsets.all(Metrics.s5),
          child: Text(text, style: t.type.small),
        );
        if (!app.workspace.isOpen) return note('Open a folder to see its changes.');
        if (s == null) return note('Reading the repository…');
        if (!s.repo) return note(s.reason ?? 'This folder is not in a Git repository.');
        final g = gitGroups(s);
        final sections = [
          ('conflicts', g.conflicts),
          ('staged', g.staged),
          ('changes', g.changes),
          ('untracked', g.untracked),
        ].where((x) => x.$2.isNotEmpty).toList();
        final flat = [
          for (final (k, files) in sections)
            for (final f in files) '$k:${f.path}',
        ];
        KeyEventResult key(String id, GitFile f, KeyEvent e) {
          if (e is! KeyDownEvent) return KeyEventResult.ignored;
          final i = flat.indexOf(id);
          final to = switch (e.logicalKey) {
            LogicalKeyboardKey.arrowDown => i + 1,
            LogicalKeyboardKey.arrowUp => i - 1,
            LogicalKeyboardKey.home => 0,
            LogicalKeyboardKey.end => flat.length - 1,
            _ => null,
          };
          if (e.logicalKey == LogicalKeyboardKey.enter &&
              HardwareKeyboard.instance.isShiftPressed) {
            unawaited(app.documents.open(f.path));
            return KeyEventResult.handled;
          }
          if (to == null || to < 0 || to >= flat.length) return KeyEventResult.ignored;
          _node(flat[to]).requestFocus();
          return KeyEventResult.handled;
        }

        return ListView(
          padding: const EdgeInsets.symmetric(horizontal: Metrics.s2),
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(Metrics.s3, 0, Metrics.s3, Metrics.s3),
              child: Row(
                children: [
                  AxiomIcon(AxiomIcons.branch, size: 14, color: c.ink2),
                  const SizedBox(width: 6),
                  Expanded(
                    child: Text(
                      s.branch ?? 'Detached at ${s.head ?? '?'}',
                      style: t.type.label,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                  if ((s.ahead ?? 0) > 0) Text(' ↑${s.ahead}', style: t.type.numeric),
                  if ((s.behind ?? 0) > 0) Text(' ↓${s.behind}', style: t.type.numeric),
                ],
              ),
            ),
            if (flat.isEmpty) note('No changes'),
            for (final (k, files) in sections)
              Semantics(
                container: true,
                explicitChildNodes: true,
                label: _labels[k],
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Padding(
                      padding: const EdgeInsets.fromLTRB(Metrics.s3, Metrics.s3, Metrics.s3, 4),
                      child: Row(
                        children: [
                          Text(_labels[k]!, style: t.type.tiny),
                          const SizedBox(width: 6),
                          AxBadge(files.length, attention: false),
                        ],
                      ),
                    ),
                    for (final f in files) _row(context, k, f, key),
                  ],
                ),
              ),
          ],
        );
      },
    );
  }

  Widget _row(
    BuildContext context,
    String group,
    GitFile f,
    KeyEventResult Function(String, GitFile, KeyEvent) key,
  ) {
    final app = AppScope.of(context);
    final t = context.axiom;
    final c = t.colors;
    final id = '$group:${f.path}';
    final m = group == 'staged'
        ? markOf(GitFile(path: f.path, index_: f.index_, worktree: GitFileState.unmodified))
        : markOf(
            GitFile(
              path: f.path,
              index_: group == 'changes' ? GitFileState.unmodified : f.index_,
              worktree: f.worktree,
            ),
          );
    final name = f.path.split('/').last;
    final dir = f.path.length > name.length
        ? f.path.substring(0, f.path.length - name.length - 1)
        : '';
    return Focus(
      canRequestFocus: false,
      skipTraversal: true,
      onKeyEvent: (_, e) => key(id, f, e),
      child: AxListRow(
        label: '${f.path}, ${m?.word ?? 'changed'}${group == 'staged' ? ', staged' : ''}',
        hint: 'Show the changes — Enter; open the file — Shift+Enter',
        focusNode: _node(id),
        selected: app.git.diff == f.path,
        height: 26,
        onPressed: () {
          if (HardwareKeyboard.instance.isShiftPressed) {
            unawaited(app.documents.open(f.path));
          } else {
            app.git.openDiff(f.path);
          }
        },
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 8),
          child: Row(
            children: [
              Text(name, style: t.type.body.copyWith(color: gitInk(c, m?.kind))),
              const SizedBox(width: 6),
              Expanded(
                child: Text(dir, style: t.type.tiny, overflow: TextOverflow.ellipsis),
              ),
              Text(m?.letter ?? '', style: t.type.codeSmall.copyWith(color: gitInk(c, m?.kind))),
            ],
          ),
        ),
      ),
    );
  }
}

/// HEAD ↔ the working tree, side by side, for [path]. Esc closes it.
class DiffView extends StatefulWidget {
  const DiffView({super.key, required this.path});
  final String path;

  @override
  State<DiffView> createState() => DiffViewState();
}

class DiffViewState extends State<DiffView> {
  late final AppState app = AppScope.of(context);
  final focusNode = FocusNode(debugLabel: 'diff');
  List<DiffLine>? lines;
  String error = '';

  @override
  void initState() {
    super.initState();
    unawaited(_load());
    app.git.addListener(_onGit);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) focusNode.requestFocus();
    });
  }

  int _version = -1;

  void _onGit() {
    if (app.git.version != _version) unawaited(_load());
  }

  @override
  void didUpdateWidget(DiffView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.path != widget.path) unawaited(_load());
  }

  @override
  void dispose() {
    app.git.removeListener(_onGit);
    focusNode.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    _version = app.git.version;
    final path = widget.path;
    final head = await app.git.headText(path);
    // The working tree: the open document's text when it is open (unsaved edits too).
    final doc = app.documents.docs.where((d) => d.path == path).firstOrNull;
    String now;
    if (doc != null) {
      now = doc.controller.text;
    } else {
      try {
        final r = await app.client.call(
          Methods.fsReadFile,
          ReadFileParams(workspaceId: app.workspace.id, path: path),
        );
        now = r.text;
      } on DaemonException {
        now = ''; // deleted in the working tree
      }
    }
    if (!mounted || path != widget.path) return;
    setState(() {
      lines = diffLines(
        head == null || head.isEmpty ? const [] : head.split('\n'),
        now.isEmpty ? const [] : now.split('\n'),
      );
    });
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final l = lines;
    final added = l?.where((x) => x.op == DiffOp.added).length ?? 0;
    final removed = l?.where((x) => x.op == DiffOp.removed).length ?? 0;
    return Focus(
      focusNode: focusNode,
      onKeyEvent: (_, e) {
        if (e is KeyDownEvent && e.logicalKey == LogicalKeyboardKey.escape) {
          app.git.closeDiff();
          return KeyEventResult.handled;
        }
        return KeyEventResult.ignored;
      },
      child: Semantics(
        container: true,
        explicitChildNodes: true,
        label: 'Changes to ${widget.path}: $added added, $removed removed',
        child: ColoredBox(
          color: c.ground,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              SizedBox(
                height: 32,
                child: ColoredBox(
                  color: c.bar,
                  child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: Metrics.s4),
                    child: Row(
                      children: [
                        Text(widget.path, style: t.type.label),
                        const SizedBox(width: Metrics.s3),
                        Text('HEAD ↔ working tree', style: t.type.tiny),
                        const SizedBox(width: Metrics.s3),
                        Text('+$added', style: t.type.numeric.copyWith(color: c.okText)),
                        const SizedBox(width: 6),
                        Text('−$removed', style: t.type.numeric.copyWith(color: c.dangerText)),
                        const Spacer(),
                        AxButton(
                          label: 'Open the file',
                          kind: ButtonKind.quiet,
                          onPressed: () {
                            app.git.closeDiff();
                            unawaited(app.documents.open(widget.path));
                          },
                        ),
                        AxIconButton(
                          icon: AxiomIcons.close,
                          label: 'Close the diff',
                          shortcut: 'Esc',
                          size: 24,
                          onPressed: app.git.closeDiff,
                        ),
                      ],
                    ),
                  ),
                ),
              ),
              const Groove(),
              Expanded(
                child: l == null
                    ? const Center(child: Spinner(label: 'Reading the changes'))
                    : _SideBySide(lines: l),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _SideBySide extends StatelessWidget {
  const _SideBySide({required this.lines});
  final List<DiffLine> lines;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final code = t.type.code.copyWith(fontSize: 12.5, color: c.syntax.text, height: 1.5);
    final num = t.type.numeric.copyWith(fontSize: 11, color: c.ink3);
    // Pairs: a removed line beside the added line that replaced it.
    final rows = <(DiffLine?, DiffLine?)>[];
    var i = 0;
    while (i < lines.length) {
      final l = lines[i];
      if (l.op == DiffOp.same) {
        rows.add((l, l));
        i++;
        continue;
      }
      final del = <DiffLine>[], add = <DiffLine>[];
      while (i < lines.length && lines[i].op != DiffOp.same) {
        (lines[i].op == DiffOp.removed ? del : add).add(lines[i]);
        i++;
      }
      for (var k = 0; k < (del.length > add.length ? del.length : add.length); k++) {
        rows.add((k < del.length ? del[k] : null, k < add.length ? add[k] : null));
      }
    }
    Widget side(DiffLine? d, bool left) {
      final n = d == null ? null : (left ? d.left : d.right);
      final changed = d != null && d.op != DiffOp.same;
      return Expanded(
        child: ColoredBox(
          color: !changed
              ? (d == null ? c.recessed : c.ground)
              : (left ? c.danger : c.ok).withValues(alpha: 0.12),
          child: Row(
            children: [
              SizedBox(
                width: 44,
                child: Text('${n ?? ''}', style: num, textAlign: TextAlign.right),
              ),
              const SizedBox(width: 6),
              SizedBox(
                width: 12,
                child: Text(
                  changed ? (left ? '−' : '+') : '',
                  style: code.copyWith(color: left ? c.dangerText : c.okText),
                ),
              ),
              Expanded(
                child: Text(
                  d?.text ?? '',
                  style: code,
                  maxLines: 1,
                  softWrap: false,
                  overflow: TextOverflow.clip,
                ),
              ),
            ],
          ),
        ),
      );
    }

    return ListView.builder(
      itemExtent: 19,
      itemCount: rows.length,
      itemBuilder: (context, k) {
        final (l, r) = rows[k];
        final what = l != null && r != null && l.op == DiffOp.same
            ? 'unchanged: ${l.text}'
            : [
                if (l != null) 'removed line ${l.left}: ${l.text}',
                if (r != null) 'added line ${r.right}: ${r.text}',
              ].join('; ');
        return Semantics(
          label: what,
          excludeSemantics: true,
          child: Row(
            children: [
              side(l, true),
              SizedBox(width: 1, child: ColoredBox(color: c.hairline)),
              side(r, false),
            ],
          ),
        );
      },
    );
  }
}
