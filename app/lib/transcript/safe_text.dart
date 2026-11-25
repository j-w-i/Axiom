/// Text from agents, repositories and servers, made safe to show (SI-15).
library;

import 'dart:typed_data';

const _bad = 0xFFFD;

// Code points are named by number only: a source file that contained them would be unreadable.

/// Marks that only help a line break or a joining; leaving them out changes nothing a person reads.
bool _benign(int c) =>
    c == 0xAD ||
    c == 0x200B ||
    c == 0x200E ||
    c == 0x200F ||
    c == 0x61C ||
    c == 0x2060 ||
    c == 0xFEFF;

/// What must never pass unmarked: control codes, marks that reorder what is read, characters that
/// draw nothing, halves of characters.
bool _hidden(int c) =>
    (c < 0x20 && c != 9 && c != 10) ||
    (c >= 0x7F && c <= 0x9F) ||
    c == 0x34F ||
    c == 0x115F ||
    c == 0x1160 ||
    c == 0x17B4 ||
    c == 0x17B5 ||
    (c >= 0x180B && c <= 0x180F) ||
    (c >= 0x2028 && c <= 0x202E) ||
    (c >= 0x2061 && c <= 0x206F) ||
    c == 0x3164 ||
    (c >= 0xD800 && c <= 0xDFFF) ||
    (c >= 0xFE00 && c <= 0xFE0D) ||
    c == 0xFFA0 ||
    (c >= 0xFFF9 && c <= 0xFFFC) ||
    c & 0xFFFE == 0xFFFE ||
    (c >= 0x13430 && c <= 0x13455) ||
    (c >= 0x1BCA0 && c <= 0x1BCA3) ||
    c == 0x16FE4 ||
    c == 0x1D159 ||
    (c >= 0x1D173 && c <= 0x1D17A) ||
    c == 0x1DA9B ||
    (c >= 0xE0000 && c <= 0xE0FFF);

const _zwnj = 0x200C, _zwj = 0x200D;

/// A letter of a script that joins, or an emoji: what a joiner may follow.
bool _joins(int c) =>
    (c >= 0x600 && c <= 0x8FF) ||
    (c >= 0x900 && c <= 0xDFF) ||
    (c >= 0x1000 && c <= 0x109F) ||
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

/// For prose. What is hidden becomes U+FFFD; harmless invisible marks and carriage returns go.
String visible(String s) {
  if (_plain(s)) return s;
  final b = StringBuffer();
  var before = 0;
  for (final c in s.runes) {
    if (c == 13 || _benign(c)) continue;
    if (c == _zwj || c == _zwnj) {
      b.writeCharCode(_joins(before) ? c : _bad);
    } else {
      b.writeCharCode(_hidden(c) ? _bad : c);
    }
    before = c;
  }
  return b.toString();
}

final _sign = RegExp(r'^[\p{L}\p{N}\p{P}\p{S}\p{M}]$', unicode: true);

/// What is known of the first three planes: 0 nothing, 1 shown, 2 spelled out.
final _known = Uint8List(0x30000);

bool _shown(int c) {
  if (c < _known.length && _known[c] != 0) return _known[c] == 1;
  final ok =
      c == 0x20 ||
      c == 9 ||
      c == 10 ||
      (!_hidden(c) &&
          !_benign(c) &&
          c != 0x2800 &&
          c != 0xFE0E &&
          c != 0xFE0F &&
          c != _bad &&
          _sign.hasMatch(String.fromCharCode(c)));
  if (c < _known.length) _known[c] = ok ? 1 : 2;
  return ok;
}

/// For what a person decides on, and for code: whatever is not plainly a letter, a digit, a mark, a
/// sign or an ordinary space is spelled out, so two texts that differ never look the same for that
/// reason.
String literal(String s) {
  if (_plain(s)) return s;
  final b = StringBuffer();
  for (final c in s.runes) {
    final ok = _shown(c);
    ok ? b.writeCharCode(c) : b.write('\\u{${c.toRadixString(16).toUpperCase()}}');
  }
  return b.toString();
}

/// The first [max] code units of [s], never half a character.
String cutTo(String s, int max) {
  if (s.length <= max) return s;
  final half = max > 0 && s.codeUnitAt(max - 1) & 0xFC00 == 0xD800;
  return s.substring(0, half ? max - 1 : max);
}

/// The last [max] code units of [s], never half a character.
String cutFrom(String s, int max) {
  if (s.length <= max) return s;
  final at = s.length - max;
  return s.substring(s.codeUnitAt(at) & 0xFC00 == 0xDC00 ? at + 1 : at);
}

/// For names and titles: one line, bounded.
String oneLine(String s, [int max = 200]) {
  final t = visible(cutTo(s, max * 4)).replaceAll(RegExp(r'\s+'), ' ').trim();
  return t.length <= max ? t : '${cutTo(t, max - 1)}…';
}
