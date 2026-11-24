// The Search page (FL-GIT-001; the Tauri shell's Search.tsx, M4-UX-001): a query with Match case,
// Whole word and Regular expression; files to include and exclude; matches grouped by file with
// the hit marked. ↓ from the query enters the results; ↑/↓ move; Enter opens the file at the
// match. Mod+Shift+F focuses the query. The results are one Tab stop and are built lazily.

import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../app/app_state.dart';
import '../daemon/protocol.g.dart';
import '../ui/controls.dart';
import '../state/search.dart';
import '../ui/pressable.dart';
import '../ui/roving.dart';
import '../ui/text_input.dart';
import '../ui/theme.dart';
import '../ui/tokens.dart';

class SearchSide extends StatefulWidget {
  const SearchSide({super.key});

  @override
  State<SearchSide> createState() => SearchSideState();
}

class SearchSideState extends State<SearchSide> {
  late final AppState app = AppScope.of(context);
  late final query = TextEditingController(text: app.search.query);
  final queryFocus = FocusNode(debugLabel: 'search query');
  final _include = TextEditingController();
  final _exclude = TextEditingController();
  final _results = GlobalKey<SearchResultsState>();

  @override
  void initState() {
    super.initState();
    app.focusSearch = () => WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      queryFocus.requestFocus();
      query.selection = TextSelection(baseOffset: 0, extentOffset: query.text.length);
    });
    query.addListener(() {
      if (query.text != app.search.query) app.search.update(query: query.text);
    });
    _include.addListener(() => app.search.update(include: _include.text));
    _exclude.addListener(() => app.search.update(exclude: _exclude.text));
  }

  @override
  void dispose() {
    app.focusSearch = null;
    query.dispose();
    queryFocus.dispose();
    _include.dispose();
    _exclude.dispose();
    super.dispose();
  }

  void _open(SearchMatch m) =>
      unawaited(app.documents.open(m.path, line: m.line, column: m.column));

  /// The line under the fields: what the search found, or why it found nothing.
  static String summary(SearchStore s) {
    if (s.error.isNotEmpty) return s.error;
    if (s.query.isEmpty) return '';
    if (s.running && s.groups.isEmpty) return 'Searching…';
    if (s.total == 0) return 'No results';
    final files = '${s.groups.length} file${s.groups.length == 1 ? '' : 's'}';
    if (s.truncated) {
      // The daemon stops at its limit: say what is shown and how to get the rest (AUD-067).
      return 'Showing the first ${s.total} results, in $files. Refine the search or add files '
          'to include to see the rest.';
    }
    return '${s.total} result${s.total == 1 ? '' : 's'} in $files';
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final s = app.search;
    Widget toggle(String label, String text, bool on, VoidCallback flip) => Pressable(
      label: label,
      hint: label,
      role: PressRole.toggle,
      checked: on,
      onPressed: flip,
      builder: (context, st) => FocusRing(
        visible: st.focused,
        child: Container(
          width: 26,
          height: Metrics.controlHeightSmall,
          alignment: Alignment.center,
          decoration: BoxDecoration(
            color: on ? c.selected : (st.hovered ? c.hover : null),
            borderRadius: BorderRadius.circular(Metrics.radiusControl),
          ),
          child: Text(text, style: t.type.codeSmall.copyWith(color: on ? c.ink : c.ink3)),
        ),
      ),
    );
    const gutter = EdgeInsets.fromLTRB(Metrics.sideGutter, 0, Metrics.sideGutter, Metrics.s2);
    return ListenableBuilder(
      listenable: Listenable.merge([s, app.workspace]),
      builder: (context, _) {
        if (!app.workspace.isOpen) {
          return Padding(
            padding: const EdgeInsets.all(Metrics.sideGutter),
            child: Text('Open a folder to search it.', style: t.type.small),
          );
        }
        final text = summary(s);
        // Tab follows the page's order: the query, its three options, the globs, the results.
        return FocusTraversalGroup(
          policy: WidgetOrderTraversalPolicy(),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: gutter,
                child: Focus(
                  canRequestFocus: false,
                  skipTraversal: true,
                  onKeyEvent: (_, e) {
                    if (e is! KeyDownEvent) return KeyEventResult.ignored;
                    if (e.logicalKey == LogicalKeyboardKey.enter) {
                      app.search.now();
                      return KeyEventResult.handled;
                    }
                    if (e.logicalKey == LogicalKeyboardKey.arrowDown && s.groups.isNotEmpty) {
                      _results.currentState?.focusRow(0);
                      return KeyEventResult.handled;
                    }
                    return KeyEventResult.ignored;
                  },
                  child: AxTextInput(
                    label: 'Search',
                    controller: query,
                    focusNode: queryFocus,
                    placeholder: 'Search',
                    trailing: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        toggle(
                          'Match case',
                          'Aa',
                          s.caseSensitive,
                          () => s.update(caseSensitive: !s.caseSensitive),
                        ),
                        const SizedBox(width: Metrics.s1),
                        toggle(
                          'Whole word',
                          'ab',
                          s.wholeWord,
                          () => s.update(wholeWord: !s.wholeWord),
                        ),
                        const SizedBox(width: Metrics.s1),
                        toggle(
                          'Regular expression',
                          '.*',
                          s.regex,
                          () => s.update(regex: !s.regex),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
              Padding(
                padding: gutter,
                child: AxTextInput(
                  label: 'Files to include',
                  controller: _include,
                  mono: true,
                  placeholder: 'Files to include (e.g. src/**, *.rs)',
                ),
              ),
              Padding(
                padding: gutter,
                child: AxTextInput(
                  label: 'Files to exclude',
                  controller: _exclude,
                  mono: true,
                  placeholder: 'Files to exclude',
                ),
              ),
              if (text.isNotEmpty)
                Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: Metrics.sideGutter,
                    vertical: Metrics.s2,
                  ),
                  // Announced when the search settles, not on every keystroke's "Searching…".
                  child: Semantics(
                    liveRegion: !s.running,
                    child: Text(
                      text,
                      style: t.type.tiny.copyWith(color: s.error.isNotEmpty ? c.dangerText : null),
                    ),
                  ),
                ),
              Expanded(
                child: SearchResults(
                  key: _results,
                  groups: s.groups,
                  onOpen: _open,
                  onLeaveTop: queryFocus.requestFocus,
                ),
              ),
            ],
          ),
        );
      },
    );
  }
}

