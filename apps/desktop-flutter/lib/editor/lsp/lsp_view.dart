// The editor's language features in the view (FL-EDIT-002): the completion popup (Ctrl+Space,
// trigger characters, quick suggestions while typing a word), the hover card, F12 (definition),
// F2 (rename across files, unsaved and undoable), and Format Document. Popups sit at the caret:
// the font is monospaced and lines have a fixed height, so the caret's place follows from the
// line, the column, the gutter and the scroll offsets.

import 'dart:async';

import 'package:flutter/foundation.dart' show TargetPlatform, defaultTargetPlatform;
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:re_editor/re_editor.dart';

import '../../app/app_state.dart';
import '../../daemon/client.dart';
import '../../daemon/protocol.g.dart' show ServerState;
import '../../state/documents.dart';
import '../../state/lsp.dart';
import '../../chat/markdown_view.dart';
import '../../ui/fuzzy.dart';
import '../../ui/overlays.dart';
import '../../ui/surfaces.dart';
import '../../ui/text_input.dart';
import '../../ui/theme.dart';
import '../../ui/tokens.dart';
import '../snippets.dart';
import 'semantic_tokens.dart';
import 'lsp_editor.dart';

/// Geometry of the code view, for placing popups at a text position.
final class CodeGeometry {
  CodeGeometry({
    required this.fontSize,
    required this.gutterKey,
    required this.scroll,
    this.fontFamily,
    this.padLeft = 6,
    this.padTop = 6,
  });
  final double fontSize;

  /// The editor's font (null or empty: the default mono), for text drawn in line with the code.
  final String? fontFamily;
  String get family => (fontFamily == null || fontFamily!.isEmpty) ? AxiomType.mono : fontFamily!;
  final GlobalKey gutterKey;
  final CodeScrollController scroll;
  final double padLeft, padTop;
  double? _charWidth;

  double get lineHeight => fontSize * 1.5;

  double get charWidth => _charWidth ??= () {
    final tp = TextPainter(
      text: TextSpan(
        text: 'M' * 64,
        style: TextStyle(fontFamily: family, fontSize: fontSize),
      ),
      textDirection: TextDirection.ltr,
    )..layout();
    return tp.width / 64;
  }();

  double get gutter =>
      (gutterKey.currentContext?.findRenderObject() as RenderBox?)?.size.width ?? 48;
  double get _v => scroll.verticalScroller.hasClients ? scroll.verticalScroller.offset : 0;
  double get _h => (scroll.horizontalScroller.hasClients) ? scroll.horizontalScroller.offset : 0;

  /// Top-left of the character at [line]:[column] (0-based) in the view.
  Offset at(int line, int column, String lineText) {
    var cols = 0;
    for (var i = 0; i < column && i < lineText.length; i++) {
      cols += lineText.codeUnitAt(i) == 9 ? 4 - (cols % 4) : 1;
    }
    return Offset(gutter + padLeft + cols * charWidth - _h, padTop + line * lineHeight - _v);
  }

  /// The text position under [p] in the view, or null outside the text.
  ({int line, int column})? positionAt(Offset p, CodeLines lines) {
    final line = ((p.dy - padTop + _v) / lineHeight).floor();
    if (line < 0 || line >= lines.length) return null;
    final x = p.dx - gutter - padLeft + _h;
    if (x < 0) return null;
    final col = (x / charWidth).floor();
    if (col >= lines[line].length) return null;
    return (line: line, column: col);
  }
}

class LspEditorFeatures extends StatefulWidget {
  const LspEditorFeatures({
    super.key,
    required this.doc,
    required this.controller,
    required this.geometry,
    required this.builder,
  });
  final Doc doc;
  final CodeLineEditingController controller;
  final CodeGeometry geometry;

  /// Builds the code view with the key overrides these features need while a popup is open.
  final Widget Function(BuildContext context, Map<Type, Action<Intent>> overrides) builder;

  @override
  State<LspEditorFeatures> createState() => LspEditorFeaturesState();
}

class LspEditorFeaturesState extends State<LspEditorFeatures> {
  late final AppState app = AppScope.of(context);
  LspStore get lsp => app.lsp;
  String? get _server => lsp.serverFor(lspLanguageId(widget.doc))?.serverId;

  // completion
  List<CompletionChoice> _items = const [];
  List<({CompletionChoice item, FuzzyMatch match})> _shown = const [];
  int _active = 0;
  ({int line, int start})? _anchor; // where the completed word starts
  CancelToken? _pending;
  Timer? _quick;
  final _listScroll = ScrollController();

  // the documentation of the selected item (completionItem/resolve)
  Timer? _resolveTimer;
  CancelToken? _resolvePending;

  // an accepted snippet's tab stops: absolute, in Tab order; [_stop] is the current one
  List<({int index, int line, int start, int end})>? _stops;
  int _stop = 0;

  // hover
  Timer? _hoverTimer;
  ({Offset at, String text})? _hover;
  CancelToken? _hoverPending;

  CodeLines? _lastLines;

  // signature help
  ({String label, List<(int, int)> params, int active, String? doc})? _signature;
  CancelToken? _sigPending;

  // document highlights
  Timer? _highlightTimer;
  CancelToken? _highlightPending;
  CodeLinePosition? _lastCaret;

  bool get completing => _anchor != null;

  /// The signature help shown (tests).
  @visibleForTesting
  String? get signatureLabel => _signature?.label;

  Map<String, Object?>? get _caps {
    final c = lsp.servers.where((s) => s.serverId == _server).firstOrNull?.capabilities;
    return c is Map<String, Object?> ? c : (c is Map ? c.cast<String, Object?>() : null);
  }

  List<String> _capTriggers(String provider, {List<String> fallback = const []}) {
    final p = _caps?[provider];
    return p is Map
        ? (p['triggerCharacters'] as List? ?? const []).whereType<String>().toList()
        : fallback;
  }

  /// The labels the popup shows (tests).
  @visibleForTesting
  List<String> get shownLabels => [for (final s in _shown) s.item.label];

  @override
  void initState() {
    super.initState();
    widget.controller.addListener(_onEdit);
    _lastLines = widget.controller.codeLines;
    lsp.addListener(_onServers);
    _scheduleStructure();
  }

  // ---- semantic tokens (ADR-0021 §2) ---------------------------------------------------------

  Timer? _structureTimer;
  CancelToken? _structurePending;
  String? _structureServer;

  /// A server became ready (or changed): ask again.
  void _onServers() {
    final s = _server;
    final ready = lsp.servers.where((x) => x.serverId == s).firstOrNull?.state == ServerState.ready;
    if (ready && _structureServer != s) _scheduleStructure();
  }

  void _scheduleStructure() {
    _structureTimer?.cancel();
    _structureTimer = Timer(const Duration(milliseconds: 250), () => unawaited(_structure()));
  }

