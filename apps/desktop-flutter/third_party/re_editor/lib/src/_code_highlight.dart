part of re_editor;

class _CodeHighlighter extends ValueNotifier<List<_HighlightResult>> {

  final BuildContext _context;
  final _CodeParagraphProvider _provider;
  final _CodeHighlightEngine _engine;

  CodeLineEditingController _controller;
  CodeHighlightTheme? _theme;

  _CodeHighlighter({
    required BuildContext context,
    required CodeLineEditingController controller,
    CodeHighlightTheme? theme,
  }) : _context = context,
    _provider = _CodeParagraphProvider(),
    _controller = controller,
    _theme = theme,
    _engine = _CodeHighlightEngine(theme),
    super(const []) {
    _controller.addListener(_onCodesChanged);
    _processHighlight();
  }

  set controller(CodeLineEditingController value) {
    if (_controller == value) {
      return;
    }
    _controller.removeListener(_onCodesChanged);
    _controller = value;
    _controller.addListener(_onCodesChanged);
    _processHighlight();
  }

  set theme(CodeHighlightTheme? value) {
    if (_theme == value) {
      return;
    }
    _theme = value;
    _engine.theme = value;
    _processHighlight();
  }

  IParagraph build({
    required int index,
    required TextStyle style,
    required double maxWidth,
    int? maxLengthSingleLineRendering,
  }) {
    _provider.updateBaseStyle(style);
    _provider.updateMaxLengthSingleLineRendering(maxLengthSingleLineRendering);
    return _provider.build(_controller.buildTextSpan(
      context: _context,
      index: index,
      textSpan: _buildSpan(index, style),
      style: style
    ), maxWidth);
  }

  void clearCache() {
    _provider.clearCache();
  }

  @override
  void dispose() {
    _controller.removeListener(_onCodesChanged);
    _engine.dispose();
    super.dispose();
  }

  TextSpan _buildSpan(int index, TextStyle style) {
    final String text = _controller.codeLines[index].text;
    if (index >= value.length) {
      return TextSpan(
        text: text,
        style: style
      );
    }
    final _HighlightResult result = value[index];
    if (result.nodes.isEmpty) {
      return TextSpan(
        text: text,
        style: style
      );
    }
    if (result.source == text) {
      return _buildSpanFromNodes(result.nodes, style);
    }
    // Diff the changes and reuse node to avoid style blink.
    final List<_HighlightNode> startNodes = [];
    int start = 0;
    int end = text.length;
    for (int i = 0; i < result.nodes.length && start < end; i++) {
      final String value = result.nodes[i].value;
      if (text.startsWith(value, start)) {
        startNodes.add(result.nodes[i]);
        start += value.length;
      } else {
        break;
      }
    }
    final List<_HighlightNode> endNodes = [];
    for (int i = result.nodes.length - 1; i >= 0 && start < end; i--) {
      final String value = result.nodes[i].value;
      if (text.substring(start, end).endsWith(value)) {
        endNodes.insert(0, result.nodes[i]);
        end -= value.length;
      } else {
        break;
      }
    }
    final _HighlightNode? midNode;
    if (startNodes.isEmpty) {
      midNode = _HighlightNode(text.substring(start, end), result.nodes[0].className);
    } else if (startNodes.length < result.nodes.length) {
      midNode = _HighlightNode(text.substring(start, end), result.nodes[startNodes.length].className);
    } else if (end > start){
      midNode = _HighlightNode(text.substring(start, end), result.nodes.last.className);
    } else {
      midNode = null;
    }
    return _buildSpanFromNodes([
      ...startNodes,
      if (midNode != null)
        midNode,
      ...endNodes
    ], style);
  }

  TextSpan _buildSpanFromNodes(List<_HighlightNode> nodes, TextStyle baseStyle) {
    return TextSpan(
      children: nodes.map((e) => TextSpan(
          text: e.value,
          style: _findStyle(e.className)
        )).toList(),
      style: baseStyle
    );
  }

  TextStyle? _findStyle(String? className) {
    if (className == null) {
      return null;
    }
    while(true) {
      final TextStyle? style = _theme?.theme[className];
      if (style != null) {
        return style;
      }
      final int pieceIndex = className!.indexOf('-');
      if (pieceIndex < 0) {
        break;
      }
      className = className.substring(pieceIndex + 1);
      if (className.isEmpty) {
        break;
      }
    }
    return null;
  }

  void _onCodesChanged() {
    if (_controller.preValue?.codeLines == _controller.codeLines) {
      return;
    }
    _processHighlight();
  }

  void _processHighlight() {
    _engine.run(_controller.codeLines, (result) {
      reEditorHighlightsDelivered++;
      value = result;
    });
  }

}

/// Axiom patch (FL-PERF-003): how many highlight results reached a live editor (tests: a
/// tab shown after quick switching is still highlighted).
@visibleForTesting
int reEditorHighlightsDelivered = 0;

class _CodeHighlightEngine {

  late final _IsolateTasker<_HighlightPayload, List<_HighlightResult>?> _tasker;

  Highlight? _highlight;
  CodeHighlightTheme? _theme;

  _CodeHighlightEngine(final CodeHighlightTheme? theme) {
    this.theme = theme;
    _tasker = _IsolateTasker<_HighlightPayload, List<_HighlightResult>?>('CodeHighlightEngine', _run);
  }

