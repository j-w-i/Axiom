// The explorer (FL-FILES-001): the workspace's tree, listed lazily by the daemon. Keyboard as in
// the Tauri shell: ↑/↓ move, → expands (or goes to the first child), ← collapses (or goes to the
// parent), Enter opens a file or toggles a folder, Home/End go to the first/last row. The tree is
// one Tab stop (AUD-021). Rows have a fixed height (grown with the system's text scale, AUD-020)
// so long folders scroll without layout work; deep nesting stops indenting before the name runs
// out of room (AUD-016).

import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../app/app_state.dart';
import '../state/git.dart';
import '../state/workspace.dart';
import '../ui/controls.dart';
import '../ui/icon_data.dart';
import '../ui/glyph.dart';
import '../ui/icons.dart';
import '../ui/pressable.dart';
import '../ui/roving.dart';
import '../ui/theme.dart';
import '../ui/tokens.dart';
import 'file_actions.dart';

/// A row's height at text scale 1.
const rowExtent = 22.0;
const _indent = 14.0;

/// What a row needs besides its indent: the left pad, the chevron, the icon and their gaps, and
/// room for the trailing Git letter or spinner.
const _rowFixed = Metrics.s4 + 16 + 4 + 16 + 4 + 26;

/// The smallest width a row keeps for the name, however deep it is.
const minNameWidth = 64.0;

/// The indent of a row at [depth] in a tree [width] px wide: 14 px a level, but never more than
/// 40 % of the width and never so much that the name gets less than [minNameWidth] (AUD-016).
/// Deeper rows than that share the deepest indent; the hint line gives their full path.
double explorerIndent(int depth, double width) {
  final cap = [width * 0.4, width - _rowFixed - minNameWidth].reduce((a, b) => a < b ? a : b);
  final want = depth * _indent;
  if (cap <= 0) return 0;
  return want < cap ? want : cap.floorToDouble();
}

/// What opening a file does (the editor, from FL-EDIT-001 on). [focus]: move keyboard focus to
/// the editor (opened by keyboard), or leave it in the explorer (a click).
typedef OpenFile = void Function(String path, {bool focus});

class Explorer extends StatefulWidget {
  const Explorer({super.key, required this.workspace, this.onOpenFile});
  final WorkspaceStore workspace;
  final OpenFile? onOpenFile;

  @override
  State<Explorer> createState() => _ExplorerState();
}

class _ExplorerState extends State<Explorer> {
  final _scroll = ScrollController();
  final _nodes = RowFocusNodes(debugPrefix: 'row');
  String? _current;
  List<VisibleRow> _rows = const [];
  double _extent = rowExtent;

  /// Focus is somewhere in the tree (the selected row's marker is brass).
  bool _treeFocused = false;

  @override
  void dispose() {
    _scroll.dispose();
    _nodes.dispose();
    super.dispose();
  }

