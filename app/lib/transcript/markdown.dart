/// Incremental Markdown. Pure Dart, so it can run in any isolate.
library;

enum BlockKind { paragraph, heading, item, quote, code, rule, table }

/// A piece of a document that no longer changes.
class Block {
  const Block(
    this.kind,
    this.text, {
    this.level = 0,
    this.depth = 0,
    this.marker = '',
    this.language = '',
    this.continues = false,
  });
  final BlockKind kind;
  final String text;

  /// Heading level.
  final int level;

  /// How many list items it is inside. An `item` is its own first paragraph, at its own depth.
  final int depth;

  /// `•`, `3.`, `todo` or `done`, for an item.
  final String marker;
  final String language;

  /// Continues the block before it (a long one is cut so that layout stays small).
  final bool continues;

  @override
  bool operator ==(Object other) =>
      other is Block &&
      other.kind == kind &&
      other.text == text &&
      other.level == level &&
      other.depth == depth &&
      other.marker == marker &&
      other.language == language &&
      other.continues == continues;
  @override
  int get hashCode => Object.hash(kind, text, level, depth, marker, language, continues);
  @override
  String toString() =>
      '${kind.name}${level > 0 ? level : ''}${'>' * depth}${continues ? '+' : ''}'
      '$marker${language.isEmpty ? '' : ':$language'}(${text.replaceAll('\n', r'\n')})';
}

/// Markdown that arrives in pieces. Work is proportional to what arrives, never to what is already
/// there: closed blocks are final, and only the open one is rebuilt. How the text was cut into
/// pieces does not change the result, and [tail] is what [finish] would give now
/// (test/transcript/markdown_test.dart holds both as properties).
class MarkdownDoc {
  MarkdownDoc();
  MarkdownDoc._fork(MarkdownDoc d)
    : _open = [...d._open],
      _openLength = d._openLength,
      _kind = d._kind,
      _marker = d._marker,
      _depth = d._depth,
      _level = d._level,
      _language = d._language,
      _fence = d._fence,
      _fenceIndent = d._fenceIndent,
      _indented = d._indented,
      _continues = d._continues,
      _blank = d._blank,
      _forced = d._forced,
      _forcedSpace = d._forcedSpace {
    _stack.addAll(d._stack);
  }

  final List<Block> closed = [];

  /// Raised when closed blocks were replaced, not only added to.
  int revision = 0;

  StringBuffer _line = StringBuffer();
  int _lineLength = 0;
  bool _cr = false;

  List<String> _open = [];
  int _openLength = 0;
  BlockKind? _kind;
  String _marker = '', _language = '', _fence = '';
  int _depth = 0, _level = 0, _fenceIndent = 0;
  bool _indented = false, _continues = false, _blank = false, _forced = false, _finished = false;
  bool _forcedSpace = false;

  /// The list items the next line may belong to: the column from which text is inside one.
  final List<int> _stack = [];

  static const maxText = 4000, maxCodeLines = 120, maxDepth = 12;

  void reset() {
    closed.clear();
    revision++;
    _line = StringBuffer();
    _lineLength = 0;
    _cr = false;
    _open = [];
    _openLength = 0;
    _kind = null;
    _fence = '';
    _stack.clear();
    _indented = _continues = _blank = _forced = _finished = false;
  }

  void append(String text) {
    if (_finished) throw StateError('the document is finished');
    var from = 0;
    while (from < text.length) {
      final i = text.indexOf('\n', from);
      final end = i < 0 ? text.length : i;
      if (end > from) _part(text.substring(from, end));
      if (i < 0) return;
      // A carriage return before the newline may have come with an earlier piece.
      _cr = false;
      _take();
      from = i + 1;
    }
  }

  void _part(String s) {
    // A carriage return counts only if something other than a newline follows it.
    if (_cr) {
      _write('\r');
      _cr = false;
    }
    if (s.endsWith('\r')) {
      _cr = true;
      s = s.substring(0, s.length - 1);
    }
    _write(s);
    // A line without end: cut at a fixed place, so the result does not depend on the pieces. Code
    // is never cut inside a line.
    while (_lineLength > maxText && _fence.isEmpty) {
      final all = _line.toString();
      final cut = _cutAt(all);
      _line = StringBuffer(all.substring(cut));
      _lineLength = all.length - cut;
      _feed(all.substring(0, cut));
      _forced = true;
      _forcedSpace = all.codeUnitAt(cut - 1) == 32;
    }
  }

  void _write(String s) {
    _line.write(s);
    _lineLength += s.length;
  }

