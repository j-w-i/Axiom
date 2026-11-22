// The workspace and its explorer tree: a port of the Tauri shell's WorkspaceStore (itself the
// Qt WorkspaceController + FileTreeModel). Same protocol sequence (data-flow.md §Open a
// workspace), same persisted key (`explorer.expanded`), same lazy listing and change handling:
// only expanded, loaded directories that are parents of changed paths are re-listed.

import 'dart:async';

import 'package:flutter/foundation.dart';

import '../daemon/client.dart';
import '../daemon/protocol.g.dart';

const expandedKey = 'explorer.expanded';
const _saveDebounce = Duration(milliseconds: 500);

class TreeNode {
  TreeNode({
    required this.path,
    required this.name,
    required this.kind,
    this.ignored = false,
    this.expanded = false,
  });

  /// Workspace-relative, `/`-separated; `''` is the root.
  final String path;
  final String name;
  final EntryKind kind;
  bool ignored;
  bool expanded;
  bool loaded = false;
  bool loading = false;
  List<String> children = [];

  /// The listing failed: shown inline under the directory.
  String? error;

  bool get isDir => kind == EntryKind.directory;
}

typedef VisibleRow = ({TreeNode node, int depth});

String parentPath(String p) {
  final i = p.lastIndexOf('/');
  return i < 0 ? '' : p.substring(0, i);
}

class WorkspaceStore extends ChangeNotifier {
  WorkspaceStore(this.client) {
    _offChange = client.on(Notifications.fsDidChange, (p) {
      if (p.workspaceId != id) return;
      applyChanges([for (final c in p.changes) c.path]);
    });
    client.state.addListener(_onDaemonState);
  }

  final DaemonClient client;
  late final void Function() _offChange;

  String id = '';
  String root = '';
  String name = '';
  bool opening = false;
  String error = '';
  List<RecentWorkspace> recent = const [];
  bool rootLoaded = false;

  /// Bumped on every tree change (nodes are mutated in place).
  int treeVersion = 0;

  final _nodes = <String, TreeNode>{};
  var _restore = <String>{};
  final _refetch = <String>{};
  int _generation = 0;
  bool _restoring = false;
  int _pendingClose = 0;
  String? _deferredOpen;
  Timer? _saveTimer;
  final _opened = <VoidCallback>[];
  final _closed = <VoidCallback>[];

  bool get isOpen => id.isNotEmpty;

  void Function() onOpened(VoidCallback fn) {
    _opened.add(fn);
    return () => _opened.remove(fn);
  }

  void Function() onClosed(VoidCallback fn) {
    _closed.add(fn);
    return () => _closed.remove(fn);
  }

  TreeNode? node(String path) => _nodes[path];

  void _onDaemonState() {
    if (client.state.value is DaemonReady) {
      unawaited(refreshRecent());
    } else {
      // A new daemon process knows no workspaces.
      _pendingClose = 0;
      _deferredOpen = null;
      if (isOpen || opening) _clearOpenState();
    }
  }

  /// Depth-first rows to render (children of expanded directories).
  List<VisibleRow> visibleRows() {
    final out = <VisibleRow>[];
    void walk(TreeNode dir, int depth) {
      for (final p in dir.children) {
        final n = _nodes[p];
        if (n == null) continue;
        out.add((node: n, depth: depth));
        if (n.isDir && n.expanded) walk(n, depth + 1);
      }
    }

    final r = _nodes[''];
    if (r != null) walk(r, 0);
    return out;
  }

  List<String> expandedPaths() => [
    for (final n in _nodes.values)
      if (n.path.isNotEmpty && n.isDir && n.expanded) n.path,
  ]..sort();

  void dismissError() {
    error = '';
    notifyListeners();
  }

  // ---- open / close ---------------------------------------------------------------------