  Future<void> _structure() async {
    final server = _server;
    final caps = _caps;
    if (server == null || caps == null || !mounted) return;
    final info = lsp.servers.where((x) => x.serverId == server).firstOrNull;
    if (info?.state != ServerState.ready) return;
    _structureServer = server;
    _structurePending?.cancel();
    final cancel = _structurePending = CancelToken();
    final doc = {
      'textDocument': {'uri': lsp.uriOf(widget.doc.path)},
    };
    final sem = caps['semanticTokensProvider'];
    // Files over 1 MiB keep the grammar alone.
    final small =
        (textLength(widget.controller.codeLines, 1) ?? widget.controller.text.length) <= 1 << 20;
    if (sem is Map && sem['full'] != null && sem['full'] != false && small) {
      try {
        final r = await lsp.request(
          server,
          'textDocument/semanticTokens/full',
          doc,
          cancel: cancel,
        );
        final legend = sem['legend'];
        if (!mounted || cancel.isCancelled || r is! Map || legend is! Map) return;
        final data = [...?(r['data'] as List?)?.whereType<num>().map((n) => n.toInt())];
        widget.doc.setSemantic(
          decodeSemanticTokens(
            data,
            [...?(legend['tokenTypes'] as List?)?.whereType<String>()],
            [...?(legend['tokenModifiers'] as List?)?.whereType<String>()],
            context.colors.syntax,
          ),
        );
      } on DaemonException {
        // The grammar's colours stay.
      }
    }
  }

  @override
  void dispose() {
    widget.controller.removeListener(_onEdit);
    _pending?.cancel();
    _sigPending?.cancel();
    _highlightPending?.cancel();
    _highlightTimer?.cancel();
    _hoverPending?.cancel();
    _quick?.cancel();
    _hoverTimer?.cancel();
    _resolveTimer?.cancel();
    _resolvePending?.cancel();
    _structureTimer?.cancel();
    _structurePending?.cancel();
    lsp.removeListener(_onServers);
    _listScroll.dispose();
    super.dispose();
  }

  Map<String, Object?> _docPos([CodeLinePosition? p]) {
    final s = widget.controller.selection;
    return {
      'textDocument': {'uri': lsp.uriOf(widget.doc.path)},
      'position': {'line': p?.index ?? s.extentIndex, 'character': p?.offset ?? s.extentOffset},
    };
  }

  static final _word = RegExp(r'[A-Za-z0-9_$]');

  /// The start of the word ending at the caret.
  int _wordStart() {
    final s = widget.controller.selection;
    final text = widget.controller.codeLines[s.extentIndex].text;
    var i = s.extentOffset.clamp(0, text.length);
    while (i > 0 && _word.hasMatch(text[i - 1])) {
      i--;
    }
    return i;
  }

  List<String> get _triggers {
    final caps = lsp.servers.where((s) => s.serverId == _server).firstOrNull?.capabilities;
    final cp = caps is Map ? caps['completionProvider'] : null;
    return cp is Map
        ? (cp['triggerCharacters'] as List? ?? const []).whereType<String>().toList()
        : const ['.'];
  }

  void _onEdit() {
    final c = widget.controller;
    final now = c.codeLines;
    final before = _lastLines;
    final typed = !identical(now, before);
    _lastLines = now;
    if (typed && before != null) _followStops(before, now);
    if (typed && completing && _commit(now)) return;
    _hide();
    _scheduleHighlight(edited: typed);
    if (typed) _scheduleStructure();
    if (typed && c.selection.isCollapsed) {
      final s = c.selection;
      final line = c.codeLines[s.extentIndex].text;
      final ch = s.extentOffset > 0 && s.extentOffset <= line.length
          ? line[s.extentOffset - 1]
          : '';
      final sigTriggers = _capTriggers('signatureHelpProvider');
      if (ch == ')') {
        _closeSignature();
      } else if (sigTriggers.contains(ch) || (_signature != null && ch == ',')) {
        unawaited(signatureHelp());
      }
    } else if (_signature != null && c.selection.extentIndex != _lastCaret?.index) {
      _closeSignature();
    }
    if (!typed || !c.selection.isCollapsed) {
      if (!completing && _pending != null) {
        // The caret moved before the list came: it will not be shown.
        _quick?.cancel();
        _pending?.cancel();
      }
      if (completing &&
          (c.selection.extentIndex != _anchor!.line || c.selection.extentOffset < _anchor!.start)) {
        _close();
      }
      return;
    }
    final s = c.selection;
    final text = c.codeLines[s.extentIndex].text;
    final ch = s.extentOffset > 0 && s.extentOffset <= text.length ? text[s.extentOffset - 1] : '';
    if (completing) {
      if (s.extentIndex != _anchor!.line ||
          s.extentOffset < _anchor!.start ||
          (ch.isNotEmpty && !_word.hasMatch(ch) && !_triggers.contains(ch))) {
        _close();
      } else if (_triggers.contains(ch)) {
        unawaited(complete());
      } else {
        _filter();
      }
      return;
    }
    if (!_word.hasMatch(ch) && !_triggers.contains(ch)) {
      // Out of the word: a request on its way is for the word left behind.
      _quick?.cancel();
      _pending?.cancel();
    }
    if (_triggers.contains(ch)) {
      unawaited(complete());
    } else if (_word.hasMatch(ch) && s.extentOffset - _wordStart() >= 1) {
      // As you type a word (ADR-0021 §7): after the first character, debounced.
      _quick?.cancel();
      _quick = Timer(const Duration(milliseconds: 30), () => unawaited(complete()));
    }
  }

  /// Ctrl+Space, a trigger character, or a pause in a word.
  Future<void> complete() async {
    final server = _server;
    final state = lsp.servers.where((s) => s.serverId == server).firstOrNull?.state;
    // No server, or one that cannot answer: the document's words.
    if (server == null || state == ServerState.missing || state == ServerState.failed) {
      _completeWords();
      return;
    }
    _pending?.cancel();
    final token = _pending = CancelToken();
    final line = widget.controller.selection.extentIndex;
    final start = _wordStart();
    try {
      final r = await lsp.request(server, 'textDocument/completion', _docPos(), cancel: token);
      if (!mounted || token.isCancelled) return;
      // The caret left the word meanwhile (Enter, a click, a non-word character): the reply is
      // for a place no longer being typed at, and its edits would land wrong (audit finding 6).
      final now = widget.controller.selection;
      if (!now.isCollapsed || now.extentIndex != line || now.extentOffset < start) return;
      _items = parseCompletion(r);
      _anchor = (line: line, start: start);
      _filter();
    } on DaemonException {
      // A cancelled or failed request shows nothing.
    }
  }

  /// Where no server serves the language: the document's words (ADR-0021 §7).
  void _completeWords() {
    final c = widget.controller;
    final s = c.selection;
    final start = _wordStart();
    final current = c.codeLines[s.extentIndex].text.substring(start, s.extentOffset);
    // The lines around the caret (a long file is not scanned whole on every keystroke).
    final from = (s.extentIndex - 1000).clamp(0, c.codeLines.length);
    final to = (s.extentIndex + 1000).clamp(0, c.codeLines.length);
    _items = [
      for (final w in documentWords([
        for (var i = from; i < to; i++) c.codeLines[i].text,
      ], current: current).take(500))
        CompletionChoice(label: w, insert: w, kind: 1),
    ];
    _anchor = (line: s.extentIndex, start: start);
    _filter();
  }

  void _filter() {
    final a = _anchor;
    if (a == null) return;
    final s = widget.controller.selection;
    final text = widget.controller.codeLines[s.extentIndex].text;
    final prefix = text.substring(
      a.start.clamp(0, text.length),
      s.extentOffset.clamp(0, text.length),
    );
    final ranked = rankFuzzy(_items, prefix, (i) => i.filter ?? i.label, limit: 100);
    if (prefix.isEmpty) {
      ranked.sort((x, y) => (x.item.sort ?? x.item.label).compareTo(y.item.sort ?? y.item.label));
    }
    final pre = prefix.isEmpty ? ranked.indexWhere((r) => r.item.preselect) : -1;
    setState(() {
      _shown = ranked;
      _active = pre < 0 ? 0 : pre;
      if (_shown.isEmpty) _anchor = null;
    });
    _scheduleResolve();
  }