/// The results, grouped by file: one lazily built list of file headings and match rows (only
/// the rows on screen are built; AUD-050). One Tab stop; ↑/↓ move between matches (↑ from the
/// first goes back to the query); Enter opens the file at the match.
class SearchResults extends StatefulWidget {
  const SearchResults({super.key, required this.groups, required this.onOpen, this.onLeaveTop});
  final List<MatchGroup> groups;
  final ValueChanged<SearchMatch> onOpen;
  final VoidCallback? onLeaveTop;

  @override
  State<SearchResults> createState() => SearchResultsState();
}

/// One item of the flattened results: a file's heading or a match.
typedef _Item = ({MatchGroup? group, SearchMatch? match, int k});

class SearchResultsState extends State<SearchResults> {
  final _scroll = ScrollController();
  final _nodes = RowFocusNodes(debugPrefix: 'result');
  List<_Item> _items = const [];

  /// Index in [_items] of each match, in order.
  List<int> _matchAt = const [];
  List<MatchGroup>? _builtFor;
  int _current = 0;
  bool _focused = false;
  double _head = 26, _row = 22;

  static String _id(SearchMatch m) => '${m.path}:${m.line}:${m.column}';

  void _flatten() {
    if (identical(_builtFor, widget.groups)) return;
    _builtFor = widget.groups;
    final items = <_Item>[];
    final at = <int>[];
    for (final g in widget.groups) {
      items.add((group: g, match: null, k: -1));
      for (final m in g.matches) {
        items.add((group: null, match: m, k: at.length));
        at.add(items.length - 1);
      }
    }
    _items = items;
    _matchAt = at;
    if (_current >= at.length) _current = 0;
  }

  double _extent(int i) => _items[i].group != null ? _head : _row;

  /// The number of match rows built (tests: only the visible ones are).
  @visibleForTesting
  int get builtRows => _nodes.length;