  Future<void> openFolder(String path) async {
    if (path.isEmpty) return;
    final gen = ++_generation;
    opening = true;
    error = '';
    notifyListeners();
    if (_pendingClose > 0) {
      _deferredOpen = path;
      return;
    }
    if (isOpen) {
      final oldId = id;
      await flushState();
      _resetTree('');
      id = '';
      try {
        await client.call(Methods.workspaceClose, WorkspaceRef(workspaceId: oldId));
      } on DaemonException {
        // The old one is gone either way.
      }
    }
    if (gen != _generation) return;
    final OpenResult r;
    try {
      r = await client.call(Methods.workspaceOpen, OpenParams(path: path));
    } on DaemonException catch (e) {
      if (gen != _generation) return;
      opening = false;
      error = e.kind == ErrorKind.notFound
          ? 'The folder does not exist.'
          : 'Could not open the folder: ${e.message}';
      notifyListeners();
      return;
    }
    if (gen != _generation) return;
    _resetTree(r.workspaceId);
    _restoring = true;
    id = r.workspaceId;
    root = r.root;
    name = r.name;
    notifyListeners();
    try {
      await client.call(Methods.fsWatch, WatchParams(workspaceId: r.workspaceId, enabled: true));
    } on DaemonException {
      // Watching is best effort; listing still works.
    }
    if (gen != _generation) return;
    var expanded = <String>[];
    try {
      final s = await client.call(
        Methods.workspaceGetState,
        GetStateParams(workspaceId: r.workspaceId, keys: const [expandedKey]),
      );
      final v = s.values_[expandedKey];
      if (v is List) expanded = v.whereType<String>().toList();
    } on DaemonException {
      // No saved state.
    }
    if (gen != _generation) return;
    _restore = expanded.toSet();
    await _fetch('');
    if (gen != _generation) return;
    _restoring = false;
    opening = false;
    notifyListeners();
    for (final fn in List.of(_opened)) {
      fn();
    }
    unawaited(refreshRecent());
  }

  Future<void> close() async {
    if (!isOpen) return;
    final old = id;
    _pendingClose++;
    final flushed = flushState();
    _clearOpenState();
    await flushed;
    try {
      await client.call(Methods.workspaceClose, WorkspaceRef(workspaceId: old));
    } on DaemonException {
      // Ignore.
    }
    if (_pendingClose > 0) _pendingClose--;
    if (_pendingClose == 0 && _deferredOpen != null) {
      final p = _deferredOpen!;
      _deferredOpen = null;
      await openFolder(p);
    }
  }

  void _clearOpenState() {
    _saveTimer?.cancel();
    ++_generation;
    _resetTree('');
    id = '';
    root = '';
    name = '';
    opening = false;
    notifyListeners();
    for (final fn in List.of(_closed)) {
      fn();
    }
  }

  void _resetTree(String workspaceId) {
    _nodes.clear();
    _refetch.clear();
    if (workspaceId.isNotEmpty) {
      _nodes[''] = TreeNode(path: '', name: '', kind: EntryKind.directory, expanded: true);
    }
    rootLoaded = false;
    _bump();
  }

  Future<void> refreshRecent() async {
    try {
      recent = (await client.call(Methods.workspaceRecent, const Empty())).workspaces;
      notifyListeners();
    } on DaemonException {
      // Keep the old list.
    }
  }

  /// Opens the last workspace when `workspace.reopenLast` is true.
  Future<void> reopenLast() async {
    try {
      final r = await client.call(
        Methods.settingsGet,
        const GetParams(keys: ['workspace.reopenLast', 'workspace.lastRoot']),
      );
      if (isOpen || opening) return;
      final last = r.values_['workspace.lastRoot'];
      final reopen = r.values_['workspace.reopenLast'] != false;
      if (reopen && last is String && last.isNotEmpty) await openFolder(last);
    } on DaemonException {
      // Nothing to reopen.
    }
  }

  // ---- explorer state persistence --------------------------------------------------------

  Future<void> flushState() async {
    _saveTimer?.cancel();
    if (!isOpen || !rootLoaded) return;
    try {
      await client.call(
        Methods.workspaceSetState,
        SetStateParams(workspaceId: id, values_: {expandedKey: expandedPaths()}),
      );
    } on DaemonException {
      // Best effort.
    }
  }

  void _scheduleSave() {
    _saveTimer?.cancel();
    _saveTimer = Timer(_saveDebounce, () => unawaited(flushState()));
  }

  // ---- tree -------------------------------------------------------------------------------