  /// The selected item's documentation, from the server when the item came without it.
  void _scheduleResolve() {
    _resolveTimer?.cancel();
    if (_shown.isEmpty) return;
    final item = _shown[_active].item;
    final server = _server;
    final caps = _caps?['completionProvider'];
    if (item.documentation != null || item.raw == null || server == null) return;
    if (caps is! Map || caps['resolveProvider'] != true) return;
    _resolveTimer = Timer(const Duration(milliseconds: 60), () async {
      _resolvePending?.cancel();
      final token = _resolvePending = CancelToken();
      try {
        final r = await lsp.request(server, 'completionItem/resolve', item.raw, cancel: token);
        if (!mounted || token.isCancelled) return;
        final done = item.resolved(r);
        setState(() {
          _items = [for (final i in _items) identical(i, item) ? done : i];
          _shown = [
            for (final x in _shown) identical(x.item, item) ? (item: done, match: x.match) : x,
          ];
        });
      } on DaemonException {
        // No documentation then.
      }
    });
  }

  void _close() {
    _pending?.cancel();
    if (_anchor == null && _shown.isEmpty) return;
    setState(() {
      _anchor = null;
      _shown = const [];
    });
  }

  void _accept([int? index]) {
    final a = _anchor;
    if (a == null || _shown.isEmpty) return;
    final item = _shown[index ?? _active].item;
    final c = widget.controller;
    final s = c.selection;
    final from = item.range != null && item.range!.start.line == s.extentIndex
        ? item.range!.start.character
        : a.start;
    final parsed = item.snippet
        ? parseSnippet(item.insert)
        : (text: item.insert, stops: const <TabStop>[]);
    final start = (line: s.extentIndex, character: from);
    // The item and its additional edits (an import) are one undo step.
    final at = applyEditsTracking(c, [
      (
        range: (start: start, end: (line: s.extentIndex, character: s.extentOffset)),
        text: parsed.text,
      ),
      ...item.additional,
    ], start);
    _close();
    final stops = [
      for (final t in parsed.stops)
        () {
          final p0 = positionIn(parsed.text, t.start, at.line, at.character);
          final p1 = positionIn(parsed.text, t.end, at.line, at.character);
          return (
            index: t.index,
            line: p0.line,
            start: p0.column,
            end: p1.line == p0.line ? p1.column : p0.column,
          );
        }(),
    ];
    if (item.snippet && stops.length > 1) {
      _stops = stops;
      _goToStop(0);
    } else {
      final end = positionIn(parsed.text, parsed.text.length, at.line, at.character);
      final last = parsed.stops.isEmpty ? null : stops.last;
      c.selection = CodeLineSelection.collapsed(
        index: last?.line ?? end.line,
        offset: last?.start ?? end.column,
      );
      _stops = null;
    }
    _lastLines = c.codeLines;
  }

  /// Selects tab stop [i] (its placeholder text), the last one ends the snippet.
  void _goToStop(int i) {
    final stops = _stops;
    if (stops == null || stops.isEmpty) return;
    _stop = i.clamp(0, stops.length - 1);
    final t = stops[_stop];
    widget.controller.selection = CodeLineSelection(
      baseIndex: t.line,
      baseOffset: t.start,
      extentIndex: t.line,
      extentOffset: t.end,
    );
    if (_stop == stops.length - 1) _stops = null;
  }

  /// Typing inside the current stop moves the stops after it on that line; any other edit ends
  /// the snippet.
  void _followStops(CodeLines before, CodeLines now) {
    final stops = _stops;
    if (stops == null) return;
    final t = stops[_stop];
    final s = widget.controller.selection;
    if (before.length != now.length || s.extentIndex != t.line || t.line >= now.length) {
      _stops = null;
      return;
    }
    final delta = now[t.line].text.length - before[t.line].text.length;
    if (s.extentOffset < t.start || s.extentOffset > t.end + delta.clamp(0, 1 << 20)) {
      _stops = null;
      return;
    }
    _stops = [
      for (final (k, x) in stops.indexed)
        if (k == _stop || (x.index == t.index && x.line == t.line && x.start == t.start))
          (index: x.index, line: x.line, start: x.start, end: x.end + delta)
        else if (x.line == t.line && x.start >= t.end)
          (index: x.index, line: x.line, start: x.start + delta, end: x.end + delta)
        else
          x,
    ];
  }

  /// A commit character typed while the list is open accepts the item, then is typed (ADR-0021
  /// §7). True when it did.
  bool _commit(CodeLines now) {
    if (_shown.isEmpty) return false;
    final item = _shown[_active].item;
    if (item.commit.isEmpty) return false;
    final c = widget.controller;
    final s = c.selection;
    final line = now[s.extentIndex].text;
    if (s.extentOffset == 0 || s.extentOffset > line.length) return false;
    final ch = line[s.extentOffset - 1];
    if (!item.commit.contains(ch)) return false;
    c.replaceSelection(
      '',
      CodeLineSelection(
        baseIndex: s.extentIndex,
        baseOffset: s.extentOffset - 1,
        extentIndex: s.extentIndex,
        extentOffset: s.extentOffset,
      ),
    );
    _accept();
    c.replaceSelection(ch);
    _lastLines = c.codeLines;
    return true;
  }

  void _move(int d) {
    if (_shown.isEmpty) return;
    setState(() => _active = (_active + d + _shown.length) % _shown.length);
    _scheduleResolve();
    final extent = _rowExtent;
    if (_listScroll.hasClients) {
      final top = _active * extent;
      if (top < _listScroll.offset) {
        _listScroll.jumpTo(top);
      } else if (top + extent > _listScroll.offset + _listScroll.position.viewportDimension) {
        _listScroll.jumpTo(top + extent - _listScroll.position.viewportDimension);
      }
    }
  }

  // ---- hover --------------------------------------------------------------------------------

  void _onHover(PointerHoverEvent e) {
    _hoverTimer?.cancel();
    final local = e.localPosition;
    if (_hover != null && (local - _hover!.at).distance > 24) _hide();
    _hoverTimer = Timer(const Duration(milliseconds: 600), () => unawaited(_hoverAt(local)));
  }

  Future<void> _hoverAt(Offset local) async {
    final server = _server;
    if (server == null || completing) return;
    final pos = widget.geometry.positionAt(local, widget.controller.codeLines);
    if (pos == null) return;
    _hoverPending?.cancel();
    final token = _hoverPending = CancelToken();
    try {
      final r = await lsp.request(
        server,
        'textDocument/hover',
        _docPos(CodeLinePosition(index: pos.line, offset: pos.column)),
        cancel: token,
      );
      if (!mounted || token.isCancelled) return;
      final text = hoverText(r);
      if (text.isEmpty) return;
      setState(() => _hover = (at: local, text: text));
    } on DaemonException {
      // Nothing to show.
    }
  }

  void _hide() {
    _hoverTimer?.cancel();
    if (_hover != null) setState(() => _hover = null);
  }

  // ---- navigation and edits ------------------------------------------------------------------