  void _take() {
    final l = _line.toString();
    _line = StringBuffer();
    _lineLength = 0;
    _feed(l);
    _forced = false;
  }

  static int _cutAt(String s) {
    var cut = maxText;
    while (cut > maxText ~/ 2 && s.codeUnitAt(cut - 1) != 32) {
      cut--;
    }
    if (cut <= maxText ~/ 2) cut = maxText;
    // Never between the halves of a surrogate pair.
    if ((s.codeUnitAt(cut) & 0xFC00) == 0xDC00) cut--;
    return cut;
  }

  /// No more text will come.
  void finish() {
    if (_finished) return;
    if (_cr) {
      _write('\r');
      _cr = false;
    }
    if (_lineLength > 0) _take();
    _close();
    _finished = true;
  }

  /// What is being written now: what [finish] would add to [closed], except that a line whose
  /// meaning is still undecided (`-`, `##`, `1.`, ```` `` ````) is not shown until it is.
  List<Block> get tail {
    if (_finished) return const [];
    final d = MarkdownDoc._fork(this);
    var l = _line.toString();
    if (_fence.isNotEmpty) {
      // A closing fence being written is not code.
      if (l.trim().isNotEmpty && l.trim().replaceAll(_fence[0], '').isEmpty) l = '';
      if (l.length > maxText) l = l.substring(0, maxText);
    } else if (!_forced && undecided(l)) {
      l = '';
    }
    if (l.isNotEmpty) d._feed(l);
    d._close();
    return d.closed;
  }

  static bool undecided(String l) {
    final t = l.trim();
    if (t.isEmpty) return true;
    for (var i = 0; i < t.length; i++) {
      final c = t.codeUnitAt(i);
      final digit = c >= 48 && c <= 57;
      if (!digit && !'-=*_#`~+|.:)>\\ '.contains(t[i])) return false;
    }
    return true;
  }

  void _close() {
    if (_kind case final k?) {
      var text = _open.join('\n');
      if (k != BlockKind.code || _indented) text = text.trimRight();
      if (text.isNotEmpty ||
          (k == BlockKind.code && !_continues) ||
          (k == BlockKind.item && !_continues)) {
        closed.add(
          Block(
            k,
            text,
            level: _level,
            depth: _depth,
            marker: _marker,
            language: _language,
            continues: _continues,
          ),
        );
      }
    }
    _kind = null;
    _open = [];
    _openLength = 0;
    _level = 0;
    _marker = _language = '';
    _indented = _continues = false;
  }

  void _start(BlockKind k, {String marker = '', String language = '', bool indented = false}) {
    _close();
    _kind = k;
    _depth = _stack.length;
    _marker = marker;
    _language = language;
    _indented = indented;
  }

  void _one(BlockKind k, String text, {int level = 0}) {
    _close();
    closed.add(
      Block(
        k,
        text.length > maxText ? text.substring(0, maxText) : text,
        level: level,
        depth: _stack.length,
      ),
    );
  }

  void _add(String l) {
    if (_kind == BlockKind.code) {
      if (_open.length >= maxCodeLines) {
        final (lang, ind, depth) = (_language, _indented, _depth);
        _start(BlockKind.code, language: lang, indented: ind);
        _depth = depth;
        _continues = true;
      }
      _open.add(l);
      return;
    }
    _open.add(l);
    _openLength += l.length + 1;
    if (_kind == BlockKind.table) {
      // A table is cut between rows; its header goes with each part.
      if (_openLength > maxText && _open.length > 3) {
        final head = _open.take(2).toList();
        final last = _open.removeLast();
        _close();
        _start(BlockKind.table);
        _continues = true;
        _open = [...head, last];
        _openLength = _open.fold(0, (n, x) => n + x.length + 1);
      }
      return;
    }
    while (_openLength > maxText + 1) {
      final all = _open.join('\n');
      if (all.length <= maxText) break;
      final cut = _cutAt(all);
      closed.add(
        Block(
          _kind!,
          all.substring(0, cut).trimRight(),
          depth: _depth,
          marker: _marker,
          continues: _continues,
        ),
      );
      final rest = all.substring(cut).trimLeft();
      _marker = '';
      _continues = true;
      _open = [rest];
      _openLength = rest.length + 1;
    }
  }

  /// Leading columns (a tab reaches the next multiple of four) and where the text begins.
  static (int, int) _indent(String l) {
    var col = 0, i = 0;
    for (; i < l.length; i++) {
      final c = l.codeUnitAt(i);
      if (c == 32) {
        col++;
      } else if (c == 9) {
        col += 4 - col % 4;
      } else {
        break;
      }
    }
    return (col, i);
  }