  /// Focuses the [n]th match (scrolling it into view first).
  void focusRow(int n) {
    if (_matchAt.isEmpty) return;
    n = n.clamp(0, _matchAt.length - 1);
    setState(() => _current = n);
    final index = _matchAt[n];
    if (_scroll.hasClients) {
      var top = Metrics.s1;
      for (var i = 0; i < index; i++) {
        top += _extent(i);
      }
      final view = _scroll.position.viewportDimension;
      // The match's file heading shows with the first match.
      final from = n == 0 || _matchAt[n - 1] != index - 1 ? top - _head : top;
      if (from < _scroll.offset) {
        _scroll.jumpTo(from.clamp(0, _scroll.position.maxScrollExtent));
      } else if (top + _row > _scroll.offset + view) {
        _scroll.jumpTo((top + _row - view).clamp(0, _scroll.position.maxScrollExtent));
      }
    }
    final id = _id(_items[index].match!);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _nodes[id].requestFocus();
    });
  }

  KeyEventResult _key(FocusNode _, KeyEvent e) {
    if (e is! KeyDownEvent && e is! KeyRepeatEvent) return KeyEventResult.ignored;
    switch (e.logicalKey) {
      case LogicalKeyboardKey.enter:
        if (_current < _matchAt.length) widget.onOpen(_items[_matchAt[_current]].match!);
        return KeyEventResult.handled;
      case LogicalKeyboardKey.arrowDown:
        focusRow(_current + 1);
        return KeyEventResult.handled;
      case LogicalKeyboardKey.arrowUp:
        if (_current == 0 && widget.onLeaveTop != null) {
          widget.onLeaveTop!();
        } else {
          focusRow(_current - 1);
        }
        return KeyEventResult.handled;
      case LogicalKeyboardKey.home:
        focusRow(0);
        return KeyEventResult.handled;
      case LogicalKeyboardKey.end:
        focusRow(_matchAt.length - 1);
        return KeyEventResult.handled;
      default:
        return KeyEventResult.ignored;
    }
  }

  @override
  void dispose() {
    _scroll.dispose();
    _nodes.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    _flatten();
    _head = context.scaled(26, fontSize: 12, lineHeight: 15.6);
    _row = context.scaled(22, fontSize: 12, lineHeight: 16.8);
    return RovingFocus(
      enabled: _matchAt.isNotEmpty,
      onEnter: () => focusRow(_current),
      onFocusChange: (v) => setState(() => _focused = v),
      child: Focus(
        canRequestFocus: false,
        skipTraversal: true,
        onKeyEvent: _key,
        child: ListView.builder(
          controller: _scroll,
          padding: const EdgeInsets.symmetric(
            horizontal: Metrics.sideListInset,
            vertical: Metrics.s1,
          ),
          itemCount: _items.length,
          itemExtentBuilder: (i, _) => i < _items.length ? _extent(i) : null,
          itemBuilder: (context, i) {
            final it = _items[i];
            final g = it.group;
            if (g != null) {
              final n = g.matches.length;
              return Semantics(
                container: true,
                header: true,
                label: '${g.path}, $n match${n == 1 ? '' : 'es'}',
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
                      Expanded(
                        child: Text(
                          g.path,
                          style: t.type.label,
                          overflow: TextOverflow.ellipsis,
                          maxLines: 1,
                        ),
                      ),
                      AxBadge(n, attention: false),
                    ],
                  ),
                ),
              );
            }
            final m = it.match!;
            final k = it.k;
            return _result(
              context,
              m,
              selected: _focused && k == _current,
              onTap: () {
                setState(() => _current = k);
                widget.onOpen(m);
              },
            );
          },
        ),
      ),
    );
  }

  Widget _result(
    BuildContext context,
    SearchMatch m, {
    required bool selected,
    required VoidCallback onTap,
  }) {
    final t = context.axiom;
    final c = t.colors;
    final a = (m.previewColumn - 1).clamp(0, m.preview.length);
    final b = (a + m.endColumn - m.column).clamp(a, m.preview.length);
    final style = t.type.codeSmall.copyWith(color: c.ink2);
    return AxListRow(
      label: '${m.path}, line ${m.line}: ${m.preview.trim()}',
      hint: 'Open at the match — Enter',
      focusNode: _nodes[_id(m)],
      skipTraversal: true,
      selected: selected,
      listFocused: _focused,
      height: 22,
      onPressed: onTap,
      child: Padding(
        padding: const EdgeInsets.only(left: Metrics.s5 + Metrics.s3, right: Metrics.rowInset),
        child: Row(
          children: [
            SizedBox(
              width: 34,
              child: Text('${m.line}', style: t.type.numeric.copyWith(fontSize: 11)),
            ),
            Expanded(
              child: Text.rich(
                TextSpan(
                  children: [
                    TextSpan(text: m.preview.substring(0, a)),
                    TextSpan(
                      text: m.preview.substring(a, b),
                      style: style.copyWith(
                        color: c.ink,
                        backgroundColor: c.infoSoft,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    TextSpan(text: m.preview.substring(b)),
                  ],
                ),
                style: style,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
              ),
            ),
          ],
        ),
      ),
    );
  }
}