  /// F12: the definition, in its file at its place.
  /// Ctrl+Shift+Space, `(` or `,`: the signature of the call at the caret, its active parameter
  /// in bold.
  Future<void> signatureHelp() async {
    final server = _server;
    if (server == null) return;
    _sigPending?.cancel();
    final cancel = _sigPending = CancelToken();
    try {
      final r = await lsp.request(server, 'textDocument/signatureHelp', _docPos(), cancel: cancel);
      if (!mounted || cancel.isCancelled) return;
      final sigs = r is Map ? r['signatures'] : null;
      if (sigs is! List || sigs.isEmpty) {
        _closeSignature();
        return;
      }
      final at = (r! as Map)['activeSignature'];
      final sig = sigs[(at is int && at < sigs.length) ? at : 0];
      if (sig is! Map) return;
      final label = sig['label'] as String? ?? '';
      final params = <(int, int)>[];
      for (final p in sig['parameters'] as List? ?? const []) {
        final l = p is Map ? p['label'] : null;
        if (l is List && l.length == 2 && l[0] is int && l[1] is int) {
          params.add((l[0] as int, l[1] as int));
        } else if (l is String) {
          final i = label.indexOf(l);
          if (i >= 0) params.add((i, i + l.length));
        }
      }
      final ap = sig['activeParameter'] ?? (r as Map)['activeParameter'];
      final docs = sig['documentation'];
      setState(() {
        _lastCaret = widget.controller.selection.extent;
        _signature = (
          label: label,
          params: params,
          active: ap is int ? ap : 0,
          doc: docs is String ? docs : (docs is Map ? docs['value'] as String? : null),
        );
      });
    } on DaemonException {
      // No signature here.
    }
  }

  void _closeSignature() {
    _sigPending?.cancel();
    if (_signature != null && mounted) setState(() => _signature = null);
  }

  /// The symbol under the caret, highlighted wherever it is in the document (after a pause).
  void _scheduleHighlight({bool edited = false}) {
    _highlightTimer?.cancel();
    if (edited) widget.doc.setHighlights(const []);
    if (_caps?['documentHighlightProvider'] == null ||
        _caps?['documentHighlightProvider'] == false) {
      return;
    }
    _highlightTimer = Timer(const Duration(milliseconds: 250), () => unawaited(_highlight()));
  }

  Future<void> _highlight() async {
    final server = _server;
    if (server == null || !mounted) return;
    _highlightPending?.cancel();
    final cancel = _highlightPending = CancelToken();
    try {
      final r = await lsp.request(
        server,
        'textDocument/documentHighlight',
        _docPos(),
        cancel: cancel,
      );
      if (!mounted || cancel.isCancelled) return;
      widget.doc.setHighlights([
        if (r is List)
          for (final h in r)
            if (h is Map) lspRange(h['range']),
      ]);
    } on DaemonException {
      widget.doc.setHighlights(const []);
    }
  }

  Map<String, Object?> _diagnosticJson(LspDiagnostic d) => {
    'range': {
      'start': {'line': d.range.start.line, 'character': d.range.start.character},
      'end': {'line': d.range.end.line, 'character': d.range.end.character},
    },
    'severity': d.severity,
    'message': d.message,
    'source': ?d.source,
    'code': ?d.code,
  };

  /// Ctrl+.: the server's code actions for the selection (or the caret's line), in a picker;
  /// one runs its edit and then its command.
  Future<void> codeActions() async {
    final server = _server;
    if (server == null) return;
    final c = widget.controller;
    final sel = c.selection;
    final start = sel.isCollapsed
        ? (sel.extentIndex, sel.extentOffset)
        : (sel.startIndex, sel.startOffset);
    final end = sel.isCollapsed
        ? (sel.extentIndex, sel.extentOffset)
        : (sel.endIndex, sel.endOffset);
    final diags = [
      for (final d in widget.doc.diagnostics)
        if (d.range.start.line <= end.$1 && d.range.end.line >= start.$1) _diagnosticJson(d),
    ];
    final text = _textNow();
    try {
      final r = await lsp.request(server, 'textDocument/codeAction', {
        'textDocument': {'uri': lsp.uriOf(widget.doc.path)},
        'range': {
          'start': {'line': start.$1, 'character': start.$2},
          'end': {'line': end.$1, 'character': end.$2},
        },
        'context': {'diagnostics': diags},
      });
      if (!mounted) return;
      final actions = [
        if (r is List)
          for (final a in r)
            if (a is Map && a['title'] is String) a,
      ];
      if (actions.isEmpty) {
        ToastHost.of(context).show('No code actions here.');
        return;
      }
      app.openPicker?.call(
        label: 'Code actions',
        inputLabel: 'Code action',
        placeholder: 'Choose a code action',
        empty: 'No matching action',
        items: [
          for (final (i, a) in actions.indexed)
            Command(
              id: 'action:$i',
              title: a['title']! as String,
              category: a['isPreferred'] == true ? 'Preferred' : (a['kind'] as String? ?? 'Action'),
              run: () => unawaited(_runAction(server, a, text)),
            ),
        ],
      );
    } on DaemonException catch (e) {
      if (mounted) ToastHost.of(context).show('Code actions: ${e.message}', kind: ToastKind.error);
    }
  }

  Future<void> _runAction(
    String server,
    Map<Object?, Object?> a,
    Map<String, CodeLines> text,
  ) async {
    var action = a;
    // A lazily resolved action.
    if (action['edit'] == null && action['command'] is! String && action['data'] != null) {
      try {
        final r = await lsp.request(server, 'codeAction/resolve', action);
        if (r is Map) action = r;
      } on DaemonException {
        // Use it as it is.
      }
    }
    final edit = action['edit'];
    if (edit != null && !await _applyWorkspaceEdit(edit, text, '${a['title']}')) return;
    final cmd = action['command'];
    final command = cmd is Map ? cmd : (cmd is String ? action : null);
    if (command != null && command['command'] is String) {
      try {
        await lsp.request(server, 'workspace/executeCommand', {
          'command': command['command'],
          'arguments': ?command['arguments'],
        });
      } on DaemonException catch (e) {
        if (mounted) {
          ToastHost.of(context).show('${a['title']}: ${e.message}', kind: ToastKind.error);
        }
      }
    }
  }

  /// The open documents' text now: a server's edits were computed for the text of the moment
  /// the request was sent, and are applied only if none of it changed since (audit finding 5).
  Map<String, CodeLines> _textNow() => {
    for (final d in app.documents.docs) d.path: d.controller.codeLines,
  };

  bool _changedSince(Map<String, CodeLines> text, Iterable<String> paths) => paths.any((p) {
    final then = text[p];
    final d = app.documents.doc(p);
    return then != null && d != null && !identical(d.controller.codeLines, then);
  });

  /// Applies a server's workspace edit, all of it or (a document it edits changed since [text]
  /// was taken) none of it, saying why. False when it was not applied.
  Future<bool> _applyWorkspaceEdit(Object? edit, Map<String, CodeLines> text, String what) async {
    final edits = <String, List<({LspRange range, String text})>>{};
    for (final e in workspaceEdits(edit).entries) {
      final path = lsp.pathOf(e.key);
      if (path != null) edits[path] = e.value;
    }
    if (_changedSince(text, edits.keys)) {
      if (mounted) {
        ToastHost.of(context).show('$what: the text changed meanwhile, so nothing was applied.');
      }
      return false;
    }
    for (final e in edits.entries) {
      if (app.documents.doc(e.key) == null) {
        await app.documents.open(e.key, background: true, focus: false);
      }
      final d = app.documents.doc(e.key);
      if (d != null) applyEdits(d.controller, e.value);
    }
    return true;
  }