  void setExpanded(String path, bool expanded) {
    final n = _nodes[path];
    if (n == null || !n.isDir || n.expanded == expanded) return;
    n.expanded = expanded;
    _bump();
    // Re-list on expand even when loaded: it may have changed while collapsed.
    if (expanded) unawaited(_fetch(path));
    if (!_restoring && isOpen) _scheduleSave();
  }

  void toggle(String path) {
    final n = _nodes[path];
    if (n != null) setExpanded(path, !n.expanded);
  }

  void _bump() {
    treeVersion++;
    notifyListeners();
  }

  Future<void> _fetch(String path) async {
    final dir = _nodes[path];
    final workspaceId = id;
    if (dir == null || workspaceId.isEmpty) return;
    if (dir.loading) {
      _refetch.add(path); // a change arrived mid-listing: list again afterwards
      return;
    }
    dir.loading = true;
    if (!dir.loaded && path.isNotEmpty) _bump();
    final gen = _generation;
    List<DirEntry>? entries;
    String? err;
    try {
      entries = (await client.call(
        Methods.fsList,
        ListParams(workspaceId: workspaceId, path: path),
      )).entries;
    } on DaemonException catch (e) {
      err = e.message;
    }
    if (gen != _generation) return;
    final current = _nodes[path];
    if (current == null) return; // disappeared while listing
    current.loading = false;
    if (err != null) {
      if (path.isEmpty) {
        error = 'Could not list the folder: $err';
      } else {
        _removeSubtree(current, false);
        current.children = [];
        current.error = err;
      }
      _bump();
      return;
    }
    final toFetch = _applyListing(current, entries ?? const []);
    current.loaded = true;
    current.error = null;
    if (path.isEmpty) rootLoaded = true;
    _bump();
    final again = _refetch.remove(path);
    await Future.wait([for (final p in toFetch) _fetch(p), if (again) _fetch(path)]);
  }

  /// Replaces a directory's children, keeping nodes (and their expansion) that still exist.
  List<String> _applyListing(TreeNode dir, List<DirEntry> entries) {
    final keep = <String, TreeNode>{};
    for (final p in dir.children) {
      final n = _nodes[p];
      if (n != null) keep[n.name] = n;
    }
    final next = <String>[];
    final toFetch = <String>[];
    for (final e in entries) {
      final path = dir.path.isEmpty ? e.name : '${dir.path}/${e.name}';
      final old = keep[e.name];
      if (old != null && old.kind == e.kind) {
        old.ignored = e.ignored ?? false;
        keep.remove(e.name);
        next.add(path);
        continue;
      }
      final restore = e.kind == EntryKind.directory && _restore.remove(path);
      _nodes[path] = TreeNode(
        path: path,
        name: e.name,
        kind: e.kind,
        ignored: e.ignored ?? false,
        expanded: restore,
      );
      next.add(path);
      if (restore) toFetch.add(path);
    }
    for (final gone in keep.values) {
      _removeSubtree(gone, true);
    }
    dir.children = next;
    return toFetch;
  }

  void _removeSubtree(TreeNode n, bool self) {
    for (final c in n.children) {
      final child = _nodes[c];
      if (child != null) _removeSubtree(child, true);
    }
    if (self) _nodes.remove(n.path);
  }

  /// [paths] from `fs/didChange`; `''` means "re-list everything shown".
  void applyChanges(List<String> paths) {
    if (!isOpen || !rootLoaded) return;
    final all = paths.contains('');
    final parents = {
      for (final p in paths)
        if (p.isNotEmpty) parentPath(p),
    };
    void visit(TreeNode n) {
      final shown = n.path.isEmpty || (n.expanded && n.loaded);
      if (!shown) return;
      if (all || parents.contains(n.path)) unawaited(_fetch(n.path));
      for (final c in n.children) {
        final child = _nodes[c];
        if (child != null && child.isDir) visit(child);
      }
    }

    final r = _nodes[''];
    if (r != null) visit(r);
  }

  @override
  void dispose() {
    _offChange();
    _saveTimer?.cancel();
    client.state.removeListener(_onDaemonState);
    super.dispose();
  }
}
