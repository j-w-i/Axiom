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
TextSpan applyUnderlines(TextSpan span, List<Underline> underlines) {
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
        TextSpan(
          text: piece,
          style: hit == null
              ? style
              : (style ?? const TextStyle()).copyWith(
                  decoration: TextDecoration.underline,
                  decorationStyle: TextDecorationStyle.wavy,
                  decorationColor: hit.color,
                  decorationThickness: 1.6,
                ),
        ),
      );
    }
    at = end;
  }
  return TextSpan(children: children);
}
