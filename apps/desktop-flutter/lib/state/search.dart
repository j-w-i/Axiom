// Search in the folder (FL-GIT-001; a port of the Tauri shell's SearchStore, M4-UX-001): the
// query and its options, results grouped by file, through `workspace/search`. A new query
// cancels the previous request; typing searches 200 ms after the last change.

import 'dart:async';

import 'package:flutter/foundation.dart';

import '../daemon/client.dart';
import '../daemon/protocol.g.dart';
import 'workspace.dart';

typedef MatchGroup = ({String path, List<SearchMatch> matches});

/// Matches grouped by file, in the order the daemon found them.
List<MatchGroup> groupMatches(List<SearchMatch> matches) {
  final out = <MatchGroup>[];
  for (final m in matches) {
    if (out.isNotEmpty && out.last.path == m.path) {
      out.last.matches.add(m);
    } else {
      out.add((path: m.path, matches: [m]));
    }
  }
  return out;
}

class SearchStore extends ChangeNotifier {
  SearchStore(this.client, this.workspace) {
    workspace.addListener(_onWorkspace);
  }

  final DaemonClient client;
  final WorkspaceStore workspace;

  String query = '';
  bool regex = false;
  bool caseSensitive = false;
  bool wholeWord = false;
  String include = '';
  String exclude = '';
  bool running = false;
  List<MatchGroup> groups = const [];
  int total = 0;
  bool truncated = false;
  String error = '';

  CancelToken? _cancel;
  Timer? _debounce;
  int _seq = 0;
  String? _lastWs;

  void _onWorkspace() {
    final id = workspace.isOpen ? workspace.id : null;
    if (id == _lastWs) return;
    _lastWs = id;
    _cancel?.cancel();
    query = '';
    groups = const [];
    total = 0;
    truncated = false;
    error = '';
    running = false;
    notifyListeners();
  }

  /// Changes the query or options; the search runs 200 ms after the last change.
  void update({
    String? query,
    bool? regex,
    bool? caseSensitive,
    bool? wholeWord,
    String? include,
    String? exclude,
  }) {
    this.query = query ?? this.query;
    this.regex = regex ?? this.regex;
    this.caseSensitive = caseSensitive ?? this.caseSensitive;
    this.wholeWord = wholeWord ?? this.wholeWord;
    this.include = include ?? this.include;
    this.exclude = exclude ?? this.exclude;
    notifyListeners();
    _debounce?.cancel();
    _debounce = Timer(const Duration(milliseconds: 200), () => unawaited(_run()));
  }

  /// Runs now (Enter in the query field).
  void now() {
    _debounce?.cancel();
    unawaited(_run());
  }

  Future<void> _run() async {
    _cancel?.cancel();
    _cancel = null;
    if (!workspace.isOpen || query.isEmpty) {
      running = false;
      groups = const [];
      total = 0;
      truncated = false;
      error = '';
      notifyListeners();
      return;
    }
    final seq = ++_seq;
    final ws = workspace.id;
    final cancel = _cancel = CancelToken();
    running = true;
    error = '';
    notifyListeners();
    try {
      final r = await client.call(
        Methods.workspaceSearch,
        SearchParams(
          workspaceId: workspace.id,
          query: query,
          regex: regex,
          caseSensitive: caseSensitive,
          wholeWord: wholeWord,
          include: include.trim().isEmpty ? null : include.trim(),
          exclude: exclude.trim().isEmpty ? null : exclude.trim(),
        ),
        cancel: cancel,
      );
      // A result for a folder that is no longer open is dropped (AUD-093).
      if (seq != _seq || workspace.id != ws) return;
      final m = r.matches ?? const <SearchMatch>[];
      running = false;
      groups = groupMatches(m);
      total = m.length;
      truncated = r.truncated;
      notifyListeners();
    } on DaemonException catch (e) {
      if (seq != _seq) return;
      running = false;
      groups = const [];
      total = 0;
      truncated = false;
      error = cancel.isCancelled ? '' : e.message;
      notifyListeners();
    } finally {
      if (seq == _seq) _cancel = null;
    }
  }

  @override
  void dispose() {
    _cancel?.cancel();
    _debounce?.cancel();
    workspace.removeListener(_onWorkspace);
    super.dispose();
  }
}
