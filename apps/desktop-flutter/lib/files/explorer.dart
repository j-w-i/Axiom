// The explorer (FL-FILES-001): the workspace's tree, listed lazily by the daemon. Keyboard as in
// the Tauri shell: ↑/↓ move, → expands (or goes to the first child), ← collapses (or goes to the
// parent), Enter opens a file or toggles a folder, Home/End go to the first/last row. Rows have
// a fixed height so long folders scroll without layout work.

import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../app/app_state.dart';
import '../state/workspace.dart';
import '../ui/controls.dart';
import '../ui/icon_data.dart';
import '../ui/icons.dart';
import '../ui/pressable.dart';
import '../ui/theme.dart';
import '../ui/tokens.dart';

const rowExtent = 22.0;
const _indent = 12.0;

/// What opening a file does (the editor, from FL-EDIT-001 on).
typedef OpenFile = void Function(String path);

class Explorer extends StatefulWidget {
  const Explorer({super.key, required this.workspace, this.onOpenFile});
  final WorkspaceStore workspace;
  final OpenFile? onOpenFile;

  @override
  State<Explorer> createState() => _ExplorerState();
}

class _ExplorerState extends State<Explorer> {
  final _scroll = ScrollController();
  final _nodes = <String, FocusNode>{};
  String? _current;
  List<VisibleRow> _rows = const [];

  FocusNode _node(String path) =>
      _nodes.putIfAbsent(path, () => FocusNode(debugLabel: 'row $path'));

  @override
  void dispose() {
    _scroll.dispose();
    for (final n in _nodes.values) {
      n.dispose();
    }
    super.dispose();
  }

  void _focusIndex(int i) {
    if (_rows.isEmpty) return;
    i = i.clamp(0, _rows.length - 1);
    final path = _rows[i].node.path;
    setState(() => _current = path);
    // Bring the row into view first (rows off screen are not built), then focus it.
    final top = i * rowExtent;
    final view = _scroll.position.viewportDimension;
    if (top < _scroll.offset) {
      _scroll.jumpTo(top);
    } else if (top + rowExtent > _scroll.offset + view) {
      _scroll.jumpTo(top + rowExtent - view);
    }
    WidgetsBinding.instance.addPostFrameCallback((_) => _node(path).requestFocus());
  }

  int get _index => _rows.indexWhere((r) => r.node.path == _current);

  void _activate(TreeNode n) {
    if (n.isDir) {
      widget.workspace.toggle(n.path);
    } else {
      widget.onOpenFile?.call(n.path);
    }
  }

  void _key(_TreeKey k) {
    final i = _index;
    if (i < 0) {
      _focusIndex(0);
      return;
    }
    final n = _rows[i].node;
    switch (k) {
      case _TreeKey.down:
        _focusIndex(i + 1);
      case _TreeKey.up:
        _focusIndex(i - 1);
      case _TreeKey.home:
        _focusIndex(0);
      case _TreeKey.end:
        _focusIndex(_rows.length - 1);
      case _TreeKey.right:
        if (n.isDir && !n.expanded) {
          widget.workspace.setExpanded(n.path, true);
        } else if (n.isDir && n.children.isNotEmpty) {
          _focusIndex(i + 1);
        }
      case _TreeKey.left:
        if (n.isDir && n.expanded) {
          widget.workspace.setExpanded(n.path, false);
        } else {
          final parent = parentPath(n.path);
          final pi = _rows.indexWhere((r) => r.node.path == parent);
          if (pi >= 0) _focusIndex(pi);
        }
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    return ListenableBuilder(
      listenable: widget.workspace,
      builder: (context, _) {
        final ws = widget.workspace;
        _rows = ws.visibleRows();
        if (!ws.rootLoaded) {
          return Center(child: Spinner(label: 'Listing ${ws.name}'));
        }
        if (_rows.isEmpty) {
          return Padding(
            padding: const EdgeInsets.all(Metrics.s5),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('This folder is empty', style: t.type.small),
                const SizedBox(height: Metrics.s4),
                AxButton(
                  label: 'Refresh',
                  kind: ButtonKind.quiet,
                  onPressed: () => ws.applyChanges(const ['']),
                ),
              ],
            ),
          );
        }
        return Shortcuts(
          shortcuts: const {
            SingleActivator(LogicalKeyboardKey.arrowDown): _TreeIntent(_TreeKey.down),
            SingleActivator(LogicalKeyboardKey.arrowUp): _TreeIntent(_TreeKey.up),
            SingleActivator(LogicalKeyboardKey.arrowRight): _TreeIntent(_TreeKey.right),
            SingleActivator(LogicalKeyboardKey.arrowLeft): _TreeIntent(_TreeKey.left),
            SingleActivator(LogicalKeyboardKey.home): _TreeIntent(_TreeKey.home),
            SingleActivator(LogicalKeyboardKey.end): _TreeIntent(_TreeKey.end),
          },
          child: Actions(
            actions: {_TreeIntent: CallbackAction<_TreeIntent>(onInvoke: (i) => _key(i.key))},
            child: Semantics(
              label: 'Files in ${ws.name}',
              container: true,
              child: ListView.builder(
                controller: _scroll,
                itemExtent: rowExtent,
                itemCount: _rows.length,
                padding: const EdgeInsets.symmetric(horizontal: Metrics.s2),
                itemBuilder: (context, i) {
                  final (:node, :depth) = _rows[i];
                  return _Row(
                    key: ValueKey(node.path),
                    node: node,
                    depth: depth,
                    focusNode: _node(node.path),
                    selected: node.path == _current,
                    onPressed: () {
                      setState(() => _current = node.path);
                      _activate(node);
                    },
                    colors: c,
                    type: t.type,
                  );
                },
              ),
            ),
          ),
        );
      },
    );
  }
}