  /// Shift+F12: every reference to the symbol at the caret, in a picker; one opens there.
  Future<void> references() async {
    final server = _server;
    if (server == null) return;
    try {
      final r = await lsp.request(server, 'textDocument/references', {
        ..._docPos(),
        'context': {'includeDeclaration': true},
      });
      if (!mounted) return;
      final locs = <({String path, int line, int column})>[];
      for (final l in r is List ? r : const []) {
        if (l is! Map || l['uri'] is! String) continue;
        final path = lsp.pathOf(l['uri']! as String);
        if (path == null) continue;
        final p = lspRange(l['range']).start;
        locs.add((path: path, line: p.line + 1, column: p.character + 1));
      }
      if (locs.isEmpty) {
        ToastHost.of(context).show('No references found.');
        return;
      }
      String preview(String path, int line) {
        final d = app.documents.doc(path);
        if (d == null || line - 1 >= d.controller.codeLines.length) return '';
        return d.controller.codeLines[line - 1].text.trim();
      }

      app.openPicker?.call(
        label: 'References',
        inputLabel: 'Reference',
        placeholder: '${locs.length} reference${locs.length == 1 ? '' : 's'}',
        empty: 'No matching reference',
        items: [
          for (final (i, l) in locs.indexed)
            Command(
              id: 'ref:$i',
              title: '${l.path}:${l.line}  ${preview(l.path, l.line)}'.trim(),
              category: 'Ln ${l.line}, Col ${l.column}',
              run: () => unawaited(app.documents.open(l.path, line: l.line, column: l.column)),
            ),
        ],
      );
    } on DaemonException catch (e) {
      if (mounted) ToastHost.of(context).show('References: ${e.message}', kind: ToastKind.error);
    }
  }

  /// Mod+T: the workspace's symbols matching the word at the caret (or all), in a picker.
  Future<void> workspaceSymbols() async {
    final server = _server;
    if (server == null) return;
    final c = widget.controller;
    final s = c.selection;
    final line = c.codeLines[s.extentIndex].text;
    var a = s.extentOffset.clamp(0, line.length), b = a;
    while (a > 0 && _word.hasMatch(line[a - 1])) {
      a--;
    }
    while (b < line.length && _word.hasMatch(line[b])) {
      b++;
    }
    try {
      final r = await lsp.request(server, 'workspace/symbol', {'query': line.substring(a, b)});
      if (!mounted) return;
      final items = <Command>[];
      for (final (i, x) in (r is List ? r : const <Object?>[]).indexed) {
        if (x is! Map) continue;
        final loc = x['location'];
        if (loc is! Map || loc['uri'] is! String) continue;
        final path = lsp.pathOf(loc['uri']! as String);
        if (path == null) continue;
        final p = lspRange(loc['range']).start;
        items.add(
          Command(
            id: 'wsym:$i',
            title: '${x['name']}  $path',
            category: 'Ln ${p.line + 1}',
            run: () =>
                unawaited(app.documents.open(path, line: p.line + 1, column: p.character + 1)),
          ),
        );
      }
      if (items.isEmpty) {
        ToastHost.of(context).show('No matching symbol in the workspace.');
        return;
      }
      app.openPicker?.call(
        label: 'Workspace symbols',
        inputLabel: 'Symbol',
        placeholder: '${items.length} symbol${items.length == 1 ? '' : 's'}',
        empty: 'No matching symbol',
        items: items,
      );
    } on DaemonException catch (e) {
      if (mounted) ToastHost.of(context).show('Symbols: ${e.message}', kind: ToastKind.error);
    }
  }

  /// Shift+Alt+O: the server's "organize imports" source action, applied.
  Future<void> organizeImports() async {
    final server = _server;
    if (server == null) return;
    final text = _textNow();
    try {
      final r = await lsp.request(server, 'textDocument/codeAction', {
        'textDocument': {'uri': lsp.uriOf(widget.doc.path)},
        'range': {
          'start': {'line': 0, 'character': 0},
          'end': {'line': widget.controller.codeLines.length, 'character': 0},
        },
        'context': {
          'diagnostics': <Object?>[],
          'only': ['source.organizeImports'],
        },
      });
      if (!mounted) return;
      final action = (r is List ? r : const <Object?>[])
          .whereType<Map<Object?, Object?>>()
          .firstOrNull;
      if (action == null) {
        ToastHost.of(context).show('The imports are already organized.');
        return;
      }
      var a = action;
      if (a['edit'] == null && a['data'] != null) {
        final done = await lsp.request(server, 'codeAction/resolve', a);
        if (done is Map<Object?, Object?>) a = done;
      }
      if (a['edit'] != null) await _applyWorkspaceEdit(a['edit'], text, 'Organize imports');
    } on DaemonException catch (e) {
      if (mounted) {
        ToastHost.of(context).show('Organize imports: ${e.message}', kind: ToastKind.error);
      }
    }
  }

  /// Shift+Alt+H: who calls the symbol at the caret, in a picker.
  Future<void> callHierarchy() async {
    final server = _server;
    if (server == null) return;
    try {
      final items = await lsp.request(server, 'textDocument/prepareCallHierarchy', _docPos());
      final item = (items is List ? items : const <Object?>[]).firstOrNull;
      if (item == null) {
        if (mounted) ToastHost.of(context).show('No call hierarchy here.');
        return;
      }
      final r = await lsp.request(server, 'callHierarchy/incomingCalls', {'item': item});
      if (!mounted) return;
      final out = <Command>[];
      for (final (i, x) in (r is List ? r : const <Object?>[]).indexed) {
        final from = x is Map ? x['from'] : null;
        if (from is! Map || from['uri'] is! String) continue;
        final path = lsp.pathOf(from['uri']! as String);
        if (path == null) continue;
        final ranges = (x as Map)['fromRanges'];
        final at = ranges is List && ranges.isNotEmpty
            ? lspRange(ranges.first).start
            : lspRange(from['range']).start;
        out.add(
          Command(
            id: 'call:$i',
            title: '${from['name']}  $path',
            category: 'Ln ${at.line + 1}',
            run: () =>
                unawaited(app.documents.open(path, line: at.line + 1, column: at.character + 1)),
          ),
        );
      }
      final name = item is Map ? item['name'] : '';
      if (out.isEmpty) {
        ToastHost.of(context).show('Nothing calls $name.');
        return;
      }
      app.openPicker?.call(
        label: 'Calls to $name',
        inputLabel: 'Caller',
        placeholder: '${out.length} caller${out.length == 1 ? '' : 's'}',
        empty: 'No matching caller',
        items: out,
      );
    } on DaemonException catch (e) {
      if (mounted) {
        ToastHost.of(context).show('Call hierarchy: ${e.message}', kind: ToastKind.error);
      }
    }
  }

  /// Ctrl+Shift+O: the document's symbols, in a picker; one goes there.
  Future<void> symbols() async {
    final server = _server;
    if (server == null) return;
    try {
      final r = await lsp.request(server, 'textDocument/documentSymbol', {
        'textDocument': {'uri': lsp.uriOf(widget.doc.path)},
      });
      if (!mounted) return;
      final out = <({String name, String where, int line, int column})>[];
      void walk(Object? list, String container) {
        for (final s in list is List ? list : const []) {
          if (s is! Map || s['name'] is! String) continue;
          final name = s['name']! as String;
          final range =
              s['selectionRange'] ??
              s['range'] ??
              (s['location'] is Map ? (s['location']! as Map)['range'] : null);
          final p = lspRange(range).start;
          out.add((
            name: name,
            where: container.isEmpty ? (s['containerName'] as String? ?? '') : container,
            line: p.line + 1,
            column: p.character + 1,
          ));
          walk(s['children'], container.isEmpty ? name : '$container.$name');
        }
      }

      walk(r, '');
      if (out.isEmpty) {
        ToastHost.of(context).show('No symbols in this file.');
        return;
      }
      final path = widget.doc.path;
      app.openPicker?.call(
        label: 'Go to Symbol',
        inputLabel: 'Symbol',
        placeholder: 'Symbols in ${path.split('/').last}',
        empty: 'No matching symbol',
        items: [
          for (final (i, s) in out.indexed)
            Command(
              id: 'sym:$i',
              title: s.name,
              category: s.where.isEmpty ? 'Ln ${s.line}' : '${s.where} · Ln ${s.line}',
              run: () => unawaited(app.documents.open(path, line: s.line, column: s.column)),
            ),
        ],
      );
    } on DaemonException catch (e) {
      if (mounted) ToastHost.of(context).show('Symbols: ${e.message}', kind: ToastKind.error);
    }
  }

