import 'package:axiom/gallery/specimen.dart' as s;
import 'package:axiom/patterns/session_panel.dart';
import 'package:axiom/patterns/transcript_parts.dart';
import 'package:axiom/ui/controls.dart';
import 'package:axiom/ui/icons.dart';
import 'package:axiom/ui/marks.dart';
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

  testWidgets('marks and lamps of a session stand in columns of their own', (t) async {
    await mount(
      t,
      Align(
        alignment: Alignment.topLeft,
        child: SizedBox(
          width: 248,
          child: Column(
            children: [
              SessionPanel(session: s.session, selected: 'builder', onSelect: (_) {}),
              for (final o in s.otherSessions) SessionRow(o, onOpen: () {}),
              for (final k in s.session.tasks) TaskRow(k),
            ],
          ),
        ),
      ),
      size: const Size(800, 900),
      dpr: 2,
    );
    void oneLine(Iterable<double> xs, String what) => expect(
      xs.reduce((a, b) => a > b ? a : b) - xs.reduce((a, b) => a < b ? a : b),
      lessThan(.6),
      reason: '$what: $xs',
    );
    Iterable<double> middles(Finder f) =>
        t.renderObjectList<RenderBox>(f).map((m) => m.localToGlobal(m.size.center(Offset.zero)).dx);
    final row = find.byType(AgentRow);
    oneLine(middles(find.descendant(of: row, matching: find.byType(AxServiceMark))), 'marks');
    oneLine(middles(find.descendant(of: row, matching: find.byType(AxLamp))), 'lamps');
    oneLine(
      middles(find.descendant(of: find.byType(TaskRow), matching: find.byType(AxLamp))),
      'tasks',
    );
    oneLine([
      for (final x in ['2h', '1d', '3d']) t.getTopRight(find.text(x)).dx,
    ], 'ages');
    oneLine([
      for (final x in ['Builder', 'Tester', 'Reviewer']) t.getTopRight(find.text(x).last).dx,
    ], 'owners');
    // An agent's two lines begin where each other does.
    oneLine([
      for (final x in [
        find.textContaining('Lead  ', findRichText: true).first,
        find.textContaining('Reading review notes', findRichText: true).first,
      ])
        t.getTopLeft(x).dx,
    ], 'lines');
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
    const capHeight = .727; // Inter
    expect(glyph, closeTo(b - 13 * capHeight / 2, .75));
  });
}
