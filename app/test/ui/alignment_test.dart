import 'package:axiom/gallery/specimen.dart' as s;
import 'package:axiom/patterns/transcript_parts.dart';
import 'package:axiom/ui/controls.dart';
import 'package:axiom/ui/icons.dart';
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