  Future<void> definition() async {
    final server = _server;
    if (server == null) return;
    try {
      final r = await lsp.request(server, 'textDocument/definition', _docPos());
      final loc = firstLocation(r);
      if (loc == null || !mounted) return;
      final path = lsp.pathOf(loc.uri);
      if (path == null) {
        ToastHost.of(context).show(
          'The definition is outside the workspace: ${Uri.decodeComponent(loc.uri.replaceFirst('file://', ''))}',
        );
        return;
      }
      await app.documents.open(path, line: loc.pos.line + 1, column: loc.pos.character + 1);
    } on DaemonException catch (e) {
      if (mounted) {
        ToastHost.of(context).show('Go to definition: ${e.message}', kind: ToastKind.error);
      }
    }
  }

  /// F2: rename the symbol at the caret in every file the server changes (unsaved, undoable).
  Future<void> rename() async {
    final server = _server;
    if (server == null) return;
    final c = widget.controller;
    final s = c.selection;
    final text = c.codeLines[s.extentIndex].text;
    var start = s.extentOffset.clamp(0, text.length), end = start;
    while (start > 0 && _word.hasMatch(text[start - 1])) {
      start--;
    }
    while (end < text.length && _word.hasMatch(text[end])) {
      end++;
    }
    final current = text.substring(start, end);
    if (current.isEmpty) return;
    final name = await showAxDialog<String>(
      context: context,
      label: 'Rename symbol',
      builder: (context) => _RenameBox(initial: current),
    );
    if (name == null || name.isEmpty || name == current || !mounted) return;
    final now = _textNow();
    try {
      final r = await lsp.request(server, 'textDocument/rename', {..._docPos(), 'newName': name});
      await _applyWorkspaceEdit(r, now, 'Rename');
    } on DaemonException catch (e) {
      if (mounted) ToastHost.of(context).show('Rename: ${e.message}', kind: ToastKind.error);
    }
  }

  /// Format Document.
  Future<void> format() async {
    final server = _server;
    if (server == null) return;
    final before = widget.doc.controller.codeLines;
    try {
      final r = await lsp.request(server, 'textDocument/formatting', {
        'textDocument': {'uri': lsp.uriOf(widget.doc.path)},
        'options': {'tabSize': 4, 'insertSpaces': true},
      });
      if (!identical(widget.doc.controller.codeLines, before)) {
        // Computed for the text before the typing since: its ranges would land wrong.
        if (mounted) ToastHost.of(context).show('Format: the text changed meanwhile; try again.');
        return;
      }
      if (r is List && mounted) {
        applyEdits(widget.doc.controller, [
          for (final e in r.whereType<Map<Object?, Object?>>())
            (range: lspRange(e['range']), text: e['newText'] as String? ?? ''),
        ]);
      }
    } on DaemonException catch (e) {
      if (mounted) ToastHost.of(context).show('Format: ${e.message}', kind: ToastKind.error);
    }
  }

  // ---- view -----------------------------------------------------------------------------------

  Map<Type, Action<Intent>> _overrides() => {
    CodeShortcutCursorMoveIntent: CallbackAction<CodeShortcutCursorMoveIntent>(
      onInvoke: (i) {
        if (completing && (i.direction == AxisDirection.up || i.direction == AxisDirection.down)) {
          _move(i.direction == AxisDirection.down ? 1 : -1);
        } else {
          _close();
          widget.controller.moveCursor(i.direction);
        }
        return null;
      },
    ),
    CodeShortcutNewLineIntent: CallbackAction<CodeShortcutNewLineIntent>(
      onInvoke: (_) {
        if (completing) {
          _accept();
        } else {
          widget.controller.applyNewLine();
        }
        return null;
      },
    ),
    CodeShortcutIndentIntent: CallbackAction<CodeShortcutIndentIntent>(
      onInvoke: (_) {
        if (completing) {
          _accept();
        } else if (_stops != null) {
          _goToStop(_stop + 1);
        } else if (app.tabMovesFocus.value) {
          // Ctrl+M: Tab leaves the editor (as in Monaco), for keyboard navigation.
          FocusScope.of(context).nextFocus();
        } else {
          widget.controller.applyIndent();
        }
        return null;
      },
    ),
    CodeShortcutOutdentIntent: CallbackAction<CodeShortcutOutdentIntent>(
      onInvoke: (_) {
        if (_stops != null) {
          _goToStop(_stop - 1);
        } else if (app.tabMovesFocus.value) {
          FocusScope.of(context).previousFocus();
        } else {
          widget.controller.applyOutdent();
        }
        return null;
      },
    ),
    CodeShortcutEscIntent: CallbackAction<CodeShortcutEscIntent>(
      onInvoke: (_) {
        if (_stops != null && !completing) {
          _stops = null;
        } else if (completing || _hover != null || _signature != null) {
          _close();
          _hide();
          _closeSignature();
        } else {
          widget.controller.cancelSelection();
        }
        return null;
      },
    ),
  };

  final _stackKey = GlobalKey();

  /// A completion row's height: the editor's line, at least the shell's row.
  double get _rowExtent => (widget.geometry.fontSize * 1.75).roundToDouble().clamp(22, 40);

