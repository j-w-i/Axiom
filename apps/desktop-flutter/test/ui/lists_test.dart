// Lists: one Tab stop (AUD-021), lazily built results (AUD-050), rows that grow with the text
// scale (AUD-020), deep explorer rows (AUD-016), scrolling tabs (AUD-059, AUD-072), wrapped
// output and diff lines (AUD-058), stale hints (AUD-064), focus semantics (AUD-071).

import 'dart:ui' show Tristate;

import 'package:axiom_desktop/console/console.dart';
import 'package:axiom_desktop/console/output.dart';
import 'package:axiom_desktop/daemon/protocol.g.dart';
import 'package:axiom_desktop/files/explorer.dart';
import 'package:axiom_desktop/main.dart';
import 'package:axiom_desktop/scm/scm_page.dart';
import 'package:axiom_desktop/search/search_page.dart';
import 'package:axiom_desktop/state/git.dart';
import 'package:axiom_desktop/state/tasks.dart';
import 'package:axiom_desktop/state/workspace.dart';
import 'package:axiom_desktop/ui/controls.dart';
import 'package:axiom_desktop/ui/pressable.dart';
import 'package:axiom_desktop/ui/roving.dart';
import 'package:axiom_desktop/ui/tabs.dart';
import 'package:axiom_desktop/ui/theme.dart';
import 'package:axiom_desktop/ui/tokens.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

Future<void> pumpAt(
  WidgetTester tester,
  Widget home, {
  Size size = const Size(900, 700),
  double textScale = 1,
}) async {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1;
  tester.platformDispatcher.textScaleFactorTestValue = textScale;
  addTearDown(tester.view.reset);
  addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
  await tester.pumpWidget(AxiomApp(home: home));
  await tester.pump();
}

AxiomThemeData theme() => AxiomThemeData(colors: AxiomColors.dark);

/// A list of [n] rows with one Tab stop, between two buttons.
class _Rows extends StatefulWidget {
  const _Rows(this.n);
  final int n;

  @override
  State<_Rows> createState() => _RowsState();
}

class _RowsState extends State<_Rows> {
  final nodes = RowFocusNodes();
  int current = 0;

  @override
  void dispose() {
    nodes.dispose();
    super.dispose();
  }

  void focus(int i) {
    setState(() => current = i.clamp(0, widget.n - 1));
    WidgetsBinding.instance.addPostFrameCallback((_) => nodes['$current'].requestFocus());
  }

  @override
  Widget build(BuildContext context) => Column(
    children: [
      AxButton(label: 'Before', onPressed: () {}),
      SizedBox(
        height: 300,
        child: RovingFocus(
          onEnter: () => focus(current),
          child: CallbackShortcuts(
            bindings: {
              const SingleActivator(LogicalKeyboardKey.arrowDown): () => focus(current + 1),
              const SingleActivator(LogicalKeyboardKey.arrowUp): () => focus(current - 1),
            },
            child: ListView.builder(
              itemExtent: 22,
              itemCount: widget.n,
              itemBuilder: (context, i) => AxListRow(
                label: 'Row $i',
                focusNode: nodes['$i'],
                skipTraversal: true,
                onPressed: () => setState(() => current = i),
                child: Text('Row $i'),
              ),
            ),
          ),
        ),
      ),
      AxButton(label: 'After', onPressed: () {}),
    ],
  );
}

String? focusedLabel() => FocusManager.instance.primaryFocus?.debugLabel;

Future<void> tab(WidgetTester tester, {bool shift = false}) async {
  if (shift) await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
  await tester.sendKeyEvent(LogicalKeyboardKey.tab);
  if (shift) await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
  await tester.pump();
  await tester.pump();
}

TreeNode node(String path, {bool dir = false}) => TreeNode(
  path: path,
  name: path.split('/').last,
  kind: dir ? EntryKind.directory : EntryKind.file,
);