enum _TreeKey { up, down, left, right, home, end }

class _TreeIntent extends Intent {
  const _TreeIntent(this.key);
  final _TreeKey key;
}

class _Row extends StatelessWidget {
  const _Row({
    super.key,
    required this.node,
    required this.depth,
    required this.focusNode,
    required this.selected,
    required this.onPressed,
    required this.colors,
    required this.type,
  });
  final TreeNode node;
  final int depth;
  final FocusNode focusNode;
  final bool selected;
  final VoidCallback onPressed;
  final AxiomColors colors;
  final AxiomType type;

  @override
  Widget build(BuildContext context) {
    final c = colors;
    final ink = node.ignored ? c.ink3 : c.ink;
    final label = [
      node.name,
      if (node.isDir) 'folder' else 'file',
      if (node.ignored) 'ignored',
    ].join(', ');
    return Pressable(
      label: label,
      hint: node.isDir ? '${node.name} — Enter or → to expand' : '${node.name} — Enter to open',
      onPressed: onPressed,
      selected: selected,
      expanded: node.isDir ? node.expanded : null,
      focusNode: focusNode,
      builder: (context, s) {
        Widget row = Row(
          children: [
            SizedBox(width: depth * _indent + 4),
            SizedBox(
              width: 16,
              child: node.isDir
                  ? Transform.rotate(
                      angle: node.expanded ? 1.5708 : 0,
                      child: AxiomIcon(AxiomIcons.chevron, size: 12, color: c.ink3),
                    )
                  : null,
            ),
            AxiomIcon(
              node.isDir
                  ? (node.expanded ? AxiomIcons.folderOpen : AxiomIcons.folder)
                  : AxiomIcons.file,
              size: 14,
              color: node.ignored ? c.ink3 : c.ink2,
            ),
            const SizedBox(width: 6),
            Expanded(
              child: Text(
                node.name,
                style: type.body.copyWith(color: ink, fontSize: 13),
                overflow: TextOverflow.ellipsis,
                maxLines: 1,
              ),
            ),
            if (node.loading && !node.loaded)
              const Padding(padding: EdgeInsets.only(right: 6), child: Spinner(size: 10)),
          ],
        );
        if (selected) {
          row = DecoratedBox(
            decoration: BoxDecoration(
              color: s.focused ? c.accentSoft : c.raised,
              borderRadius: BorderRadius.circular(Metrics.radiusControl),
            ),
            child: row,
          );
        } else if (s.hovered) {
          row = DecoratedBox(
            decoration: BoxDecoration(
              color: c.hover,
              borderRadius: BorderRadius.circular(Metrics.radiusControl),
            ),
            child: row,
          );
        }
        if (node.error != null) {
          return _ErrorMark(error: node.error!, child: row);
        }
        return FocusRing(visible: s.focused && !selected, child: row);
      },
    );
  }
}

/// A row whose listing failed shows the reason in the hint line and in red.
class _ErrorMark extends StatelessWidget {
  const _ErrorMark({required this.error, required this.child});
  final String error;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Stack(
      children: [
        child,
        Positioned(
          right: 6,
          top: 4,
          child: Semantics(
            label: 'Could not list: $error',
            child: AxiomIcon(AxiomIcons.warning, size: 14, color: context.colors.danger),
          ),
        ),
      ],
    );
  }
}

/// The Files destination: the explorer when a folder is open, otherwise the way to open one.
class FilesPage extends StatelessWidget {
  const FilesPage({super.key, this.onOpenFile});
  final OpenFile? onOpenFile;

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final t = context.axiom;
    return ListenableBuilder(
      listenable: app.workspace,
      builder: (context, _) {
        final ws = app.workspace;
        if (ws.isOpen) return Explorer(workspace: ws, onOpenFile: onOpenFile);
        return Padding(
          padding: const EdgeInsets.all(Metrics.s5),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text('No folder open', style: t.type.label),
              const SizedBox(height: Metrics.s4),
              AxButton(
                label: 'Open Folder…',
                icon: AxiomIcons.folderOpen,
                hint: 'Open Folder… — Mod+O',
                onPressed: ws.opening ? null : () => unawaited(app.pickFolder?.call()),
              ),
            ],
          ),
        );
      },
    );
  }
}

/// The recent folders, for the welcome page.
class RecentList extends StatelessWidget {
  const RecentList({super.key});

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final t = context.axiom;
    return ListenableBuilder(
      listenable: app.workspace,
      builder: (context, _) {
        final recent = app.workspace.recent;
        if (recent.isEmpty) return const SizedBox.shrink();
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Semantics(header: true, child: Text('Recent', style: t.type.tiny)),
            const SizedBox(height: Metrics.s3),
            for (final r in recent.take(8))
              SizedBox(
                width: 520,
                child: AxListRow(
                  label: 'Open ${r.name}, ${r.root}',
                  hint: 'Open ${r.root}',
                  height: 28,
                  onPressed: () => unawaited(app.workspace.openFolder(r.root)),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 8),
                    child: Row(
                      children: [
                        AxiomIcon(AxiomIcons.folder, size: 14, color: t.colors.ink2),
                        const SizedBox(width: 8),
                        Text(r.name, style: t.type.body),
                        const SizedBox(width: 10),
                        Expanded(
                          child: Text(r.root, style: t.type.small, overflow: TextOverflow.ellipsis),
                        ),
                      ],
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