  /// The line rectangle at [line]:[column] in this view, from the editor's own layout (the
  /// arithmetic from the font size drifts from it further down a file), else from that.
  Rect _lineRect(int line, int column) {
    final stack = _stackKey.currentContext?.findRenderObject();
    if (stack != null && line < widget.controller.codeLines.length) {
      final r = codeEditorPositionRect(
        stack,
        CodeLinePosition(index: line, offset: column),
        ancestor: stack,
      );
      if (r != null) return r;
    }
    final g = widget.geometry;
    final at = g.at(line, column, widget.controller.codeLines[line].text);
    return Rect.fromLTWH(at.dx, at.dy, 0, g.lineHeight);
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final g = widget.geometry;
    Widget? popup;
    if (completing && _shown.isNotEmpty) {
      final s = widget.controller.selection;
      final line = s.extentIndex;
      final column = _anchor!.start;
      final selected = _shown[_active.clamp(0, _shown.length - 1)].item;
      popup = Positioned.fill(
        child: CustomSingleChildLayout(
          delegate: _AtText(() => _lineRect(line, column), inset: completionTextInset),
          child: _CompletionPopup(
            shown: _shown,
            active: _active,
            scroll: _listScroll,
            rowExtent: _rowExtent,
            text: TextStyle(fontFamily: g.family, fontSize: g.fontSize, height: 1.2),
            charWidth: g.charWidth,
            selected: selected,
            onTap: _accept,
          ),
        ),
      );
    }
    final hover = _hover;
    final sig = _signature;
    Widget? sigCard;
    if (sig != null) {
      final sel = widget.controller.selection;
      final active = sig.active < sig.params.length ? sig.params[sig.active] : null;
      final base = t.type.code.copyWith(fontSize: 12.5, color: c.ink2);
      sigCard = Positioned.fill(
        child: CustomSingleChildLayout(
          delegate: _AtText(
            () => _lineRect(sel.extentIndex, sel.extentOffset),
            inset: 24,
            above: true,
          ),
          child: Semantics(
            liveRegion: true,
            label: 'Signature: ${sig.label}',
            excludeSemantics: true,
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 640),
              child: Sheet(
                radius: Metrics.radiusRaised,
                padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 5),
                child: Text.rich(
                  active == null
                      ? TextSpan(text: sig.label, style: base)
                      : TextSpan(
                          style: base,
                          children: [
                            TextSpan(
                              text: sig.label.substring(0, active.$1.clamp(0, sig.label.length)),
                            ),
                            TextSpan(
                              text: sig.label.substring(
                                active.$1.clamp(0, sig.label.length),
                                active.$2.clamp(0, sig.label.length),
                              ),
                              style: base.copyWith(color: c.ink, fontWeight: FontWeight.w700),
                            ),
                            TextSpan(
                              text: sig.label.substring(active.$2.clamp(0, sig.label.length)),
                            ),
                          ],
                        ),
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
            ),
          ),
        ),
      );
    }
    return Shortcuts(
      shortcuts: {
        const SingleActivator(LogicalKeyboardKey.space, control: true): const _LspIntent(
          'complete',
        ),
        const SingleActivator(LogicalKeyboardKey.f12): const _LspIntent('definition'),
        const SingleActivator(LogicalKeyboardKey.keyM, control: true): const _LspIntent(
          'tab-focus',
        ),
        const SingleActivator(LogicalKeyboardKey.f12, shift: true): const _LspIntent('references'),
        const SingleActivator(LogicalKeyboardKey.period, control: true): const _LspIntent(
          'actions',
        ),
        const SingleActivator(LogicalKeyboardKey.keyO, control: true, shift: true):
            const _LspIntent('symbols'),
        const SingleActivator(LogicalKeyboardKey.space, control: true, shift: true):
            const _LspIntent('signature'),
        const SingleActivator(LogicalKeyboardKey.f2): const _LspIntent('rename'),
        SingleActivator(LogicalKeyboardKey.keyT, control: !_isMac, meta: _isMac): const _LspIntent(
          'workspace-symbols',
        ),
        const SingleActivator(LogicalKeyboardKey.keyO, shift: true, alt: true): const _LspIntent(
          'organize-imports',
        ),
        const SingleActivator(LogicalKeyboardKey.keyH, shift: true, alt: true): const _LspIntent(
          'call-hierarchy',
        ),
        const SingleActivator(LogicalKeyboardKey.keyI, control: true, shift: true):
            const _LspIntent('format'),
        const SingleActivator(LogicalKeyboardKey.keyF, shift: true, alt: true): const _LspIntent(
          'format',
        ),
      },
      child: Actions(
        actions: {
          _LspIntent: CallbackAction<_LspIntent>(
            onInvoke: (i) {
              switch (i.what) {
                case 'complete':
                  unawaited(complete());
                case 'definition':
                  unawaited(definition());
                case 'tab-focus':
                  final on = app.tabMovesFocus.value = !app.tabMovesFocus.value;
                  ToastHost.of(context).show(
                    on
                        ? 'Tab moves focus out of the editor (Ctrl+M to type tabs again)'
                        : 'Tab inserts a tab (Ctrl+M to move focus with it)',
                  );
                case 'references':
                  unawaited(references());
                case 'actions':
                  unawaited(codeActions());
                case 'symbols':
                  unawaited(symbols());
                case 'signature':
                  unawaited(signatureHelp());
                case 'rename':
                  unawaited(rename());
                case 'format':
                  unawaited(format());
                case 'workspace-symbols':
                  unawaited(workspaceSymbols());
                case 'organize-imports':
                  unawaited(organizeImports());
                case 'call-hierarchy':
                  unawaited(callHierarchy());
              }
              return null;
            },
          ),
        },
        child: MouseRegion(
          onHover: _onHover,
          onExit: (_) => _hide(),
          child: Stack(
            key: _stackKey,
            children: [
              Positioned.fill(child: widget.builder(context, _overrides())),
              ?sigCard,
              ?popup,
              if (hover != null)
                Positioned(
                  left: (hover.at.dx - 40).clamp(0, double.infinity),
                  top: hover.at.dy + 18,
                  child: Semantics(
                    liveRegion: true,
                    label: hover.text,
                    excludeSemantics: true,
                    child: ConstrainedBox(
                      constraints: const BoxConstraints(maxWidth: 560, maxHeight: 320),
                      child: Sheet(
                        radius: Metrics.radiusRaised,
                        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                        child: SingleChildScrollView(child: _HoverText(hover.text)),
                      ),
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

class _LspIntent extends Intent {
  const _LspIntent(this.what);
  final String what;
}

/// A letter for the completion item kind (LSP CompletionItemKind).
/// What the documentation pane shows for [item]: its detail as code, then its documentation.
bool get _isMac => defaultTargetPlatform == TargetPlatform.macOS;

/// From a completion popup's left edge to where its rows' names start: the names line up with
/// the word being completed, and the kind badges hang to their left (as in Xcode).
const completionTextInset = _popupPad + _rowPad + _badgeSize + _badgeGap;
const _popupPad = 4.0, _rowPad = 6.0, _badgeSize = 18.0, _badgeGap = 6.0;

/// Places its child at a text position: below the line (above when [above], or when there is
/// no room below), with the child's text [inset] from its left edge on the position. Laid out
/// after the editor in the same pass, so it follows the editor's layout of this frame.
final class _AtText extends SingleChildLayoutDelegate {
  _AtText(this.line, {required this.inset, this.above = false});
  final Rect Function() line;
  final double inset;
  final bool above;

  @override
  BoxConstraints getConstraintsForChild(BoxConstraints constraints) => constraints.loosen();

  @override
  Offset getPositionForChild(Size size, Size child) {
    final r = line();
    const gap = 2.0;
    final below = r.bottom + gap;
    final over = r.top - gap - child.height;
    final fitsBelow = below + child.height <= size.height;
    final y = (above ? (over >= 0 || !fitsBelow) && over >= 0 : !fitsBelow && over >= 0)
        ? over
        : below;
    final x = (r.left - inset).clamp(0.0, (size.width - child.width).clamp(0.0, double.infinity));
    return Offset(x, y.clamp(0.0, double.infinity));
  }

  @override
  bool shouldRelayout(_AtText old) => true;
}

/// The completion list as Xcode has it: rows with a kind badge and the name with its parameters,
/// in the editor's font, under the word being typed; the selected item's declaration and
/// documentation in a panel below the list.
class _CompletionPopup extends StatelessWidget {
  const _CompletionPopup({
    required this.shown,
    required this.active,
    required this.scroll,
    required this.rowExtent,
    required this.text,
    required this.charWidth,
    required this.selected,
    required this.onTap,
  });
  final List<({CompletionChoice item, FuzzyMatch match})> shown;
  final int active;
  final ScrollController scroll;
  final double rowExtent;
  final TextStyle text;
  final double charWidth;
  final CompletionChoice selected;
  final void Function(int) onTap;

  static const _visibleRows = 8;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    var longest = 0;
    for (final (:item, match: _) in shown.take(200)) {
      final n = item.label.length + (item.labelDetail?.length ?? 0);
      if (n > longest) longest = n;
    }
    final width = (completionTextInset + longest * charWidth + _rowPad + _popupPad + 8).clamp(
      320.0,
      680.0,
    );
    final declaration = selected.detail?.trim();
    final doc = selected.documentation?.trim();
    final hasDeclaration = declaration != null && declaration.isNotEmpty;
    final hasDoc = doc != null && doc.isNotEmpty;
    return SizedBox(
      width: width,
      child: Sheet(
        radius: Metrics.radiusSheet,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Semantics(
              label: 'Suggestions',
              container: true,
              liveRegion: true,
              child: SizedBox(
                height: shown.length.clamp(1, _visibleRows) * rowExtent + _popupPad * 2,
                child: Padding(
                  padding: const EdgeInsets.all(_popupPad),
                  child: ListView.builder(
                    controller: scroll,
                    itemExtent: rowExtent,
                    itemCount: shown.length,
                    itemBuilder: (context, i) {
                      final (:item, :match) = shown[i];
                      final on = i == active;
                      return GestureDetector(
                        onTap: () => onTap(i),
                        child: Semantics(
                          selected: on,
                          label: [item.label, if (item.detail != null) item.detail!].join(', '),
                          excludeSemantics: true,
                          child: DecoratedBox(
                            decoration: BoxDecoration(
                              color: on ? c.accentSoft : null,
                              borderRadius: BorderRadius.circular(Metrics.radiusRaised),
                            ),
                            child: Padding(
                              padding: const EdgeInsets.symmetric(horizontal: _rowPad),
                              child: Row(
                                children: [
                                  _KindBadge(kind: item.kind, size: _badgeSize),
                                  const SizedBox(width: _badgeGap),
                                  Expanded(
                                    child: Text.rich(
                                      TextSpan(
                                        style: text.copyWith(color: c.ink),
                                        children: [
                                          _highlightedSpan(item.label, match, c),
                                          if (item.labelDetail case final d?)
                                            TextSpan(
                                              text: d,
                                              style: TextStyle(color: c.ink3),
                                            ),
                                        ],
                                      ),
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      softWrap: false,
                                    ),
                                  ),
                                ],
                              ),
                            ),
                          ),
                        ),
                      );
                    },
                  ),
                ),
              ),
            ),
            if (hasDeclaration || hasDoc) ...[
              SizedBox(height: 1, child: ColoredBox(color: c.hairline)),
              Semantics(
                label: 'Documentation',
                container: true,
                child: ConstrainedBox(
                  constraints: const BoxConstraints(maxHeight: 180),
                  child: SingleChildScrollView(
                    padding: const EdgeInsets.fromLTRB(12, 8, 12, 10),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        if (hasDeclaration)
                          Text(
                            declaration,
                            style: text.copyWith(fontSize: text.fontSize! * 0.92, color: c.ink2),
                            maxLines: 4,
                            overflow: TextOverflow.ellipsis,
                          ),
                        if (hasDeclaration && hasDoc) const SizedBox(height: 6),
                        if (hasDoc) MarkdownView(doc),
                      ],
                    ),
                  ),
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

TextSpan _highlightedSpan(String text, FuzzyMatch match, AxiomColors c) {
  final spans = <TextSpan>[];
  var at = 0;
  for (final (s, e) in match.ranges) {
    if (s > at) spans.add(TextSpan(text: text.substring(at, s)));
    spans.add(
      TextSpan(
        text: text.substring(s, e),
        style: const TextStyle(fontWeight: FontWeight.w700),
      ),
    );
    at = e;
  }
  if (at < text.length) spans.add(TextSpan(text: text.substring(at)));
  return TextSpan(children: spans);
}

/// A completion item's kind as Xcode marks it: a letter on a tinted square, coloured by what
/// the symbol is (types, callables, values, keywords and snippets).
class _KindBadge extends StatelessWidget {
  const _KindBadge({required this.kind, required this.size});
  final int? kind;
  final double size;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final y = c.syntax;
    final k = kind;
    final colour = switch (k) {
      7 || 8 || 9 || 13 || 22 || 25 => y.type,
      2 || 3 || 4 || 23 || 24 => c.accentText,
      5 || 6 || 10 || 11 || 12 || 20 || 21 => y.number,
      14 || 15 => y.keyword,
      _ => c.ink3,
    };
    return Container(
      width: size,
      height: size,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: colour.withValues(alpha: 0.16),
        borderRadius: BorderRadius.circular(Metrics.radiusControl),
      ),
      child: Text(
        _kindLetter(k),
        style: t.type.tiny.copyWith(color: colour, fontWeight: FontWeight.w700, height: 1),
      ),
    );
  }
}

/// Xcode's letters for the LSP CompletionItemKind.
String _kindLetter(int? k) => switch (k) {
  2 || 4 => 'M',
  3 => 'F',
  5 || 10 => 'P',
  6 || 12 || 21 => 'V',
  7 => 'C',
  8 => 'Pr',
  9 => 'Mo',
  11 => 'U',
  13 || 20 => 'E',
  14 => 'K',
  15 => '{}',
  16 => 'Co',
  17 => 'Fi',
  18 => 'R',
  19 => 'Fo',
  22 => 'S',
  23 => 'Ev',
  24 => 'Op',
  25 => 'T',
  _ => 'Ab',
};

/// Hover text: paragraphs as body text, fenced code as code.
class _HoverText extends StatelessWidget {
  const _HoverText(this.text);
  final String text;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final parts = <Widget>[];
    final fence = RegExp(r'```[^\n]*\n([\s\S]*?)```');
    var at = 0;
    for (final m in fence.allMatches(text)) {
      final before = text.substring(at, m.start).trim();
      if (before.isNotEmpty) {
        parts.add(
          Text(
            before.replaceAll('**', '').replaceAll('`', ''),
            style: t.type.small.copyWith(color: t.colors.ink),
          ),
        );
      }
      parts.add(
        Padding(
          padding: const EdgeInsets.symmetric(vertical: 4),
          child: Text(m[1]!.trimRight(), style: t.type.code.copyWith(fontSize: 12.5)),
        ),
      );
      at = m.end;
    }
    final rest = text.substring(at).trim();
    if (rest.isNotEmpty) {
      parts.add(
        Text(
          rest.replaceAll('**', '').replaceAll('`', ''),
          style: t.type.small.copyWith(color: t.colors.ink),
        ),
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: parts,
    );
  }
}

class _RenameBox extends StatefulWidget {
  const _RenameBox({required this.initial});
  final String initial;

  @override
  State<_RenameBox> createState() => _RenameBoxState();
}

class _RenameBoxState extends State<_RenameBox> {
  late final _text = TextEditingController(text: widget.initial)
    ..selection = TextSelection(baseOffset: 0, extentOffset: widget.initial.length);

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  void _go() => Navigator.of(context).pop(_text.text.trim());

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    return SizedBox(
      width: 380,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Rename ${widget.initial}', style: t.type.label),
            const SizedBox(height: Metrics.s4),
            CallbackShortcuts(
              bindings: {const SingleActivator(LogicalKeyboardKey.enter): _go},
              child: AxTextInput(
                label: 'New name',
                controller: _text,
                autofocus: true,
                mono: true,
                onSubmitted: (_) => _go(),
              ),
            ),
            const SizedBox(height: Metrics.s4),
            Text(
              'Enter renames in every file; the changes are unsaved and can be undone.',
              style: t.type.tiny,
            ),
          ],
        ),
      ),
    );
  }
}
