// Source Control (FL-GIT-001; the Tauri shell's SourceControl.tsx and DiffHost.tsx): the side
// bar's branch and changes grouped as Conflicts, Staged, Changes and Untracked (Enter opens a
// row's diff, Shift+Enter the file; ↑/↓ move; one Tab stop, built lazily); and the diff view in
// the editor area, HEAD on the left and the working tree on the right (aligned off the UI isolate
// for large files, long lines wrapped), Esc closes it. Read-only: no staging or commits yet.

import 'dart:async';
import 'dart:isolate';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../app/app_state.dart';
import '../daemon/client.dart';
import '../daemon/protocol.g.dart';
import '../state/git.dart';
import '../ui/controls.dart';
import '../ui/icon_data.dart';
import '../ui/icons.dart';
import '../ui/roving.dart';
import '../ui/surfaces.dart';
import '../ui/theme.dart';
import '../ui/tokens.dart';

class ScmSide extends StatefulWidget {
  const ScmSide({super.key});

  @override
  State<ScmSide> createState() => _ScmSideState();
}

/// One item of the changes list: the branch line, a group's heading, or a changed file.
typedef _Item = ({String kind, String group, GitFile? file, int k});

class _ScmSideState extends State<ScmSide> {
  final _nodes = RowFocusNodes(debugPrefix: 'change');
  final _scroll = ScrollController();
  List<_Item> _items = const [];
  List<int> _rowAt = const [];
  int _current = 0;
  bool _focused = false;
  double _head = 26, _row = 26, _branch = 28;

  @override
  void dispose() {
    _nodes.dispose();
    _scroll.dispose();
    super.dispose();
  }

  static const _labels = {
    'conflicts': 'Conflicts',
    'staged': 'Staged',
    'changes': 'Changes',
    'untracked': 'Untracked',
  };

  static String _id(String group, GitFile f) => '$group:${f.path}';

  double _extent(int i) => switch (_items[i].kind) {
    'branch' => _branch,
    'heading' => _head,
    _ => _row,
  };