  static String _strip(String l, int columns) {
    var col = 0, i = 0;
    while (i < l.length && col < columns) {
      final c = l.codeUnitAt(i);
      if (c == 32) {
        col++;
      } else if (c == 9) {
        col += 4 - col % 4;
      } else {
        break;
      }
      i++;
    }
    return ' ' * (col > columns ? col - columns : 0) + l.substring(i);
  }

  static int _run(String s, int at) {
    var n = at;
    while (n < s.length && s.codeUnitAt(n) == s.codeUnitAt(at)) {
      n++;
    }
    return n - at;
  }

  /// A fence's marker, or null.
  static (String, String)? _fenceOf(String t) {
    if (t.isEmpty || (t[0] != '`' && t[0] != '~')) return null;
    final n = _run(t, 0);
    if (n < 3) return null;
    final info = t.substring(n).trim();
    if (t[0] == '`' && info.contains('`')) return null;
    final lang = info.split(RegExp(r'\s')).first.toLowerCase();
    return (t.substring(0, n), lang.length > 24 ? lang.substring(0, 24) : lang);
  }

  static (int, String)? _headingOf(String t) {
    if (!t.startsWith('#')) return null;
    final n = _run(t, 0);
    if (n > 6 || (n < t.length && t[n] != ' ' && t[n] != '\t')) return null;
    var x = t.substring(n).trim();
    var e = x.length;
    while (e > 0 && x[e - 1] == '#') {
      e--;
    }
    if (e == 0 || (e < x.length && (x[e - 1] == ' ' || x[e - 1] == '\t'))) {
      x = x.substring(0, e).trimRight();
    }
    return (n, x);
  }

  static bool _ruleOf(String t) {
    if (t.isEmpty || !'-*_'.contains(t[0])) return false;
    var n = 0;
    for (var i = 0; i < t.length; i++) {
      if (t[i] == t[0]) {
        n++;
      } else if (t[i] != ' ' && t[i] != '\t') {
        return false;
      }
    }
    return n >= 3;
  }

  /// A list marker: its text, its width, and whether it may interrupt a paragraph.
  static (String, int, bool)? _itemOf(String t) {
    if (t.isEmpty) return null;
    if ('-*+'.contains(t[0])) {
      if (t.length == 1) return ('•', 1, false);
      return t[1] == ' ' || t[1] == '\t' ? ('•', 1, true) : null;
    }
    var n = 0;
    while (n < t.length && n < 9 && t.codeUnitAt(n) >= 48 && t.codeUnitAt(n) <= 57) {
      n++;
    }
    if (n == 0 || n >= t.length || (t[n] != '.' && t[n] != ')')) return null;
    final end = n + 1;
    if (end < t.length && t[end] != ' ' && t[end] != '\t') return null;
    final m = t.substring(0, end);
    return (m, end, end < t.length && t.substring(0, n) == '1');
  }

  static bool _delimiter(String t) {
    if (!t.contains('-')) return false;
    var dash = false, pipe = false;
    for (var i = 0; i < t.length; i++) {
      switch (t[i]) {
        case '-':
          dash = true;
        case '|':
          pipe = true;
        case ':' || ' ' || '\t':
          break;
        case _:
          return false;
      }
    }
    return dash && pipe;
  }

  bool get _text => _kind == BlockKind.paragraph || _kind == BlockKind.item;