  void _focusIndex(int i) {
    if (_rows.isEmpty) return;
    i = i.clamp(0, _rows.length - 1);
    final path = _rows[i].node.path;
    setState(() => _current = path);
    widget.workspace.selection = path;
    // Bring the row into view first (rows off screen are not built), then focus it.
    if (_scroll.hasClients) {
      final top = i * _extent + 2;
      final view = _scroll.position.viewportDimension;
      if (top < _scroll.offset) {
        _scroll.jumpTo(top);
      } else if (top + _extent > _scroll.offset + view) {
        _scroll.jumpTo(top + _extent - view);
      }
    }
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _nodes[path].requestFocus();
    });
  }

  /// Tab reached the tree: focus the selected row, or the first.
  void _enter() {
    final i = _index;
    _focusIndex(i < 0 ? 0 : i);
  }

  int get _index => _rows.indexWhere((r) => r.node.path == _current);

  void _activate(TreeNode n) {
    if (n.isDir) {
      widget.workspace.toggle(n.path);
    } else {
      widget.onOpenFile?.call(n.path, focus: FocusVisibility.instance.keyboard.value);
    }
  }

  void _key(_TreeKey k) {
    final i = _index;
    if (i < 0) {
      _focusIndex(0);
      return;
    }
    final n = _rows[i].node;
    final app = AppScope.of(context);
    switch (k) {
      // The file operations by keyboard (AUD-022).
      case _TreeKey.rename:
        unawaited(renamePath(context, app, n.path));
      case _TreeKey.trash:
        unawaited(trashPath(context, app, n.path));
      case _TreeKey.menu:
        app.openFileMenu?.call();
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
    final git = AppScope.of(context).git;
    return ListenableBuilder(
      listenable: Listenable.merge([widget.workspace, git]),
      builder: (context, _) {
        final ws = widget.workspace;
        _rows = ws.visibleRows();
        _extent = context.scaled(rowExtent);
        // One turning spinner in the tree (motion-system.md §5.3): the first loading folder's.
        final firstLoading = _rows.indexWhere((r) => r.node.loading && !r.node.loaded);
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
            SingleActivator(LogicalKeyboardKey.f2): _TreeIntent(_TreeKey.rename),
            SingleActivator(LogicalKeyboardKey.delete): _TreeIntent(_TreeKey.trash),
            SingleActivator(LogicalKeyboardKey.f10, shift: true): _TreeIntent(_TreeKey.menu),
            SingleActivator(LogicalKeyboardKey.contextMenu): _TreeIntent(_TreeKey.menu),
          },
          child: Actions(
            actions: {_TreeIntent: CallbackAction<_TreeIntent>(onInvoke: (i) => _key(i.key))},
            child: Semantics(
              label: 'Files in ${ws.name}',
              container: true,
              child: RovingFocus(
                onEnter: _enter,
                onFocusChange: (v) => setState(() => _treeFocused = v),
                child: LayoutBuilder(
                  builder: (context, box) => ListView.builder(
                    controller: _scroll,
                    itemExtent: _extent,
                    itemCount: _rows.length,
                    padding: const EdgeInsets.only(top: Metrics.s1, bottom: Metrics.s5),
                    itemBuilder: (context, i) {
                      final (:node, :depth) = _rows[i];
                      // A right-click selects the row and opens its menu (AUD-022).
                      return GestureDetector(
                        onSecondaryTapUp: (_) {
                          setState(() => _current = node.path);
                          widget.workspace.selection = node.path;
                          AppScope.of(context).openFileMenu?.call();
                        },
                        child: ExplorerRow(
                          key: ValueKey(node.path),
                          node: node,
                          indent: explorerIndent(depth, box.maxWidth),
                          focusNode: _nodes[node.path],
                          selected: node.path == _current,
                          treeFocused: _treeFocused,
                          spinnerStill: i != firstLoading,
                          onPressed: () {
                            setState(() => _current = node.path);
                            widget.workspace.selection = node.path;
                            _activate(node);
                          },
                          colors: c,
                          type: t.type,
                          mark: git.marks[node.path],
                          folderKind: node.isDir ? git.folders[node.path] : null,
                        ),
                      );
                    },
                  ),
                ),
              ),
            ),
          ),
        );
      },
    );
  }
}

enum _TreeKey { up, down, left, right, home, end, rename, trash, menu }

class _TreeIntent extends Intent {
  const _TreeIntent(this.key);
  final _TreeKey key;
}

/// One row of the tree: indent, chevron, icon or file chip, name (ellipsised, never squeezed out),
/// Git letter or folder dot, a spinner while it lists.
class ExplorerRow extends StatelessWidget {
  const ExplorerRow({
    super.key,
    required this.node,
    required this.indent,
    required this.selected,
    required this.onPressed,
    required this.colors,
    required this.type,
    this.focusNode,
    this.treeFocused = false,
    this.spinnerStill = false,
    this.mark,
    this.folderKind,
  });
  final TreeNode node;

  /// From [explorerIndent].
  final double indent;
  final FocusNode? focusNode;
  final bool selected;

  /// Focus is in the tree: the selected row's marker is brass.
  final bool treeFocused;

  /// Another row's spinner already turns.
  final bool spinnerStill;
  final VoidCallback onPressed;
  final AxiomColors colors;
  final AxiomType type;

  /// The file's Git mark, or the most severe change inside a folder.
  final GitMark? mark;
  final GitKind? folderKind;