  void _focus(int n) {
    if (_rowAt.isEmpty) return;
    n = n.clamp(0, _rowAt.length - 1);
    setState(() => _current = n);
    final index = _rowAt[n];
    if (_scroll.hasClients) {
      var top = 0.0;
      for (var i = 0; i < index; i++) {
        top += _extent(i);
      }
      final view = _scroll.position.viewportDimension;
      final max = _scroll.position.maxScrollExtent;
      if (top < _scroll.offset) {
        _scroll.jumpTo(top.clamp(0, max));
      } else if (top + _row > _scroll.offset + view) {
        _scroll.jumpTo((top + _row - view).clamp(0, max));
      }
    }
    final it = _items[index];
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _nodes[_id(it.group, it.file!)].requestFocus();
    });
  }

  KeyEventResult _key(AppState app, KeyEvent e) {
    if (e is! KeyDownEvent && e is! KeyRepeatEvent) return KeyEventResult.ignored;
    if (e.logicalKey == LogicalKeyboardKey.enter && HardwareKeyboard.instance.isShiftPressed) {
      if (_current < _rowAt.length) {
        unawaited(app.documents.open(_items[_rowAt[_current]].file!.path));
      }
      return KeyEventResult.handled;
    }
    final to = switch (e.logicalKey) {
      LogicalKeyboardKey.arrowDown => _current + 1,
      LogicalKeyboardKey.arrowUp => _current - 1,
      LogicalKeyboardKey.home => 0,
      LogicalKeyboardKey.end => _rowAt.length - 1,
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
    final c = t.colors;
    return ListenableBuilder(
      listenable: Listenable.merge([app.git, app.workspace]),
      builder: (context, _) {
        final s = app.git.status;
        Widget note(String text) => Padding(
          padding: const EdgeInsets.all(Metrics.sideGutter),
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
        final items = <_Item>[(kind: 'branch', group: '', file: null, k: -1)];
        final rowAt = <int>[];
        for (final (k, files) in sections) {
          items.add((kind: 'heading', group: k, file: null, k: files.length));
          for (final f in files) {
            items.add((kind: 'row', group: k, file: f, k: rowAt.length));
            rowAt.add(items.length - 1);
          }
        }
        _items = items;
        _rowAt = rowAt;
        if (_current >= rowAt.length) _current = rowAt.isEmpty ? 0 : rowAt.length - 1;
        _head = context.scaled(26, fontSize: 12, lineHeight: 16);
        _row = context.scaled(26);
        _branch = context.scaled(28, fontSize: 12, lineHeight: 15.6);
        final branch = Padding(
          padding: const EdgeInsets.symmetric(horizontal: Metrics.rowInset),
          child: Row(
            children: [
              AxiomIcon(AxiomIcons.branch, size: 14, color: c.ink2),
              const SizedBox(width: Metrics.s3),
              Expanded(
                child: Text(
                  s.branch ?? 'Detached at ${s.head ?? '?'}',
                  style: t.type.label,
                  overflow: TextOverflow.ellipsis,
                  maxLines: 1,
                ),
              ),
              if ((s.ahead ?? 0) > 0) Text(' ↑${s.ahead}', style: t.type.numeric),
              if ((s.behind ?? 0) > 0) Text(' ↓${s.behind}', style: t.type.numeric),
            ],
          ),
        );
        if (rowAt.isEmpty) {
          return Padding(
            padding: const EdgeInsets.symmetric(horizontal: Metrics.sideListInset),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                SizedBox(height: _branch, child: branch),
                note('No changes'),
              ],
            ),
          );
        }
        // One lazily built list (AUD-050), one Tab stop (AUD-021).
        return RovingFocus(
          onEnter: () => _focus(_current),
          onFocusChange: (v) => setState(() => _focused = v),
          child: Focus(
            canRequestFocus: false,
            skipTraversal: true,
            onKeyEvent: (_, e) => _key(app, e),
            child: ListView.builder(
              controller: _scroll,
              padding: const EdgeInsets.symmetric(horizontal: Metrics.sideListInset),
              itemCount: items.length,
              itemExtentBuilder: (i, _) => i < _items.length ? _extent(i) : null,
              itemBuilder: (context, i) {
                final it = _items[i];
                switch (it.kind) {
                  case 'branch':
                    return branch;
                  case 'heading':
                    return Semantics(
                      container: true,
                      header: true,
                      label: '${_labels[it.group]}, ${it.k}',
                      excludeSemantics: true,
                      child: Padding(
                        padding: const EdgeInsets.fromLTRB(
                          Metrics.rowInset,
                          Metrics.s3,
                          Metrics.rowInset,
                          Metrics.s1,
                        ),
                        child: Row(
                          children: [
                            Text(_labels[it.group]!, style: t.type.sectionHeading),
                            const SizedBox(width: Metrics.s3),
                            AxBadge(it.k, attention: false),
                          ],
                        ),
                      ),
                    );
                  default:
                    return _changeRow(context, it.group, it.file!, it.k);
                }
              },
            ),
          ),
        );
      },
    );
  }

  Widget _changeRow(BuildContext context, String group, GitFile f, int k) {
    final app = AppScope.of(context);
    final t = context.axiom;
    final c = t.colors;
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
    return AxListRow(
      label: '${f.path}, ${m?.word ?? 'changed'}${group == 'staged' ? ', staged' : ''}',
      hint: 'Show the changes — Enter; open the file — Shift+Enter',
      focusNode: _nodes[_id(group, f)],
      skipTraversal: true,
      selected: app.git.diff == f.path,
      listFocused: _focused,
      height: 26,
      onPressed: () {
        setState(() => _current = k);
        if (HardwareKeyboard.instance.isShiftPressed) {
          unawaited(app.documents.open(f.path));
        } else {
          app.git.openDiff(f.path);
        }
      },
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: Metrics.rowInset),
        child: Row(
          children: [
            Flexible(
              flex: 0,
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 200),
                child: Text(
                  name,
                  style: t.type.body.copyWith(color: gitInk(c, m?.kind)),
                  overflow: TextOverflow.ellipsis,
                  maxLines: 1,
                ),
              ),
            ),
            const SizedBox(width: Metrics.s3),
            Expanded(
              child: Text(dir, style: t.type.tiny, overflow: TextOverflow.ellipsis, maxLines: 1),
            ),
            Text(m?.letter ?? '', style: t.type.codeSmall.copyWith(color: gitInk(c, m?.kind))),
          ],
        ),
      ),
    );
  }
}

