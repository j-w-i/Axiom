// The Search page (FL-GIT-001; the Tauri shell's Search.tsx, M4-UX-001): a query with Match case,
// Whole word and Regular expression; files to include and exclude; matches grouped by file with
// the hit marked. ↓ from the query enters the results; ↑/↓ move; Enter opens the file at the
// match. Mod+Shift+F focuses the query.

import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../app/app_state.dart';
import '../daemon/protocol.g.dart';
import '../ui/controls.dart';
import '../ui/pressable.dart';
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
  final _nodes = <String, FocusNode>{};

  FocusNode node(String key) => _nodes.putIfAbsent(key, () => FocusNode(debugLabel: key));

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
    for (final n in _nodes.values) {
      n.dispose();
    }
    super.dispose();
  }

  List<(String, SearchMatch)> _flat() => [
    for (final g in app.search.groups)
      for (final m in g.matches) ('${m.path}:${m.line}:${m.column}', m),
  ];

  void _open(SearchMatch m) =>
      unawaited(app.documents.open(m.path, line: m.line, column: m.column));

  KeyEventResult _resultKey(String id, SearchMatch m, KeyEvent e) {
    if (e is! KeyDownEvent) return KeyEventResult.ignored;
    final flat = _flat();
    final i = flat.indexWhere((x) => x.$1 == id);
    if (e.logicalKey == LogicalKeyboardKey.enter) {
      _open(m);
      return KeyEventResult.handled;
    }
    final to = switch (e.logicalKey) {
      LogicalKeyboardKey.arrowDown => i + 1,
      LogicalKeyboardKey.arrowUp => i - 1,
      _ => null,
    };
    if (to == null) return KeyEventResult.ignored;
    if (to < 0) {
      queryFocus.requestFocus();
    } else if (to < flat.length) {
      node(flat[to].$1).requestFocus();
    }
    return KeyEventResult.handled;
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
          height: 22,
          alignment: Alignment.center,
          decoration: BoxDecoration(
            color: on ? c.accentSoft : (st.hovered ? c.hover : null),
            borderRadius: BorderRadius.circular(Metrics.radiusControl),
            border: Border.all(color: on ? c.accent : c.hairline),
          ),
          child: Text(text, style: t.type.codeSmall.copyWith(color: on ? c.accentText : c.ink2)),
        ),
      ),
    );
    return ListenableBuilder(
      listenable: Listenable.merge([s, app.workspace]),
      builder: (context, _) {
        if (!app.workspace.isOpen) {
          return Padding(
            padding: const EdgeInsets.all(Metrics.s5),
            child: Text('Open a folder to search it.', style: t.type.small),
          );
        }
        final flat = _flat();
        final summary = s.error.isNotEmpty
            ? s.error
            : s.query.isEmpty
            ? ''
            : s.running && s.groups.isEmpty
            ? 'Searching…'
            : s.total == 0
            ? 'No results'
            : '${s.total}${s.truncated ? '+' : ''} result${s.total == 1 ? '' : 's'} in '
                  '${s.groups.length} file${s.groups.length == 1 ? '' : 's'}'
                  '${s.truncated ? ' (the first shown)' : ''}';
        // Tab follows the page's order: the query, its three options, the globs, the results.
        return FocusTraversalGroup(
          policy: WidgetOrderTraversalPolicy(),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(Metrics.s3, 0, Metrics.s3, Metrics.s2),
                child: Focus(
                  canRequestFocus: false,
                  skipTraversal: true,
                  onKeyEvent: (_, e) {
                    if (e is! KeyDownEvent) return KeyEventResult.ignored;
                    if (e.logicalKey == LogicalKeyboardKey.enter) {
                      app.search.now();
                      return KeyEventResult.handled;
                    }
                    if (e.logicalKey == LogicalKeyboardKey.arrowDown && flat.isNotEmpty) {
                      node(flat.first.$1).requestFocus();
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
                        const SizedBox(width: 2),
                        toggle(
                          'Whole word',
                          'ab',
                          s.wholeWord,
                          () => s.update(wholeWord: !s.wholeWord),
                        ),
                        const SizedBox(width: 2),
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
                padding: const EdgeInsets.fromLTRB(Metrics.s3, 0, Metrics.s3, Metrics.s2),
                child: AxTextInput(
                  label: 'Files to include',
                  controller: _include,
                  mono: true,
                  placeholder: 'Files to include (e.g. src/**, *.rs)',
                ),
              ),
              Padding(
                padding: const EdgeInsets.fromLTRB(Metrics.s3, 0, Metrics.s3, Metrics.s2),
                child: AxTextInput(
                  label: 'Files to exclude',
                  controller: _exclude,
                  mono: true,
                  placeholder: 'Files to exclude',
                ),
              ),
              if (summary.isNotEmpty)
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: Metrics.s4, vertical: 4),
                  child: Semantics(
                    liveRegion: true,
                    child: Text(
                      summary,
                      style: t.type.tiny.copyWith(color: s.error.isNotEmpty ? c.dangerText : null),
                    ),
                  ),
                ),
              Expanded(
                child: ListView(
                  padding: const EdgeInsets.symmetric(horizontal: Metrics.s2),
                  children: [
                    for (final g in s.groups)
                      Semantics(
                        container: true,
                        explicitChildNodes: true,
                        label:
                            '${g.path}, ${g.matches.length} match${g.matches.length == 1 ? '' : 'es'}',
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            Padding(
                              padding: const EdgeInsets.fromLTRB(Metrics.s3, 6, Metrics.s3, 2),
                              child: Row(
                                children: [
                                  Expanded(
                                    child: Text(
                                      g.path,
                                      style: t.type.label,
                                      overflow: TextOverflow.ellipsis,
                                    ),
                                  ),
                                  AxBadge(g.matches.length, attention: false),
                                ],
                              ),
                            ),
                            for (final m in g.matches) _result(context, m),
                          ],
                        ),
                      ),
                  ],
                ),
              ),
            ],
          ),
        );
      },
    );
  }

  Widget _result(BuildContext context, SearchMatch m) {
    final t = context.axiom;
    final c = t.colors;
    final id = '${m.path}:${m.line}:${m.column}';
    final a = (m.previewColumn - 1).clamp(0, m.preview.length);
    final b = (a + m.endColumn - m.column).clamp(a, m.preview.length);
    final style = t.type.codeSmall.copyWith(color: c.ink2);
    return Focus(
      canRequestFocus: false,
      skipTraversal: true,
      onKeyEvent: (_, e) => _resultKey(id, m, e),
      child: AxListRow(
        label: '${m.path}, line ${m.line}: ${m.preview.trim()}',
        hint: 'Open at the match — Enter',
        focusNode: node(id),
        height: 22,
        onPressed: () => _open(m),
        child: Padding(
          padding: const EdgeInsets.only(left: 18, right: 8),
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
                          backgroundColor: c.accentSoft,
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
      ),
    );
  }
}
