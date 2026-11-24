// The folder's Git state as the UI shows it (FL-GIT-001; a port of the Tauri shell's GitStore,
// ADR-0018 §5): status from the daemon (`git/status`, refreshed on `git/didChange`), a mark per
// changed file and per folder that holds changes (explorer, tabs, Source Control), the branch
// (status bar), and the HEAD text for the gutter and the diff view (cached until the repository
// changes). Also the line diff both of those use.

import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/widgets.dart';

import '../daemon/client.dart';
import '../daemon/protocol.g.dart';
import '../ui/tokens.dart';
import 'workspace.dart';

enum GitKind { modified, added, deleted, renamed, untracked, conflicted }

final class GitMark {
  const GitMark(this.kind, {required this.staged});
  final GitKind kind;

  /// Staged (the index side) rather than only in the working tree.
  final bool staged;

  /// One letter, shown as text (never colour alone).
  String get letter => switch (kind) {
    GitKind.modified => 'M',
    GitKind.added => 'A',
    GitKind.deleted => 'D',
    GitKind.renamed => 'R',
    GitKind.untracked => 'U',
    GitKind.conflicted => 'C',
  };

  /// In words, for names.
  String get word => kind.name;
}

/// The ink of a changed file's name (with its letter, never colour alone).
Color? gitInk(AxiomColors c, GitKind? k) => switch (k) {
  null => null,
  GitKind.untracked || GitKind.added => c.okText,
  GitKind.modified || GitKind.renamed => c.infoText,
  GitKind.deleted || GitKind.conflicted => c.dangerText,
};

int _severity(GitKind k) => switch (k) {
  GitKind.conflicted => 5,
  GitKind.deleted => 4,
  GitKind.modified => 3,
  GitKind.renamed => 2,
  GitKind.added => 1,
  GitKind.untracked => 0,
};

GitKind? _kindOf(GitFileState s) => switch (s) {
  GitFileState.modified || GitFileState.typeChanged => GitKind.modified,
  GitFileState.added || GitFileState.copied => GitKind.added,
  GitFileState.deleted => GitKind.deleted,
  GitFileState.renamed => GitKind.renamed,
  GitFileState.untracked => GitKind.untracked,
  GitFileState.conflicted => GitKind.conflicted,
  _ => null,
};

/// One file's mark: conflicts first, then the working-tree change, then the staged one.
GitMark? markOf(GitFile f) {
  if (f.index_ == GitFileState.conflicted || f.worktree == GitFileState.conflicted) {
    return const GitMark(GitKind.conflicted, staged: false);
  }
  final w = _kindOf(f.worktree);
  final i = _kindOf(f.index_);
  if (w != null && w != GitKind.untracked) return GitMark(w, staged: false);
  if (i != null) return GitMark(i, staged: true);
  if (w != null) return GitMark(w, staged: false);
  return null;
}

/// Changes grouped as Source Control lists them.
({List<GitFile> conflicts, List<GitFile> staged, List<GitFile> changes, List<GitFile> untracked})
gitGroups(GitStatus? status) {
  final conflicts = <GitFile>[], staged = <GitFile>[], changes = <GitFile>[];
  final untracked = <GitFile>[];
  for (final f in status?.files ?? const <GitFile>[]) {
    if (f.index_ == GitFileState.conflicted || f.worktree == GitFileState.conflicted) {
      conflicts.add(f);
    } else if (f.index_ == GitFileState.untracked || f.worktree == GitFileState.untracked) {
      untracked.add(f);
    } else {
      if (_kindOf(f.index_) != null) staged.add(f);
      if (_kindOf(f.worktree) != null) changes.add(f);
    }
  }
  for (final l in [conflicts, staged, changes, untracked]) {
    l.sort((a, b) => a.path.compareTo(b.path));
  }
  return (conflicts: conflicts, staged: staged, changes: changes, untracked: untracked);
}

/// A line of a two-sided diff: in both (same), only on the left (removed), only on the right
/// (added). Line numbers are 1-based; null on the side a line is not on.
enum DiffOp { same, removed, added }

typedef DiffLine = ({DiffOp op, int? left, int? right, String text});

/// The line diff of [a] → [b]: a longest-common-subsequence over lines after trimming the
/// common head and tail (so a small edit in a large file costs little).
List<DiffLine> diffLines(List<String> a, List<String> b) {
  var head = 0;
  while (head < a.length && head < b.length && a[head] == b[head]) {
    head++;
  }
  var tail = 0;
  while (tail < a.length - head &&
      tail < b.length - head &&
      a[a.length - 1 - tail] == b[b.length - 1 - tail]) {
    tail++;
  }
  final out = <DiffLine>[
    for (var i = 0; i < head; i++) (op: DiffOp.same, left: i + 1, right: i + 1, text: a[i]),
  ];
  final x = a.sublist(head, a.length - tail);
  final y = b.sublist(head, b.length - tail);
  if (x.length * y.length > 4000000) {
    // Too large to align: everything in the middle changed.
    for (var i = 0; i < x.length; i++) {
      out.add((op: DiffOp.removed, left: head + i + 1, right: null, text: x[i]));
    }
    for (var j = 0; j < y.length; j++) {
      out.add((op: DiffOp.added, left: null, right: head + j + 1, text: y[j]));
    }
  } else {
    // lcs[i][j]: the common length of x[i..] and y[j..].
    final w = y.length + 1;
    final lcs = Uint32List((x.length + 1) * w);
    for (var i = x.length - 1; i >= 0; i--) {
      for (var j = y.length - 1; j >= 0; j--) {
        lcs[i * w + j] = x[i] == y[j]
            ? lcs[(i + 1) * w + j + 1] + 1
            : (lcs[(i + 1) * w + j] >= lcs[i * w + j + 1]
                  ? lcs[(i + 1) * w + j]
                  : lcs[i * w + j + 1]);
      }
    }
    var i = 0, j = 0;
    while (i < x.length || j < y.length) {
      if (i < x.length && j < y.length && x[i] == y[j]) {
        out.add((op: DiffOp.same, left: head + i + 1, right: head + j + 1, text: x[i]));
        i++;
        j++;
      } else if (i < x.length && (j == y.length || lcs[(i + 1) * w + j] >= lcs[i * w + j + 1])) {
        // Removed before added, as diffs read.
        out.add((op: DiffOp.removed, left: head + i + 1, right: null, text: x[i]));
        i++;
      } else {
        out.add((op: DiffOp.added, left: null, right: head + j + 1, text: y[j]));
        j++;
      }
    }
  }
  for (var k = 0; k < tail; k++) {
    final li = a.length - tail + k, ri = b.length - tail + k;
    out.add((op: DiffOp.same, left: li + 1, right: ri + 1, text: a[li]));
  }
  return out;
}