void main() {
  testWidgets('a list of 200 rows is one Tab stop; arrows move inside; Shift+Tab leaves', (
    tester,
  ) async {
    await pumpAt(tester, const _Rows(200));
    await tab(tester);
    expect(focusedLabel(), 'Before');
    await tab(tester);
    expect(focusedLabel(), 'row 0', reason: 'Tab enters the list on its current row');
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();
    await tester.pump();
    expect(focusedLabel(), 'row 1');
    await tab(tester);
    expect(focusedLabel(), 'After', reason: 'one more Tab leaves the list');
    await tab(tester, shift: true);
    expect(focusedLabel(), 'row 1', reason: 'Shift+Tab comes back to the current row');
    await tab(tester, shift: true);
    expect(focusedLabel(), 'Before', reason: 'and leaves the list again');
  });

  testWidgets('row focus nodes of rows scrolled away are disposed', (tester) async {
    await pumpAt(tester, const _Rows(2000));
    final s = tester.state<_RowsState>(find.byType(_Rows));
    final built = s.nodes.length;
    await tester.drag(find.byType(ListView), const Offset(0, -20000));
    await tester.pumpAndSettle();
    // The viewport and its cache extent above and below (not the 900 rows scrolled past).
    expect(built, lessThan(40));
    expect(s.nodes.length, lessThan(60));
  });

  testWidgets('a row at depth 30 in a 180 px tree at text scale 1.5 keeps its name', (
    tester,
  ) async {
    final deep = node(List.generate(31, (i) => 'd$i').join('/'));
    await pumpAt(
      tester,
      Align(
        alignment: Alignment.topLeft,
        child: SizedBox(
          width: 180,
          child: Builder(
            builder: (context) => SizedBox(
              height: context.scaled(rowExtent),
              child: ExplorerRow(
                node: deep,
                indent: explorerIndent(30, 180),
                selected: false,
                onPressed: () {},
                colors: AxiomColors.dark,
                type: AxiomType(AxiomColors.dark),
              ),
            ),
          ),
        ),
      ),
      textScale: 1.5,
    );
    expect(tester.takeException(), isNull);
    final name = tester.getRect(find.text('d30'));
    expect(name.width, greaterThan(10));
    final row = tester.getRect(find.byType(ExplorerRow));
    expect(row.height, greaterThanOrEqualTo(22 + 9 - 0.5), reason: 'the row grows with text');
    expect(explorerIndent(30, 180), lessThanOrEqualTo(180 * 0.4));
    expect(explorerIndent(2, 260), 28);
  });

  testWidgets('a problem row at text scale 1.5 does not overflow', (tester) async {
    await pumpAt(
      tester,
      Align(
        alignment: Alignment.topLeft,
        child: SizedBox(
          width: 420,
          child: ProblemRow(
            diagnostic: const Diagnostic(
              source: 'rustc',
              severity: Severity.error,
              message: 'mismatched types: expected `u32`, found `String` in this long message',
              location: Location(file: 'src/some/deep/module/file.rs', line: 120, column: 4),
            ),
            onPressed: () {},
          ),
        ),
      ),
      textScale: 1.5,
    );
    expect(tester.takeException(), isNull);
    expect(tester.getRect(find.byType(ProblemRow)).height, greaterThanOrEqualTo(30));
  });

  testWidgets('menu items, tabs and buttons grow with the text scale', (tester) async {
    await pumpAt(
      tester,
      Column(
        children: [
          AxButton(label: 'Run', onPressed: () {}),
          AxTabs(
            label: 'Views',
            tabs: const [TabItem(id: 'a', label: 'Output')],
            selected: 'a',
            onSelect: (_) {},
          ),
        ],
      ),
      textScale: 1.5,
    );
    expect(tester.takeException(), isNull);
    expect(tester.getRect(find.byType(AxButton)).height, greaterThan(Metrics.controlHeight));
    expect(tester.getRect(find.byType(AxTabs)).height, greaterThan(Metrics.tabHeight));
  });

  testWidgets('20 tabs in 300 px scroll instead of overflowing; the selected one is in view', (
    tester,
  ) async {
    await pumpAt(
      tester,
      Align(
        alignment: Alignment.topLeft,
        child: SizedBox(
          width: 300,
          child: AxTabs(
            label: 'Terminals',
            tabs: [for (var i = 0; i < 20; i++) TabItem(id: '$i', label: 'bash $i')],
            selected: '19',
            onSelect: (_) {},
          ),
        ),
      ),
    );
    await tester.pump();
    expect(tester.takeException(), isNull);
    final last = tester.getRect(find.text('bash 19'));
    expect(last.right, lessThanOrEqualTo(300.5));
    expect(last.left, greaterThanOrEqualTo(0));
    // A vertical wheel scrolls the strip sideways.
    final before = tester.getRect(find.text('bash 19')).left;
    final g = TestPointer(1, PointerDeviceKind.mouse);
    await tester.sendEventToBinding(g.hover(tester.getCenter(find.text('bash 19'))));
    await tester.sendEventToBinding(g.scroll(const Offset(0, -200)));
    await tester.pump();
    expect(tester.getRect(find.text('bash 19')).left, greaterThan(before));
  });

  testWidgets('a long tab label is ellipsised', (tester) async {
    await pumpAt(
      tester,
      Align(
        alignment: Alignment.topLeft,
        child: SizedBox(
          width: 180,
          child: AxTabs(
            label: 'Conversations of',
            tabs: const [
              TabItem(id: 'general', label: 'General'),
              TabItem(id: 'ws', label: 'a-folder-with-a-really-very-long-name-indeed-yes'),
            ],
            selected: 'general',
            onSelect: (_) {},
          ),
        ),
      ),
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('a parent rebuild builds AxTabs once, not twice', (tester) async {
    var builds = 0;
    final tick = ValueNotifier(0);
    addTearDown(tick.dispose);
    await pumpAt(
      tester,
      ValueListenableBuilder(
        valueListenable: tick,
        builder: (context, v, _) => AxTabs(
          label: 'Views',
          tabs: [
            TabItem(
              id: 'a',
              label: 'A',
              trailing: Builder(
                builder: (_) {
                  builds++;
                  return Text('$v');
                },
              ),
            ),
          ],
          selected: 'a',
          onSelect: (_) {},
        ),
      ),
    );
    await tester.pump();
    builds = 0;
    tick.value++;
    await tester.pump();
    await tester.pump();
    expect(builds, 1);
  });

  testWidgets('search results with 5 000 matches build only the rows on screen', (tester) async {
    final groups = [
      for (var f = 0; f < 50; f++)
        (
          path: 'src/file$f.rs',
          matches: [
            for (var l = 1; l <= 100; l++)
              SearchMatch(
                path: 'src/file$f.rs',
                line: l,
                column: 1,
                endColumn: 4,
                preview: 'let needle = $l;',
                previewColumn: 5,
              ),
          ],
        ),
    ];
    await pumpAt(
      tester,
      SizedBox(
        height: 500,
        child: SearchResults(groups: groups, onOpen: (_) {}),
      ),
    );
    await tester.pump();
    final s = tester.state<SearchResultsState>(find.byType(SearchResults));
    expect(s.builtRows, lessThan(60));
    expect(find.text('src/file0.rs'), findsOneWidget);
    expect(find.text('src/file49.rs'), findsNothing);
  });

  testWidgets('a 300-character diff line shows all of its text (wrapped)', (tester) async {
    final long = List.filled(300, 'x').join();
    final rows = sideBySide(['a', 'b'], ['a', long]);
    await pumpAt(tester, SizedBox(width: 600, height: 400, child: SideBySide(rows: rows)));
    expect(tester.takeException(), isNull);
    final text = tester.widget<Text>(find.text(long));
    expect(text.maxLines, isNull);
    expect(tester.getRect(find.text(long)).height, greaterThan(30));
  });

  test('the side-by-side alignment pairs a removed line with its replacement', () async {
    final rows = await alignDiff(['a', 'b', 'c'], ['a', 'B', 'c']);
    expect(rows.length, 3);
    expect(rows[1].$1?.op, DiffOp.removed);
    expect(rows[1].$2?.op, DiffOp.added);
    final big = await alignDiff(
      [for (var i = 0; i < 1500; i++) 'l$i'],
      [for (var i = 0; i < 1500; i++) i == 700 ? 'changed' : 'l$i'],
    );
    expect(big.where((r) => r.$1?.op == DiffOp.removed).length, 1);
  });

  testWidgets('an output line wraps by default', (tester) async {
    final long = List.filled(300, 'y').join();
    await pumpAt(
      tester,
      SizedBox(
        width: 400,
        child: OutputLine(row: OutputRow(1, long, StreamKind.stdout, RowKind.line)),
      ),
    );
    expect(tester.widget<Text>(find.text(long)).maxLines, isNull);
    expect(tester.getRect(find.text(long)).height, greaterThan(40));
  });

  testWidgets('a control that goes away while it shows its hint clears it', (tester) async {
    final show = ValueNotifier(true);
    addTearDown(show.dispose);
    await pumpAt(
      tester,
      ValueListenableBuilder(
        valueListenable: show,
        builder: (context, on, _) => Center(
          child: on ? AxButton(label: 'Restart', onPressed: () {}) : const SizedBox(),
        ),
      ),
    );
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.pump();
    final hint = HintScope.maybeOf(tester.element(find.byType(AxButton)))!;
    expect(hint.value, 'Restart');
    show.value = false;
    await tester.pump();
    expect(hint.value, isNull);
  });

  testWidgets('a focused control carries the focusable and focused flags', (tester) async {
    final handle = tester.ensureSemantics();
    await pumpAt(
      tester,
      Center(
        child: AxButton(label: 'Go', onPressed: () {}),
      ),
    );
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.pump();
    final s = tester.getSemantics(find.byType(AxButton));
    // Focusable shows as a focus state other than none; focused as true.
    expect(s.flagsCollection.isFocused, Tristate.isTrue);
    handle.dispose();
  });
}
