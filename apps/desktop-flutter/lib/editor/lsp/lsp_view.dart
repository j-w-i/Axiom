// The editor's language features in the view (FL-EDIT-002): the completion popup (Ctrl+Space,
// trigger characters, quick suggestions while typing a word), the hover card, F12 (definition),
// F2 (rename across files, unsaved and undoable), and Format Document. Popups sit at the caret:
// the font is monospaced and lines have a fixed height, so the caret's place follows from the
// line, the column, the gutter and the scroll offsets.

import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:re_editor/re_editor.dart';

import '../../app/app_state.dart';
import '../../daemon/client.dart';
import '../../state/documents.dart';
import '../../state/lsp.dart';
import '../../ui/fuzzy.dart';
import '../../ui/overlays.dart';
import '../../ui/surfaces.dart';
import '../../ui/text_input.dart';
import '../../ui/theme.dart';
import '../../ui/tokens.dart';
import 'lsp_editor.dart';

/// Geometry of the code view, for placing popups at a text position.
final class CodeGeometry {
  CodeGeometry({
    required this.fontSize,
    required this.gutterKey,
    required this.scroll,
    this.padLeft = 6,
    this.padTop = 6,
  });
  final double fontSize;
  final GlobalKey gutterKey;
  final CodeScrollController scroll;
  final double padLeft, padTop;
  double? _charWidth;

  double get lineHeight => fontSize * 1.5;