enum GutterMark { added, modified, deleted }

/// The working copy's gutter marks against HEAD, per 0-based line of [now]: added lines,
/// modified lines (removed and added together), and a deletion shown on the line after it.
Map<int, GutterMark> gutterMarks(String head, String now) {
  final d = diffLines(head.split('\n'), now.split('\n'));
  final out = <int, GutterMark>{};
  var i = 0;
  while (i < d.length) {
    if (d[i].op == DiffOp.same) {
      i++;
      continue;
    }
    var removed = 0;
    final added = <int>[];
    while (i < d.length && d[i].op != DiffOp.same) {
      if (d[i].op == DiffOp.removed) {
        removed++;
      } else {
        added.add(d[i].right! - 1);
      }
      i++;
    }
    if (added.isEmpty) {
      final at = i < d.length ? d[i].right! - 1 : (d.lastOrNull?.right ?? 1) - 1;
      out[at < 0 ? 0 : at] = GutterMark.deleted;
    } else {
      for (final (k, line) in added.indexed) {
        out[line] = k < removed ? GutterMark.modified : GutterMark.added;
      }
    }
  }
  return out;
}

class GitStore extends ChangeNotifier {
  GitStore(this.client, this.workspace) {
    _off = client.on(Notifications.gitDidChange, (p) {
      if (!workspace.isOpen || p.workspaceId != workspace.id) return;
      _heads.clear();
      _refreshTimer?.cancel();
      _refreshTimer = Timer(const Duration(milliseconds: 150), () => unawaited(refresh()));
    });
    workspace.addListener(_onWorkspace);
  }

  final DaemonClient client;
  final WorkspaceStore workspace;
  late final void Function() _off;
  Timer? _refreshTimer;
  final _heads = <String, Future<String?>>{};
  int _seq = 0;
  String? _lastWs;

  GitStatus? status;
  Map<String, GitMark> marks = const {};

  /// Folders that hold changes → the most severe kind inside.
  Map<String, GitKind> folders = const {};

  /// The file whose diff (HEAD ↔ working tree) the editor area shows.
  String? diff;

  /// Bumped when the repository changed (HEAD texts are stale).
  int version = 0;

  void _onWorkspace() {
    final id = workspace.isOpen ? workspace.id : null;
    if (id == _lastWs) return;
    _lastWs = id;
    _heads.clear();
    _seq++;
    status = null;
    marks = const {};
    folders = const {};
    diff = null;
    notifyListeners();
    if (id != null) unawaited(refresh());
  }

  Future<void> refresh() async {
    if (!workspace.isOpen) return;
    final seq = ++_seq;
    try {
      final s = await client.call(Methods.gitStatus, WorkspaceRef(workspaceId: workspace.id));
      if (seq != _seq) return;
      status = s;
      final m = <String, GitMark>{};
      final f = <String, GitKind>{};
      for (final file in s.files ?? const <GitFile>[]) {
        final mark = markOf(file);
        if (mark == null) continue;
        m[file.path] = mark;
        final parts = file.path.split('/');
        for (var i = 1; i < parts.length; i++) {
          final dir = parts.sublist(0, i).join('/');
          final had = f[dir];
          if (had == null || _severity(mark.kind) > _severity(had)) f[dir] = mark.kind;
        }
      }
      marks = m;
      folders = f;
      version++;
      notifyListeners();
    } on DaemonException {
      // Git failed, or no repository: nothing to show.
    }
  }

  /// The file's text at HEAD (null: not in HEAD, binary, or no repository). Before the first
  /// status has arrived nothing is known, and nothing is kept: the editor asks again when it
  /// arrives (audit finding 14).
  Future<String?> headText(String path) => status == null
      ? Future<String?>.value()
      : _heads.putIfAbsent(path, () async {
          if (status?.repo != true) return null;
          try {
            final r = await client.call(
              Methods.gitShow,
              ShowParams(workspaceId: workspace.id, path: path, rev: GitRev.head),
            );
            return r.binary ? null : r.text;
          } on DaemonException {
            return null;
          }
        });

  void openDiff(String path) {
    diff = path;
    notifyListeners();
  }

  void closeDiff() {
    if (diff == null) return;
    diff = null;
    notifyListeners();
  }

  int count() => status?.files?.length ?? 0;

  @override
  void dispose() {
    _off();
    _refreshTimer?.cancel();
    workspace.removeListener(_onWorkspace);
    super.dispose();
  }
}
