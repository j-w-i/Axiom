// Diagnostics as wavy underlines on the highlighted spans of one line: the line's span tree is
// flattened to leaves (each with its resolved style) and split at the diagnostics' columns.

import 'package:flutter/painting.dart';

import '../../state/lsp.dart';

/// One underline on a line: [start, end) in UTF-16 units.
typedef Underline = ({int start, int end, Color color});

/// The underlines [diagnostics] put on line [index] (of [length] characters). A zero-width
/// range is widened to one character so it shows.
List<Underline> underlinesOn(
  int index,
  int length,
  List<LspDiagnostic> diagnostics,
  Color Function(int severity) color,
) {
  final out = <Underline>[];
  for (final d in diagnostics) {
    final r = d.range;
    if (index < r.start.line || index > r.end.line) continue;
    var s = index == r.start.line ? r.start.character : 0;
    var e = index == r.end.line ? r.end.character : length;
    s = s.clamp(0, length);
    e = e.clamp(0, length);
    if (e <= s) {
      if (length == 0) continue;
      if (s >= length) s = length - 1;
      e = s + 1;
    }
    out.add((start: s, end: e, color: color(d.severity)));
  }
  // Errors drawn last win where ranges overlap.
  out.sort((a, b) => a.start.compareTo(b.start));
  return out;
}

/// [span] with [underlines] applied.
TextSpan applyUnderlines(TextSpan span, List<Underline> underlines) => _apply(
  span,
  underlines,
  (style, u) => style.copyWith(
    decoration: TextDecoration.underline,
    decorationStyle: TextDecorationStyle.wavy,
    decorationColor: u.color,
    decorationThickness: 1.6,
  ),
);

/// [span] with the document highlights on its line (the symbol under the caret, elsewhere)
/// shown as a background.
TextSpan applyHighlights(TextSpan span, List<({int start, int end})> ranges, Color background) =>
    _apply(span, [
      for (final r in ranges) (start: r.start, end: r.end, color: background),
    ], (style, u) => style.copyWith(backgroundColor: u.color));

/// [span] with [styles] (semantic tokens) laid over it, clamped to the line's [length].
TextSpan applyStyles(
  TextSpan span,
  List<({int start, int end, TextStyle style})> styles,
  int length,
) {
  // Each range keeps its own style: keyed by colour, the italic or strikethrough of one token
  // went to every token of that colour on the line (audit finding 16).
  final styleOf = <Underline, TextStyle>{};
  final ranges = <Underline>[];
  for (final r in styles) {
    if (r.style.color == null || r.start >= length) continue;
    final u = (start: r.start.clamp(0, length), end: r.end.clamp(0, length), color: r.style.color!);
    ranges.add(u);
    styleOf[u] = r.style;
  }
  return _apply(span, ranges, (style, u) => style.merge(styleOf[u]));
}

/// [span] with the structure the grammar left plain coloured, as VS Code's grammars do: a name
/// followed by `(` (a call) in [function], a name after `.` (a member) in [variable]. Only text
/// in [plain] (the grammar's text colour) changes: keywords, strings and comments keep theirs.
/// Many highlight.js grammars (C#, Java, Kotlin, Swift, Go, Dart…) scope neither (2026-11-22).
TextSpan applyStructure(TextSpan span, Color plain, Color function, Color variable) {
  final leaves = <(int, int, bool)>[];
  final text = StringBuffer();
  void flatten(InlineSpan s, Color? inherited) {
    if (s is! TextSpan) return;
    final colour = s.style?.color ?? inherited;
    final t = s.text;
    if (t != null && t.isNotEmpty) {
      leaves.add((text.length, text.length + t.length, colour == null || colour == plain));
      text.write(t);
    }
    for (final c in s.children ?? const <InlineSpan>[]) {
      flatten(c, colour);
    }
  }

  flatten(span, null);
  final line = text.toString();
  bool isPlain(int start, int end) => leaves.every((l) => l.$3 || l.$2 <= start || l.$1 >= end);
  final ranges = <Underline>[];
  for (final m in _call.allMatches(line)) {
    if (isPlain(m.start, m.end)) ranges.add((start: m.start, end: m.end, color: function));
  }
  for (final m in _member.allMatches(line)) {
    if (isPlain(m.start, m.end) && !ranges.any((r) => r.start == m.start)) {
      ranges.add((start: m.start, end: m.end, color: variable));
    }
  }
  if (ranges.isEmpty) return span;
  return _apply(span, ranges, (style, u) => style.copyWith(color: u.color));
}

final _call = RegExp(r'\b[A-Za-z_][A-Za-z0-9_]*(?=\s*\()');
final _member = RegExp(r'(?<=\.)[A-Za-z_][A-Za-z0-9_]*\b');

/// The ranges of [highlights] (LSP ranges) on line [index].
List<({int start, int end})> highlightsOn(int index, int length, List<LspRange> highlights) => [
  for (final r in highlights)
    if (index >= r.start.line && index <= r.end.line)
      (
        start: (index == r.start.line ? r.start.character : 0).clamp(0, length),
        end: (index == r.end.line ? r.end.character : length).clamp(0, length),
      ),
];

TextSpan _apply(
  TextSpan span,
  List<Underline> underlines,
  TextStyle Function(TextStyle style, Underline u) mark,
) {
  if (underlines.isEmpty) return span;
  final leaves = <(String, TextStyle?)>[];
  void flatten(InlineSpan s, TextStyle? inherited) {
    if (s is! TextSpan) return;
    final style = inherited == null
        ? s.style
        : (s.style == null ? inherited : inherited.merge(s.style));
    if (s.text != null && s.text!.isNotEmpty) leaves.add((s.text!, style));
    for (final c in s.children ?? const <InlineSpan>[]) {
      flatten(c, style);
    }
  }

  flatten(span, null);
  final children = <TextSpan>[];
  var at = 0;
  for (final (text, style) in leaves) {
    final end = at + text.length;
    // Cut points inside this leaf.
    final cuts = <int>{at, end};
    for (final u in underlines) {
      if (u.start > at && u.start < end) cuts.add(u.start);
      if (u.end > at && u.end < end) cuts.add(u.end);
    }
    final sorted = cuts.toList()..sort();
    for (var i = 0; i + 1 < sorted.length; i++) {
      final a = sorted[i], b = sorted[i + 1];
      Underline? hit;
      for (final u in underlines) {
        if (u.start <= a && u.end >= b) hit = u;
      }
      final piece = text.substring(a - at, b - at);
      children.add(
        TextSpan(text: piece, style: hit == null ? style : mark(style ?? const TextStyle(), hit)),
      );
    }
    at = end;
  }
  return TextSpan(children: children);
}
