/// Text from agents, repositories and servers, made safe to show (SI-15).
library;

// What must never reach a person's eye unmarked: control codes, marks that reorder text, characters
// that take no space or look like space. Zero-width joiners are judged in context.
bool _hidden(int c) =>
    (c < 0x20 && c != 9 && c != 10) ||
    (c >= 0x7F && c <= 0x9F) ||
    c == 0xAD ||
    c == 0x34F ||
    c == 0x61C ||
    c == 0x115F ||
    c == 0x1160 ||
    c == 0x17B4 ||
    c == 0x17B5 ||
    c == 0x180E ||
    c == 0x200B ||
    c == 0x200E ||
    c == 0x200F ||
    (c >= 0x2028 && c <= 0x202E) ||
    (c >= 0x2060 && c <= 0x206F) ||
    c == 0x3164 ||
    c == 0xFEFF ||
    c == 0xFFA0 ||
    (c >= 0xFFF9 && c <= 0xFFFB) ||
    (c >= 0xE0000 && c <= 0xE0FFF);

const _zwnj = 0x200C, _zwj = 0x200D;

/// A letter of a script that joins, or an emoji: where a joiner belongs.
bool _joins(int c) =>
    (c >= 0x600 && c <= 0x8FF) ||
    (c >= 0x900 && c <= 0xDFF) ||
    (c >= 0x1800 && c <= 0x18AF) ||
    (c >= 0xFB50 && c <= 0xFDFF) ||
    (c >= 0xFE70 && c <= 0xFEFC) ||
    c == 0xFE0F ||
    (c >= 0x2190 && c <= 0x2BFF) ||
    (c >= 0x1F000 && c <= 0x1FAFF);

bool _plain(String s) {
  for (var i = 0; i < s.length; i++) {
    final c = s.codeUnitAt(i);
    if (c >= 0x7F || (c < 0x20 && c != 9 && c != 10)) return false;
  }
  return true;
}

/// For prose. What is hidden becomes U+FFFD; a carriage return goes.
String visible(String s) {
  if (_plain(s)) return s;
  final r = s.runes.toList();
  final b = StringBuffer();
  for (var i = 0; i < r.length; i++) {
    final c = r[i];
    if (c == 13) continue;
    if (c == _zwj || c == _zwnj) {
      final ok = i > 0 && i + 1 < r.length && _joins(r[i - 1]) && _joins(r[i + 1]);
      b.writeCharCode(ok ? c : 0xFFFD);
    } else {
      b.writeCharCode(_hidden(c) ? 0xFFFD : c);
    }
  }
  return b.toString();
}

/// For what a person decides on, and for code: everything that cannot be seen is spelled out, so two
/// texts that differ never look the same for that reason.
String literal(String s) {
  if (_plain(s)) return s;
  final b = StringBuffer();
  for (final c in s.runes) {
    final odd =
        _hidden(c) ||
        c == _zwj ||
        c == _zwnj ||
        c == 0xA0 ||
        (c >= 0x2000 && c <= 0x200A) ||
        c == 0x202F ||
        c == 0x205F ||
        c == 0x3000 ||
        c == 0x2800 ||
        (c >= 0xFE00 && c <= 0xFE0E) ||
        (c >= 0xE000 && c <= 0xF8FF);
    odd ? b.write('\\u{${c.toRadixString(16).toUpperCase()}}') : b.writeCharCode(c);
  }
  return b.toString();
}

/// For names and titles: one line, bounded.
String oneLine(String s, [int max = 200]) {
  final t = visible(s.length > max * 4 ? s.substring(0, max * 4) : s)
      .replaceAll(RegExp(r'\s+'), ' ')
      .trim();
  if (t.length <= max) return t;
  var cut = max - 1;
  if ((t.codeUnitAt(cut) & 0xFC00) == 0xDC00) cut--;
  return '${t.substring(0, cut)}…';
}
