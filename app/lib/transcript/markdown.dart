enum BlockKind { paragraph, heading, item, quote, code, rule, table }

/// A piece of a document that no longer changes, or the open one at its end.
class Block {
  const Block(
    this.kind,
    this.text, {
    this.level = 0,
    this.marker = '',
    this.language = '',
    this.continues = false,
  });
  final BlockKind kind;
  final String text;

  /// Heading level, or list depth.
  final int level;
  final String marker, language;

  /// Continues the block before it (a long one is cut so that layout stays small).
  final bool continues;

  @override
  bool operator ==(Object other) =>
      other is Block &&
      other.kind == kind &&
      other.text == text &&
      other.level == level &&
      other.marker == marker &&
      other.language == language &&
      other.continues == continues;
  @override
  int get hashCode => Object.hash(kind, text, level, marker, language, continues);
  @override
  String toString() => '${kind.name}$level${continues ? '+' : ''}$marker($text)';
}

/// Markdown that arrives in pieces. Work is proportional to what arrives, never to what is already
/// there: closed blocks are final, and only the open one is rebuilt. How the text was cut into
/// pieces does not change the result (test/transcript/markdown_test.dart).
class MarkdownDoc {
  final List<Block> closed = [];
  String _line = '';
  List<String> _open = [];
  int _openLength = 0;
  BlockKind? _kind;
  int _level = 0;
  String _marker = '', _language = '', _fence = '';
  bool _continues = false, _blankBefore = false, _finished = false;

  static const maxText = 4000, maxCodeLines = 120;
  static final _headingRe = RegExp(r'^(#{1,6})[ \t]+(.*?)(?:[ \t]+#+)?[ \t]*$');
  static final _itemRe = RegExp(r'^([ \t]*)([-*+]|\d{1,9}[.)])[ \t]+(.*)$');
  static final _fenceRe = RegExp(r'^ {0,3}(`{3,}|~{3,})[ \t]*([^`]*)$');
  static final _ruleRe = RegExp(r'^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$');

  void reset() {
    closed.clear();
    _line = '';
    _open = [];
    _openLength = 0;
    _kind = null;
    _fence = '';
    _continues = _blankBefore = _finished = false;
  }

  void append(String text) {
    assert(!_finished);
    var from = 0;
    for (var i = text.indexOf('\n'); i >= 0; i = text.indexOf('\n', from)) {
      final end = i > from && text.codeUnitAt(i - 1) == 13 ? i - 1 : i;
      // A carriage return may have come with the piece before.
      var l = _line + text.substring(from, end);
      if (l.endsWith('\r')) l = l.substring(0, l.length - 1);
      _line = '';
      from = i + 1;
      _feed(l);
    }
    _line += text.substring(from);
  }

  /// No more text will come.
  void finish() {
    if (_finished) return;
    if (_line.isNotEmpty) {
      _feed(_line);
      _line = '';
    }
    _close();
    _finished = true;
  }

  /// The block still being written, with the unfinished line, or null.
  Block? get tail {
    if (_kind == null && _line.isEmpty) return null;
    final line = _clip(_line);
    if (_kind == BlockKind.code) {
      return _block([..._open, if (line.isNotEmpty) line]);
    }
    if (line.isEmpty) return _block(_open);
    // Show the unfinished line as what it looks like now, without deciding anything.
    final m = _headingRe.firstMatch(line);
    if (m != null) return Block(BlockKind.heading, m[2]!, level: m[1]!.length);
    final it = _itemRe.firstMatch(line);
    if (it != null) {
      return Block(BlockKind.item, it[3]!, level: _depth(it[1]!), marker: _mark(it[2]!));
    }
    if (_kind == null || _startsOther(line)) return Block(BlockKind.paragraph, line);
    return _block([..._open, _strip(line)]);
  }

  /// While the tail shows a block that is not the open one, the open one is shown before it.
  Block? get beforeTail {
    if (_kind == null || _kind == BlockKind.code || _line.isEmpty) return null;
    return _startsOther(_line) ? _block(_open) : null;
  }

  bool _startsOther(String l) =>
      _headingRe.hasMatch(l) ||
      _itemRe.hasMatch(l) ||
      (_kind == BlockKind.quote) != l.startsWith('>') ||
      (_kind == BlockKind.table) != l.trimLeft().startsWith('|') ||
      _blankBefore;

  static String _clip(String s) => s.length > maxText ? s.substring(s.length - maxText) : s;
  static int _depth(String indent) =>
      (indent.replaceAll('\t', '    ').length / 2).floor().clamp(0, 6);
  static String _mark(String m) => RegExp(r'^\d').hasMatch(m) ? m : '•';
  String _strip(String l) => switch (_kind) {
    BlockKind.quote => l.replaceFirst(RegExp(r'^ {0,3}> ?'), ''),
    BlockKind.item || BlockKind.paragraph => l.trimLeft(),
    _ => l,
  };