  void _feed(String l) {
    if (_fence.isNotEmpty) return _code(l);
    final (col, at) = _indent(l);
    if (at == l.length) {
      _blank = true;
      if (_indented) {
        _add('');
      } else if (_kind != null) {
        _close();
      }
      return;
    }
    if (_forced && _open.isNotEmpty && (_text || _kind == BlockKind.quote)) {
      // The rest of a line that was cut: never a new block.
      final last = _open.removeLast();
      _openLength -= last.length + 1;
      _add('$last${_forcedSpace ? ' ' : ''}$l');
      return;
    }
    final blank = _blank;
    _blank = false;
    if (_indented) {
      if (col >= 4) return _add(_strip(l, 4));
      _close();
    }
    final t = l.substring(at).trimRight();
    // Two spaces at the end break the line; they must outlive the trimming.
    final text = l.endsWith('  ') ? '$t  ' : t;

    // Which items is this line inside?
    var keep = _stack.length;
    while (keep > 0 && col < _stack[keep - 1]) {
      keep--;
    }
    final item = _itemOf(t);
    final inside = keep > 0 ? _stack[keep - 1] : 0;
    final shift = col - inside;
    // In a list, deeper items are accepted however far they are indented.
    final canItem = item != null && (shift <= 3 || _stack.isNotEmpty);
    final fence = shift <= 3 || keep > 0 ? _fenceOf(t) : null;
    final heading = shift <= 3 ? _headingOf(t) : null;
    final rule = shift <= 3 && _ruleOf(t);
    final quote = shift <= 3 && t.startsWith('>');

    // A setext underline turns the paragraph above it into a heading.
    if (_text && !blank && keep == _stack.length && shift <= 3 && _kind == BlockKind.paragraph) {
      final u = t[0];
      if ((u == '=' || u == '-') && _run(t, 0) == t.length && (u == '=' || t.length > 1)) {
        final text = _open.join(' ').trim();
        _kind = null;
        _open = [];
        _openLength = 0;
        return _one(BlockKind.heading, text, level: u == '=' ? 1 : 2);
      }
    }
    // A delimiter row turns the line above it into a table's head.
    if (_text &&
        !blank &&
        keep == _stack.length &&
        _open.isNotEmpty &&
        _open.last.contains('|') &&
        _delimiter(t)) {
      final head = _open.removeLast();
      if (_open.isEmpty && _kind == BlockKind.paragraph) _kind = null;
      _start(BlockKind.table);
      _add(head.trim());
      return _add(t);
    }
    final row = _kind == BlockKind.table && !blank && t.contains('|');

    final starts =
        fence != null ||
        heading != null ||
        rule ||
        quote ||
        (canItem && (item.$3 || !_text || blank || keep < _stack.length));
    if (row && !starts) return _add(t);
    if (!starts) {
      // A line that only continues a paragraph, however it is indented.
      if (_text && !blank) return _add(text);
      if (_kind == BlockKind.quote && !blank) return _add(text);
    }
    if (keep < _stack.length) {
      _close();
      _stack.length = keep;
    }
    if (fence != null) {
      _start(BlockKind.code, language: fence.$2);
      _fence = fence.$1;
      _fenceIndent = col;
      return;
    }
    if (rule) return _one(BlockKind.rule, '');
    if (heading != null) return _one(BlockKind.heading, heading.$2, level: heading.$1);
    if (quote) {
      final x = text.substring(1);
      if (_kind != BlockKind.quote) _start(BlockKind.quote);
      return _add(x.startsWith(' ') ? x.substring(1) : x);
    }
    if (canItem && starts) {
      final (mark, width, _) = item;
      final rest = t.substring(width);
      final (gap, skip) = _indent(rest);
      var body = rest.substring(skip);
      if (_stack.length >= maxDepth) _stack.removeLast();
      // Text is inside an item from two columns after its marker on, whatever the marker's width:
      // stricter than needed for `- `, kinder than CommonMark to what is written under `1. `.
      _stack.add(col + 2);
      var marker = mark;
      if (body.length >= 3 && body[0] == '[' && body[2] == ']' && ' xX'.contains(body[1])) {
        marker = body[1] == ' ' ? 'todo' : 'done';
        body = body.substring(3).trimLeft();
      }
      _start(BlockKind.item, marker: marker);
      final f = _fenceOf(body);
      if (f != null) {
        _close();
        _start(BlockKind.code, language: f.$2);
        _fence = f.$1;
        _fenceIndent = col + width + gap;
        return;
      }
      if (body.isNotEmpty) _add(text.endsWith('  ') ? '$body  ' : body);
      return;
    }
    if (shift >= 4 && keep == 0) {
      _start(BlockKind.code, indented: true);
      return _add(_strip(l, 4));
    }
    _start(BlockKind.paragraph);
    _add(text);
  }

  void _code(String l) {
    final (col, at) = _indent(l);
    final t = l.substring(at).trimRight();
    if (col <= _fenceIndent + 3 &&
        t.length >= _fence.length &&
        t[0] == _fence[0] &&
        _run(t, 0) == t.length) {
      _fence = '';
      return _close();
    }
    _blank = false;
    _add(_strip(l, _fenceIndent));
  }
}

/// A block's lines as they read: a soft break is a space, two spaces or a backslash break.
String flow(String text) {
  if (!text.contains('\n')) return text;
  final b = StringBuffer();
  final lines = text.split('\n');
  for (var i = 0; i < lines.length; i++) {
    var l = lines[i];
    if (i + 1 == lines.length) {
      b.write(l);
    } else if (l.endsWith('  ')) {
      b.writeln(l.trimRight());
    } else if (l.endsWith(r'\')) {
      l = l.substring(0, l.length - 1);
      b.writeln(l);
    } else {
      b
        ..write(l.trimRight())
        ..write(' ');
    }
  }
  return b.toString();
}
