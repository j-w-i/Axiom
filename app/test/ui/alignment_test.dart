import 'package:axiom/gallery/specimen.dart' as s;
import 'package:axiom/patterns/session_panel.dart';
import 'package:axiom/patterns/transcript_parts.dart';
import 'package:axiom/ui/controls.dart';
import 'package:axiom/ui/icons.dart';
import 'package:axiom/ui/overlay.dart';
import 'package:axiom/ui/status.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import '../shot.dart';

double baseline(WidgetTester t, Finder f) {
  final p = t.renderObject<RenderParagraph>(f);
  return p.localToGlobal(Offset.zero).dy +
      p.computeDistanceToActualBaseline(TextBaseline.alphabetic);
}

void main() {
  testWidgets('a line number and its code share a baseline', (t) async {
    await mount(
      t,
      const Center(
        child: SizedBox(
          width: 500,
          child: DiffBlock(path: 'a', lines: s.diff),
        ),
      ),
      dpr: 2,
    );
    final rows = t.renderObjectList<RenderParagraph>(find.byType(RichText)).toList();
    final number = rows.firstWhere((p) => p.text.toPlainText() == '46');
    final code = rows.firstWhere((p) => p.text.toPlainText().contains('provider.rotate'));
    double y(RenderParagraph p) =>
        p.localToGlobal(Offset.zero).dy +
        p.computeDistanceToActualBaseline(TextBaseline.alphabetic);
    expect(y(number), closeTo(y(code), .01));
  });

  testWidgets('every time and owner in a session ends on the same edge', (t) async {
    await mount(
      t,
      Align(
        alignment: Alignment.topLeft,
        child: SizedBox(
          width: 248,
          child: SessionPanel(session: s.session, selected: 'builder', onSelect: (_) {}),
        ),
      ),
      dpr: 2,
    );
    final right = {
      for (final x in ['6:08', '9:55', '3:17', '2:20', '4 of 7', 'Reviewer'])
        x: t.getTopRight(find.text(x).last).dx,
    };
    for (final e in right.entries) {
      expect(e.value, closeTo(right['6:08']!, .3), reason: '$right');
    }
    final marks = {
      for (final m in t.renderObjectList<RenderBox>(find.byType(AxLamp)))
        m.localToGlobal(m.size.center(Offset.zero)).dx,
    };
    expect(
      marks.reduce((a, b) => a > b ? a : b) - marks.reduce((a, b) => a < b ? a : b),
      lessThan(.6),
    );
  });

  testWidgets('menu details end on one edge, and their marks keep their shape', (t) async {
    await mount(
      t,
      Center(
        child: SizedBox(
          width: 260,
          child: AxMenu<String>(items: s.harnesses, value: 'codex', onChosen: (_) {}),
        ),
      ),
      dpr: 2,
    );
    final edges = [
      for (final d in ['native engine', 'not found']) t.getTopRight(find.text(d)).dx,
      for (final e in find.text('installed').evaluate()) t.getTopRight(find.byWidget(e.widget)).dx,
    ];
    for (final e in edges) {
      expect(e, closeTo(edges.first, .3));
    }
    for (final g in t.renderObjectList<RenderBox>(find.byType(AxGlyph))) {
      expect(g.size.width, g.size.height);
    }
  });

  testWidgets('a key\'s glyph and its word share a centre', (t) async {
    await mount(
      t,
      Center(
        child: AxButton(label: 'Answer', icon: AxIcon.shield, onPressed: () {}),
      ),
      dpr: 2,
    );
    final glyph = t.getCenter(find.byType(AxGlyph)).dy;
    // The middle of the capitals, not of the line box.
    final b = baseline(
      t,
      find.descendant(of: find.text('Answer'), matching: find.byType(RichText)),
    );
    const capHeight = .686; // Archivo
    expect(glyph, closeTo(b - 12 * capHeight / 2, .75));
  });
}