/// A diff's rows side by side: a removed line beside the added line that replaced it.
typedef DiffPair = (DiffLine?, DiffLine?);

/// Aligns HEAD's lines [a] with the working tree's [b] (the LCS of [diffLines], then pairs).
List<DiffPair> sideBySide(List<String> a, List<String> b) {
  final lines = diffLines(a, b);
  final rows = <DiffPair>[];
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
  return rows;
}

/// Above this many lines in all the alignment runs on another isolate (it is quadratic: up to
/// 4 M cells; AUD-079), as the gutter's diff does.
const diffIsolateLines = 2000;

/// [sideBySide], off the UI isolate for large inputs.
Future<List<DiffPair>> alignDiff(List<String> a, List<String> b) =>
    a.length + b.length > diffIsolateLines
    ? Isolate.run(() => sideBySide(a, b))
    : Future.value(sideBySide(a, b));

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
  List<DiffPair>? rows;
  int added = 0, removed = 0;
  String error = '';
  int _loads = 0;

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
    final load = ++_loads;
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
    final aligned = await alignDiff(
      head == null || head.isEmpty ? const [] : head.split('\n'),
      now.isEmpty ? const [] : now.split('\n'),
    );
    // A newer load (the file changed again, or another file) wins.
    if (!mounted || path != widget.path || load != _loads) return;
    var plus = 0, minus = 0;
    for (final (l, r) in aligned) {
      if (l != null && l.op == DiffOp.removed) minus++;
      if (r != null && r.op == DiffOp.added) plus++;
    }
    setState(() {
      rows = aligned;
      added = plus;
      removed = minus;
    });
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final l = rows;
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
                height: context.scaled(Metrics.tabHeight, fontSize: 12, lineHeight: 15.6),
                child: ColoredBox(
                  color: c.bar,
                  child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: Metrics.s4),
                    child: Row(
                      children: [
                        // A long path is ellipsised; the buttons keep their room (AUD-079).
                        Flexible(
                          child: Text(
                            widget.path,
                            style: t.type.label,
                            overflow: TextOverflow.ellipsis,
                            maxLines: 1,
                          ),
                        ),
                        const SizedBox(width: Metrics.s3),
                        Text('HEAD ↔ working tree', style: t.type.tiny),
                        const SizedBox(width: Metrics.s3),
                        Text('+$added', style: t.type.numeric.copyWith(color: c.okText)),
                        const SizedBox(width: Metrics.s3),
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
                    : SideBySide(rows: l),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// The aligned rows, HEAD on the left. Long lines wrap, so all of each line shows (AUD-058)
/// and a row grows with the system's text scale (AUD-020); rows are built lazily.
class SideBySide extends StatelessWidget {
  const SideBySide({super.key, required this.rows});
  final List<DiffPair> rows;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final code = t.type.code.copyWith(fontSize: 12.5, color: c.syntax.text, height: 1.5);
    final num = t.type.numeric.copyWith(fontSize: 11, color: c.ink3, height: 12.5 * 1.5 / 11);
    Widget side(DiffLine? d, bool left) {
      final n = d == null ? null : (left ? d.left : d.right);
      final changed = d != null && d.op != DiffOp.same;
      return Expanded(
        child: ColoredBox(
          color: !changed ? (d == null ? c.recessed : c.ground) : (left ? c.diffDel : c.diffAdd),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              SizedBox(
                width: 44,
                child: Text('${n ?? ''}', style: num, textAlign: TextAlign.right),
              ),
              const SizedBox(width: Metrics.s3),
              SizedBox(
                width: Metrics.s5,
                child: Text(
                  changed ? (left ? '−' : '+') : '',
                  style: code.copyWith(color: left ? c.dangerText : c.okText),
                ),
              ),
              Expanded(child: Text(d?.text ?? '', style: code)),
            ],
          ),
        ),
      );
    }

    return ListView.builder(
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
          child: IntrinsicHeight(
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                side(l, true),
                SizedBox(width: 1, child: ColoredBox(color: c.hairline)),
                side(r, false),
              ],
            ),
          ),
        );
      },
    );
  }
}
