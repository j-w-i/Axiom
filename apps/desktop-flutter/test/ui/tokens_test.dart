// Contrast and colour discipline of the design tokens (FL-DS-001).

import 'dart:io';
import 'dart:math' as math;

import 'package:axiom_desktop/ui/tokens.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

double _lin(double c) => c <= 0.03928 ? c / 12.92 : math.pow((c + 0.055) / 1.055, 2.4).toDouble();

double luminance(Color c) => 0.2126 * _lin(c.r) + 0.7152 * _lin(c.g) + 0.0722 * _lin(c.b);

/// WCAG contrast of [fg] (alpha-blended over [bg]) on [bg].
double contrast(Color fg, Color bg) {
  final f = luminance(Color.alphaBlend(fg, bg));
  final b = luminance(bg);
  final hi = math.max(f, b), lo = math.min(f, b);
  return (hi + 0.05) / (lo + 0.05);
}

void main() {
  for (final (name, c) in [('dark', AxiomColors.dark), ('light', AxiomColors.light)]) {
    group('$name theme', () {
      final surfaces = {
        'recessed': c.recessed,
        'ground': c.ground,
        'bar': c.bar,
        'raised': c.raised,
        'sheet': c.sheet,
        'raised+hover': Color.alphaBlend(c.hover, c.raised),
      };
      final texts = {
        'ink': c.ink,
        'ink2': c.ink2,
        'ink3': c.ink3,
        'accentText': c.accentText,
        'okText': c.okText,
        'warnText': c.warnText,
        'dangerText': c.dangerText,
      };
      test('every text colour on every surface ≥ 4.5:1', () {
        final failures = <String>[];
        for (final s in surfaces.entries) {
          for (final x in texts.entries) {
            final r = contrast(x.value, s.value);
            if (r < 4.5) failures.add('${x.key} on ${s.key}: ${r.toStringAsFixed(2)}');
          }
        }
        expect(failures, isEmpty);
      });
      test('code colours on the editor ground ≥ 4.5:1 (line numbers ≥ 3:1)', () {
        final y = c.syntax;
        final failures = <String>[];
        for (final (n, col) in [
          ('text', y.text),
          ('comment', y.comment),
          ('keyword', y.keyword),
          ('type', y.type),
          ('string', y.string),
          ('number', y.number),
          ('punct', y.punct),
          ('macro', y.macro),
          ('function', y.function),
          ('variable', y.variable),
        ]) {
          final r = contrast(col, Color.alphaBlend(y.currentLine, c.ground));
          if (r < 4.5) failures.add('$n: ${r.toStringAsFixed(2)}');
        }
        if (contrast(y.lineNumber, c.ground) < 3) failures.add('line numbers');
        expect(failures, isEmpty);
      });
      test('text on the derived tints (diff rows, approval cards, selected rows) ≥ 4.5:1', () {
        final failures = <String>[];
        // Tints are translucent: checked over each surface they are drawn on.
        final tinted = {
          'diffAdd/ground': Color.alphaBlend(c.diffAdd, c.ground),
          'diffDel/ground': Color.alphaBlend(c.diffDel, c.ground),
          'diffAdd/recessed': Color.alphaBlend(c.diffAdd, c.recessed),
          'diffDel/recessed': Color.alphaBlend(c.diffDel, c.recessed),
          'accentSoft/ground': Color.alphaBlend(c.accentSoft, c.ground),
          'accentSoft/panel': Color.alphaBlend(c.accentSoft, c.panel),
          'selected/panel': Color.alphaBlend(c.selected, c.panel),
          'dangerSoft/ground': Color.alphaBlend(c.dangerSoft, c.ground),
        };
        final texts = {
          'ink': c.ink,
          'ink2': c.ink2,
          'code': c.syntax.text,
          'okText': c.okText,
          'dangerText': c.dangerText,
          'accentText': c.accentText,
        };
        for (final s in tinted.entries) {
          for (final x in texts.entries) {
            final r = contrast(x.value, s.value);
            if (r < 4.5) failures.add('${x.key} on ${s.key}: ${r.toStringAsFixed(2)}');
          }
          // Line numbers in the diff are faint by design: 3:1.
          if (contrast(c.ink3, s.value) < 3) failures.add('ink3 on ${s.key}');
        }
        expect(failures, isEmpty);
      });
      test('the approval border is a visible edge (≥ 1.5:1 on the panel and ground)', () {
        for (final s in [c.panel, c.ground]) {
          final edge = Color.alphaBlend(c.approvalBorder, s);
          expect(contrast(edge, s), greaterThanOrEqualTo(1.5));
        }
      });
      test('text on the accent and on the ink circle ≥ 4.5:1; the focus ring ≥ 3:1', () {
        expect(contrast(c.onAccent, c.accent), greaterThanOrEqualTo(4.5));
        expect(contrast(c.onInk, c.ink), greaterThanOrEqualTo(4.5));
        for (final s in [c.recessed, c.ground, c.bar, c.raised, c.sheet]) {
          expect(contrast(c.focus, s), greaterThanOrEqualTo(3), reason: 'focus ring on $s');
        }
      });
    });
  }

  test('no literal colours outside tokens.dart (and the generated protocol types)', () {
    final offenders = <String>[];
    for (final f in Directory('lib').listSync(recursive: true).whereType<File>()) {
      if (!f.path.endsWith('.dart') ||
          f.path.endsWith('tokens.dart') ||
          f.path.endsWith('.g.dart')) {
        continue;
      }
      final lines = f.readAsLinesSync();
      for (var i = 0; i < lines.length; i++) {
        if (RegExp(r'\bColor\(0x|Color\.fromARGB|Color\.fromRGBO').hasMatch(lines[i])) {
          offenders.add('${f.path}:${i + 1}');
        }
      }
    }
    expect(offenders, isEmpty);
  });
}