  double get charWidth => _charWidth ??= () {
    final tp = TextPainter(
      text: TextSpan(
        text: 'M' * 64,
        style: TextStyle(fontFamily: AxiomType.mono, fontSize: fontSize),
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
  String? get _server => lsp.serverFor(lspLanguageId(widget.doc.path))?.serverId;

  // completion
  List<CompletionChoice> _items = const [];
  List<({CompletionChoice item, FuzzyMatch match})> _shown = const [];
  int _active = 0;
  ({int line, int start})? _anchor; // where the completed word starts
  CancelToken? _pending;
  Timer? _quick;
  final _listScroll = ScrollController();

  // hover
  Timer? _hoverTimer;
  ({Offset at, String text})? _hover;
  CancelToken? _hoverPending;

  CodeLines? _lastLines;

  bool get completing => _anchor != null;

  /// The labels the popup shows (tests).
  @visibleForTesting
  List<String> get shownLabels => [for (final s in _shown) s.item.label];

  @override
  void initState() {
    super.initState();
    widget.controller.addListener(_onEdit);
    _lastLines = widget.controller.codeLines;
  }

  @override
  void dispose() {
    widget.controller.removeListener(_onEdit);
    _pending?.cancel();
    _hoverPending?.cancel();
    _quick?.cancel();
    _hoverTimer?.cancel();
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
    final typed = !identical(now, _lastLines);
    _lastLines = now;
    _hide();
    if (!typed || !c.selection.isCollapsed) {
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
    if (_triggers.contains(ch)) {
      unawaited(complete());
    } else if (_word.hasMatch(ch) && s.extentOffset - _wordStart() >= 2) {
      // Quick suggestions while typing a word, after a short pause.
      _quick?.cancel();
      _quick = Timer(const Duration(milliseconds: 120), () => unawaited(complete()));
    }
  }

  /// Ctrl+Space, a trigger character, or a pause in a word.
  Future<void> complete() async {
    final server = _server;
    if (server == null) return;
    _pending?.cancel();
    final token = _pending = CancelToken();
    final line = widget.controller.selection.extentIndex;
    final start = _wordStart();
    try {
      final r = await lsp.request(server, 'textDocument/completion', _docPos(), cancel: token);
      if (!mounted || token.isCancelled) return;
      _items = parseCompletion(r);
      _anchor = (line: line, start: start);
      _filter();
    } on DaemonException {
      // A cancelled or failed request shows nothing.
    }
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
    setState(() {
      _shown = ranked;
      _active = 0;
      if (_shown.isEmpty) _anchor = null;
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
    c.replaceSelection(
      item.insert,
      CodeLineSelection(
        baseIndex: s.extentIndex,
        baseOffset: from,
        extentIndex: s.extentIndex,
        extentOffset: s.extentOffset,
      ),
    );
    _close();
  }

  void _move(int d) {
    if (_shown.isEmpty) return;
    setState(() => _active = (_active + d + _shown.length) % _shown.length);
    const extent = 24.0;
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
    try {
      final r = await lsp.request(server, 'textDocument/rename', {..._docPos(), 'newName': name});
      final edits = workspaceEdits(r);
      for (final e in edits.entries) {
        final path = lsp.pathOf(e.key);
        if (path == null) continue;
        if (app.documents.doc(path) == null) {
          await app.documents.open(path, background: true, focus: false);
        }
        final d = app.documents.doc(path);
        if (d != null) applyEdits(d.controller, e.value);
      }
    } on DaemonException catch (e) {
      if (mounted) ToastHost.of(context).show('Rename: ${e.message}', kind: ToastKind.error);
    }
  }

  /// Format Document.
  Future<void> format() async {
    final server = _server;
    if (server == null) return;
    try {
      final r = await lsp.request(server, 'textDocument/formatting', {
        'textDocument': {'uri': lsp.uriOf(widget.doc.path)},
        'options': {'tabSize': 4, 'insertSpaces': true},
      });
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
        } else {
          widget.controller.applyIndent();
        }
        return null;
      },
    ),
    CodeShortcutEscIntent: CallbackAction<CodeShortcutEscIntent>(
      onInvoke: (_) {
        if (completing || _hover != null) {
          _close();
          _hide();
        } else {
          widget.controller.cancelSelection();
        }
        return null;
      },
    ),
  };

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final g = widget.geometry;
    Widget? popup;
    if (completing && _shown.isNotEmpty) {
      final s = widget.controller.selection;
      final at = g.at(
        s.extentIndex,
        _anchor!.start,
        widget.controller.codeLines[s.extentIndex].text,
      );
      popup = Positioned(
        left: at.dx.clamp(0, double.infinity),
        top: at.dy + g.lineHeight + 2,
        child: Semantics(
          label: 'Suggestions',
          container: true,
          liveRegion: true,
          child: SizedBox(
            width: 380,
            height: (_shown.length.clamp(1, 10)) * 24.0 + 8,
            child: Sheet(
              radius: Metrics.radiusRaised,
              padding: const EdgeInsets.all(4),
              child: ListView.builder(
                controller: _listScroll,
                itemExtent: 24,
                itemCount: _shown.length,
                itemBuilder: (context, i) {
                  final (:item, :match) = _shown[i];
                  final on = i == _active;
                  return GestureDetector(
                    onTap: () => _accept(i),
                    child: Semantics(
                      selected: on,
                      label: [item.label, if (item.detail != null) item.detail!].join(', '),
                      excludeSemantics: true,
                      child: DecoratedBox(
                        decoration: BoxDecoration(
                          color: on ? c.raised : null,
                          borderRadius: BorderRadius.circular(Metrics.radiusControl),
                        ),
                        child: Padding(
                          padding: const EdgeInsets.symmetric(horizontal: 8),
                          child: Row(
                            children: [
                              SizedBox(
                                width: 18,
                                child: Text(
                                  _kind(item.kind),
                                  style: t.type.codeSmall.copyWith(color: c.accentText),
                                ),
                              ),
                              Expanded(
                                child: _Highlighted(
                                  text: item.label,
                                  match: match,
                                  style: t.type.code.copyWith(fontSize: 12.5),
                                ),
                              ),
                              if (item.detail != null)
                                Flexible(
                                  child: Text(
                                    item.detail!,
                                    style: t.type.tiny,
                                    overflow: TextOverflow.ellipsis,
                                    textAlign: TextAlign.right,
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
      );
    }
    final hover = _hover;
    return Shortcuts(
      shortcuts: {
        const SingleActivator(LogicalKeyboardKey.space, control: true): const _LspIntent(
          'complete',
        ),
        const SingleActivator(LogicalKeyboardKey.f12): const _LspIntent('definition'),
        const SingleActivator(LogicalKeyboardKey.f2): const _LspIntent('rename'),
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
                case 'rename':
                  unawaited(rename());
                case 'format':
                  unawaited(format());
              }
              return null;
            },
          ),
        },
        child: MouseRegion(
          onHover: _onHover,
          onExit: (_) => _hide(),
          child: Stack(
            children: [
              Positioned.fill(child: widget.builder(context, _overrides())),
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
String _kind(int? k) => switch (k) {
  2 || 3 => 'ƒ',
  4 => 'c',
  5 || 10 => 'p',
  6 => 'v',
  7 || 22 => 'T',
  8 => 'I',
  9 => 'm',
  13 => 'e',
  14 => 'k',
  15 => 's',
  21 => 'π',
  _ => '·',
};

class _Highlighted extends StatelessWidget {
  const _Highlighted({required this.text, required this.match, required this.style});
  final String text;
  final FuzzyMatch match;
  final TextStyle style;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final spans = <TextSpan>[];
    var at = 0;
    for (final (s, e) in match.ranges) {
      if (s > at) spans.add(TextSpan(text: text.substring(at, s)));
      spans.add(
        TextSpan(
          text: text.substring(s, e),
          style: TextStyle(color: c.accentText, fontWeight: FontWeight.w600),
        ),
      );
      at = e;
    }
    if (at < text.length) spans.add(TextSpan(text: text.substring(at)));
    return Text.rich(
      TextSpan(children: spans),
      style: style,
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
    );
  }
}

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