  @override
  Widget build(BuildContext context) {
    final c = colors;
    final tint = gitInk(c, mark?.kind ?? folderKind);
    final ink = node.ignored ? c.ink3 : (tint ?? c.ink);
    final label = [
      node.name,
      if (node.isDir) 'folder' else 'file',
      if (node.ignored) 'ignored',
      if (mark != null) mark!.word,
      if (folderKind != null) 'holds changes',
    ].join(', ');
    return Pressable(
      label: label,
      // The full path: a deep row's indent no longer shows all of it.
      hint: node.isDir ? '${node.path} — Enter or → to expand' : '${node.path} — Enter to open',
      onPressed: onPressed,
      selected: selected,
      expanded: node.isDir ? node.expanded : null,
      focusNode: focusNode,
      skipTraversal: true,
      builder: (context, s) {
        Widget row = Row(
          children: [
            SizedBox(width: indent + Metrics.s4),
            SizedBox(
              width: 16,
              child: node.isDir
                  ? Transform.rotate(
                      angle: node.expanded ? 1.5708 : 0,
                      child: AxiomIcon(AxiomIcons.chevron, size: 13, color: c.ink3),
                    )
                  : null,
            ),
            const SizedBox(width: Metrics.s2),
            // Tauri's tree: folders in the folder tone, files as their type's chip.
            SizedBox(
              width: 16,
              child: Center(
                child: node.isDir
                    ? AxiomIcon(
                        node.expanded ? AxiomIcons.folderOpen : AxiomIcons.folder,
                        size: 15,
                        color: node.ignored ? c.ink3 : c.folder,
                      )
                    : FileGlyph(node.name),
              ),
            ),
            const SizedBox(width: Metrics.s2),
            Expanded(
              child: Text(
                node.name,
                style: type.body.copyWith(color: ink, fontSize: 13),
                overflow: TextOverflow.ellipsis,
                maxLines: 1,
              ),
            ),
            if (node.loading && !node.loaded)
              Padding(
                padding: const EdgeInsets.only(right: Metrics.s3),
                child: Spinner(size: 10, still: spinnerStill, label: 'Listing ${node.name}'),
              ),
            if (mark != null)
              Padding(
                padding: const EdgeInsets.only(right: Metrics.s4),
                child: Text(mark!.letter, style: type.codeSmall.copyWith(color: tint)),
              )
            else if (folderKind != null)
              Padding(
                padding: const EdgeInsets.only(right: Metrics.s4 + Metrics.s1),
                child: Container(
                  width: 5,
                  height: 5,
                  decoration: BoxDecoration(color: tint, shape: BoxShape.circle),
                ),
              ),
          ],
        );
        // Tauri's selected row: the selected tint and a 2 px marker, brass while the tree has
        // focus; full width, no rounding.
        if (selected) {
          row = DecoratedBox(
            decoration: BoxDecoration(color: c.selected),
            child: Stack(
              children: [
                row,
                Positioned(
                  left: 0,
                  top: 3,
                  bottom: 3,
                  width: 2,
                  child: ColoredBox(color: treeFocused || s.focused ? c.accent : c.ink3),
                ),
              ],
            ),
          );
        } else if (s.hovered) {
          row = ColoredBox(color: c.hover, child: row);
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
          right: Metrics.s3,
          top: Metrics.s2,
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
            Semantics(header: true, child: Text('Recent', style: t.type.sectionHeading)),
            const SizedBox(height: Metrics.s2),
            // Tauri's recent rows: the name, then the path in quiet mono.
            for (final r in recent.take(8))
              Transform.translate(
                offset: const Offset(-Metrics.s4, 0),
                child: AxListRow(
                  label: 'Open ${r.name}, ${r.root}',
                  hint: 'Open ${r.root}',
                  height: 28,
                  onPressed: () => unawaited(app.workspace.openFolder(r.root)),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: Metrics.s4),
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.center,
                      children: [
                        Text(r.name, style: t.type.body.copyWith(fontWeight: FontWeight.w500)),
                        const SizedBox(width: Metrics.s5),
                        Expanded(
                          child: Text(
                            r.root,
                            style: t.type.codeSmall.copyWith(fontSize: 11, color: t.colors.ink3),
                            overflow: TextOverflow.ellipsis,
                          ),
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