  Block _block(List<String> lines) => Block(
    _kind!,
    lines.join('\n'),
    level: _level,
    marker: _marker,
    language: _language,
    continues: _continues,
  );

  void _close() {
    if (_kind case final k?) {
      var text = _open.join('\n');
      if (k != BlockKind.code) text = text.trimRight();
      if (text.isNotEmpty || (k == BlockKind.code && !_continues)) {
        closed.add(
          Block(
            k,
            text,
            level: _level,
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
    _continues = false;
  }

  void _start(BlockKind k, {int level = 0, String marker = '', String language = ''}) {
    _close();
    _kind = k;
    _level = level;
    _marker = marker;
    _language = language;
  }

  void _add(String l) {
    if (_kind == BlockKind.code) {
      if (_open.length >= maxCodeLines) {
        final lang = _language;
        _start(BlockKind.code, language: lang);
        _continues = true;
      }
      _open.add(l);
      return;
    }
    _open.add(l);
    _openLength += l.length + 1;
    while (_openLength > maxText) {
      // Cut at a fixed place, so the result does not depend on how the text arrived.
      final all = _open.join('\n');
      var cut = all.lastIndexOf(RegExp(r'\s'), maxText);
      if (cut < maxText ~/ 2) cut = maxText;
      // Never between the halves of a surrogate pair.
      if (cut < all.length && (all.codeUnitAt(cut) & 0xFC00) == 0xDC00) cut--;
      final (k, level) = (_kind!, _level);
      closed.add(
        Block(k, all.substring(0, cut), level: level, marker: _marker, continues: _continues),
      );
      final rest = all.substring(cut).trimLeft();
      _marker = '';
      _continues = true;
      _open = [rest];
      _openLength = rest.length + 1;
    }
  }

  void _feed(String raw) {
    final l = raw;
    if (_kind == BlockKind.code) {
      final t = l.trim();
      if (t.length >= _fence.length &&
          t.startsWith(_fence) &&
          t.replaceAll(_fence[0], '').isEmpty) {
        _close();
        _fence = '';
      } else {
        _add(l);
      }
      _blankBefore = false;
      return;
    }
    if (l.trim().isEmpty) {
      // A list item may go on after a blank line; anything else is over.
      if (_kind != BlockKind.item) _close();
      _blankBefore = true;
      return;
    }
    final blank = _blankBefore;
    _blankBefore = false;
    final f = _fenceRe.firstMatch(l);
    if (f != null && !(f[1]![0] == '`' && f[2]!.contains('`'))) {
      _start(BlockKind.code, language: f[2]!.trim().split(RegExp(r'\s')).first.toLowerCase());
      _fence = f[1]!;
      return;
    }
    if (_ruleRe.hasMatch(l)) {
      _close();
      closed.add(const Block(BlockKind.rule, ''));
      return;
    }
    if (_headingRe.firstMatch(l) case final m?) {
      _close();
      closed.add(Block(BlockKind.heading, m[2]!, level: m[1]!.length));
      return;
    }
    if (_itemRe.firstMatch(l) case final m?) {
      _start(BlockKind.item, level: _depth(m[1]!), marker: _mark(m[2]!));
      _add(m[3]!);
      return;
    }
    if (RegExp(r'^ {0,3}>').hasMatch(l)) {
      if (_kind != BlockKind.quote) _start(BlockKind.quote);
      _add(_strip(l));
      return;
    }
    if (l.trimLeft().startsWith('|')) {
      if (_kind != BlockKind.table) _start(BlockKind.table);
      _add(l.trim());
      return;
    }
    switch (_kind) {
      case BlockKind.item when blank && !l.startsWith(RegExp(r'[ \t]')):
        _start(BlockKind.paragraph);
      case BlockKind.item when blank:
        _add('');
      case BlockKind.item || BlockKind.paragraph:
        break;
      case BlockKind.quote when !blank:
        break; // lazy continuation
      case _:
        _start(BlockKind.paragraph);
    }
    _add(_strip(l));
  }
}

/// Text from agents, made safe to show: no control codes, no marks that reorder what is read
/// (SI-15). Tabs and newlines stay.
String visible(String s) => s.contains(_hidden)
    ? s.replaceAllMapped(_hidden, (m) => m[0]!.codeUnitAt(0) == 13 ? '' : '\uFFFD')
    : s;
final _hidden = RegExp(
  r'[\u0000-\u0008\u000B-\u001F\u007F-\u009F\u061C\u200B-\u200F\u202A-\u202E\u2060-\u2069\uFEFF]',
);