  set theme(CodeHighlightTheme? value) {
    if (_theme == value) {
      return;
    }
    _theme = value;
    final Map<String, CodeHighlightThemeMode>? modes = _theme?.languages;
    if (modes == null) {
      _highlight = null;
    } else {
      final Highlight highlight = Highlight();
      highlight.registerLanguages(modes.map((key, value) => MapEntry(key, value.mode)));
      for (final HLPlugin plugin in _theme!.plugins) {
        highlight.addPlugin(plugin);
      }
      _highlight = highlight;
    }
  }

  void dispose() {
    _tasker.close();
  }

  // Axiom patch (FL-PERF-003): the worker keeps each Highlight it was given, keyed by
  // the identity of its modes, and is sent the text as one string (shared between
  // isolates, not copied). Sending the Highlight and the CodeLines with every request
  // copied the grammar and the whole document on the UI thread for each edit.
  static final Expando<int> _modeIds = Expando<int>('mode');
  static int _nextModeId = 0;
  static final Set<String> _sentKeys = {};
  static final Map<String, Highlight> _workerHighlights = {};

  String _keyOf(Map<String, CodeHighlightThemeMode> modes) {
    final StringBuffer sb = StringBuffer();
    for (final MapEntry<String, CodeHighlightThemeMode> e in modes.entries) {
      final int id = _modeIds[e.value.mode] ??= _nextModeId++;
      sb.write('${e.key}=$id;');
    }
    for (final HLPlugin plugin in _theme!.plugins) {
      final int id = _modeIds[plugin] ??= _nextModeId++;
      sb.write('plugin=$id;');
    }
    return sb.toString();
  }

  void run(CodeLines codes, IsolateCallback<List<_HighlightResult>> callback) {
    final Highlight? highlight = _highlight;
    if (highlight == null) {
      callback(const []);
      return;
    }
    final Map<String, CodeHighlightThemeMode>? modes = _theme?.languages;
    if (modes == null) {
      callback(const []);
      return;
    }
    final int maxSize = modes.values.map((e) => e.maxSize).reduce(min);
    final int maxLineLength = modes.values.map((e) => e.maxLineLength).reduce(min);
    // Evalaute performance
    bool canHighlight = true;
    int total = 0;
    for (int i = 0; i < codes.length; i++) {
      final int length = codes[i].length;
      if (length > maxLineLength || total > maxSize) {
        canHighlight = false;
        break;
      }
      total += length;
    }
    final String key = _keyOf(modes);
    final String code = codes.asString(TextLineBreak.lf, false);
    final List<String> languages = modes.keys.toList();
    void send() {
      final bool known = _sentKeys.contains(key);
      _sentKeys.add(key);
      _tasker.run(_HighlightPayload(
        key: key,
        highlight: known ? null : highlight,
        code: code,
        canHighlight: canHighlight,
        languages: languages,
      ), (List<_HighlightResult>? result) {
        if (result == null) {
          // The worker did not have this Highlight: send it once more.
          _sentKeys.remove(key);
          send();
          return;
        }
        callback(result);
      });
    }
    send();
  }

  @pragma('vm:entry-point')
  static List<_HighlightResult>? _run(_HighlightPayload payload) {
    Highlight? highlight = payload.highlight;
    if (highlight != null) {
      if (_workerHighlights.length >= 64) {
        _workerHighlights.clear();
      }
      _workerHighlights[payload.key] = highlight;
    } else {
      highlight = _workerHighlights[payload.key];
      if (highlight == null) {
        return null;
      }
    }
    final String code = payload.code;
    final HighlightResult result;
    if (!payload.canHighlight) {
      result = highlight.justTextHighlightResult(code);
    } else if (payload.languages.length == 1) {
      result = highlight.highlight(code: code, language: payload.languages.first);
    } else {
      result = highlight.highlightAuto(code, payload.languages);
    }
    final _HighlightLineRenderer renderer = _HighlightLineRenderer();
    result.render(renderer);
    return renderer.lineResults;
  }

}

class _HighlightPayload {

  final String key;
  final Highlight? highlight;
  final String code;
  final bool canHighlight;
  final List<String> languages;

  const _HighlightPayload({
    required this.key,
    required this.highlight,
    required this.code,
    required this.canHighlight,
    required this.languages,
  });

}

class _HighlightResult {
  final List<_HighlightNode> nodes;

  _HighlightResult(this.nodes);

  String get source => nodes.map((e) => e.value).join();
}

class _HighlightNode {

  final String? className;
  final String value;

  const _HighlightNode(this.value, [this.className]);
}

class _HighlightLineRenderer implements HighlightRenderer {

  final List<_HighlightResult> lineResults;
  final List<String?> classNames;
  _HighlightLineRenderer(): lineResults = [
    _HighlightResult([])
  ], classNames = [];

  @override
  void addText(String text) {
    final String? className = classNames.isEmpty ? null : classNames.last;
    final List<String> lines = text.split(TextLineBreak.lf.value);
    lineResults.last.nodes.add(_HighlightNode(lines.first, className));
    if (lines.length > 1) {
      for (int i = 1; i < lines.length; i++) {
        lineResults.add(_HighlightResult([_HighlightNode(lines[i], className)]));
      }
    }
  }

  @override
  void openNode(DataNode node) {
    final String? className = classNames.isEmpty ? null : classNames.last;
    String? newClassName;
    if (className == null || node.scope == null) {
      newClassName = node.scope;
    } else {
      newClassName = '$className-${node.scope!}';
    }
    newClassName = newClassName?.split('.')[0];
    classNames.add(newClassName);
  }


  @override
  void closeNode(DataNode node) {
    if (classNames.isNotEmpty) {
      classNames.removeLast();
    }
  }

}

