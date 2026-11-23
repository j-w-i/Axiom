// The overlay positioner (AUD-013, AUD-051), menus and tooltips inside the window, dialogs that
// follow the theme (AUD-046), the question's default answer (AUD-018), toasts (AUD-081, AUD-057)
// and the text fields' context menu (AUD-073).

import 'package:axiom_desktop/main.dart';
import 'package:axiom_desktop/ui/controls.dart';
import 'package:axiom_desktop/ui/icons.dart';
import 'package:axiom_desktop/ui/overlays.dart';
import 'package:axiom_desktop/ui/surfaces.dart';
import 'package:axiom_desktop/ui/text_input.dart';
import 'package:axiom_desktop/ui/theme.dart';
import 'package:axiom_desktop/ui/tokens.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

Future<void> pumpAt(WidgetTester tester, Widget home, Size size) async {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(AxiomApp(home: home));
  await tester.pump();
}

/// A widget at [alignment] of the window.
Widget at(Alignment alignment, Widget child) => Align(alignment: alignment, child: child);

List<MenuEntry> entries(int n) => [for (var i = 0; i < n; i++) MenuEntry.item('Item $i', () {})];

Widget menu(int n) => AxMenu(
  label: 'Menu',
  entries: () => entries(n),
  trigger: (context, s, open) =>
      const SizedBox(width: 60, height: 24, child: Center(child: Text('Open'))),
);

Rect sheetRect(WidgetTester tester) => tester.getRect(find.byType(Sheet).last);

void expectInside(Rect r, Size window, {double margin = 0}) {
  expect(r.left, greaterThanOrEqualTo(margin - 0.01), reason: '$r');
  expect(r.top, greaterThanOrEqualTo(margin - 0.01), reason: '$r');
  expect(r.right, lessThanOrEqualTo(window.width - margin + 0.01), reason: '$r');
  expect(r.bottom, lessThanOrEqualTo(window.height - margin + 0.01), reason: '$r');
}

void main() {
  group('OverlayPlacement', () {
    const bounds = Size(800, 560);
    const p = OverlayPlacement();
    test('below the anchor when it fits, start-aligned', () {
      const anchor = Rect.fromLTWH(100, 100, 60, 20);
      expect(p.position(anchor, bounds, const Size(200, 100)), const Offset(100, 124));
    });
    test('flips above near the bottom, and shifts left at the right edge', () {
      const anchor = Rect.fromLTWH(760, 520, 30, 20);
      final o = p.position(anchor, bounds, const Size(200, 150));
      expect(o.dy, 520 - 4 - 150);
      expect(o.dx, 800 - 4 - 200);
    });
    test('a taller overlay than either side gets the larger side as its maximum', () {
      const anchor = Rect.fromLTWH(10, 400, 30, 20);
      final c = p.constraints(anchor, bounds);
      expect(c.maxHeight, 400 - 4 - 4);
    });
    test('beside the anchor (a rail tooltip), centred, kept inside', () {
      const side = OverlayPlacement(side: OverlaySide.right, align: OverlayAlign.center);
      const anchor = Rect.fromLTWH(5, 2, 34, 34);
      final o = side.position(anchor, bounds, const Size(190, 24));
      expect(o.dx, 5 + 34 + 4);
      expect(o.dy, greaterThanOrEqualTo(4));
    });
  });

  for (final (name, alignment) in [
    ('top left', Alignment.topLeft),
    ('top right', Alignment.topRight),
    ('bottom left', Alignment.bottomLeft),
    ('bottom right', Alignment.bottomRight),
  ]) {
    testWidgets('a menu opened from the $name corner stays inside the window', (tester) async {
      const window = Size(800, 560);
      await pumpAt(tester, at(alignment, menu(6)), window);
      await tester.tap(find.text('Open'));
      await tester.pump();
      await tester.pump();
      expect(find.text('Item 0'), findsOneWidget);
      expectInside(sheetRect(tester), window);
    });
  }

  testWidgets('a menu taller than the window scrolls, and arrows keep the focused item in view', (
    tester,
  ) async {
    const window = Size(900, 560);
    await pumpAt(tester, at(Alignment.topCenter, menu(40)), window);
    await tester.tap(find.text('Open'));
    await tester.pump();
    await tester.pump();
    final r = sheetRect(tester);
    expectInside(r, window);
    expect(r.height, lessThan(560));
    for (var i = 0; i < 30; i++) {
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await tester.pump();
    }
    final item = tester.getRect(find.text('Item 30'));
    expect(item.bottom, lessThanOrEqualTo(r.bottom));
    expect(item.top, greaterThanOrEqualTo(r.top));
  });

  testWidgets('a tooltip at the left and right edges stays inside the window', (tester) async {
    const window = Size(600, 400);
    final g = await tester.createGesture(kind: PointerDeviceKind.mouse);
    addTearDown(g.removePointer);
    await g.addPointer(location: Offset.zero);
    for (final alignment in [Alignment.topLeft, Alignment.topRight, Alignment.bottomRight]) {
      await pumpAt(
        tester,
        at(
          alignment,
          AxIconButton(
            icon: AxiomIcons.gear,
            label: 'A rather long tooltip for a small button',
            shortcut: 'Mod+Shift+G',
            onPressed: () {},
          ),
        ),
        window,
      );
      await g.moveTo(tester.getCenter(find.byType(AxIconButton)));
      await tester.pump(const Duration(milliseconds: 600));
      expect(find.text('A rather long tooltip for a small button'), findsOneWidget);
      expectInside(sheetRect(tester), window);
      await g.moveTo(Offset.zero);
      await tester.pump();
    }
  });

  testWidgets('a rail tooltip opens to the right of its button', (tester) async {
    await pumpAt(
      tester,
      at(
        Alignment.centerLeft,
        AxIconButton(
          icon: AxiomIcons.files,
          label: 'Files',
          tooltipSide: OverlaySide.right,
          onPressed: () {},
        ),
      ),
      const Size(600, 400),
    );
    final g = await tester.createGesture(kind: PointerDeviceKind.mouse);
    addTearDown(g.removePointer);
    await g.addPointer(location: Offset.zero);
    await g.moveTo(tester.getCenter(find.byType(AxIconButton)));
    await tester.pump(const Duration(milliseconds: 600));
    expect(sheetRect(tester).left, greaterThan(tester.getRect(find.byType(AxIconButton)).right));
  });

  testWidgets('an open dialog follows a theme change (sheet and scrim)', (tester) async {
    final theme = ValueNotifier(AxiomThemeData(colors: AxiomColors.dark));
    addTearDown(theme.dispose);
    tester.view.physicalSize = const Size(900, 600);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(
      ValueListenableBuilder(
        valueListenable: theme,
        builder: (context, data, _) => WidgetsApp(
          color: data.colors.ground,
          pageRouteBuilder: <T>(s, b) =>
              PageRouteBuilder<T>(settings: s, pageBuilder: (c, _, _) => b(c)),
          home: Builder(
            builder: (context) => Center(
              child: AxButton(
                label: 'Open',
                onPressed: () => showAxDialog<void>(
                  context: context,
                  label: 'Settings',
                  builder: (_) => const SizedBox(width: 200, height: 100),
                ),
              ),
            ),
          ),
          builder: (context, child) => AxiomTheme(
            data: data,
            child: ToastHost(child: child!),
          ),
        ),
      ),
    );
    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();
    Color sheetColour() {
      final box = tester.widget<DecoratedBox>(
        find.descendant(of: find.byType(Sheet), matching: find.byType(DecoratedBox)).first,
      );
      return (box.decoration as BoxDecoration).color!;
    }

    Color scrim() => tester
        .widgetList<ColoredBox>(find.byType(ColoredBox))
        .map((b) => b.color)
        .firstWhere((c) => c == AxiomColors.dark.scrim || c == AxiomColors.light.scrim);
    expect(sheetColour(), AxiomColors.dark.sheet);
    expect(scrim(), AxiomColors.dark.scrim);
    theme.value = AxiomThemeData(colors: AxiomColors.light);
    await tester.pump();
    expect(sheetColour(), AxiomColors.light.sheet);
    expect(scrim(), AxiomColors.light.scrim);
  });

  testWidgets('a question focuses its default answer, keeps the order, gives it the primary look', (
    tester,
  ) async {
    String? chosen;
    await pumpAt(
      tester,
      Builder(
        builder: (context) => Center(
          child: AxButton(
            label: 'Ask',
            onPressed: () async {
              chosen = await showAxDialog<String>(
                context: context,
                label: 'Changed on disk',
                builder: (_) => const AxQuestion(
                  title: 'Changed on disk',
                  detail: 'The file changed.',
                  actions: [('overwrite', 'Overwrite'), ('reload', 'Reload'), ('cancel', 'Cancel')],
                  defaultAction: 'cancel',
                ),
              );
            },
          ),
        ),
      ),
      const Size(900, 600),
    );
    await tester.tap(find.text('Ask'));
    await tester.pumpAndSettle();
    final buttons = tester.widgetList<AxButton>(
      find.descendant(of: find.byType(AxQuestion), matching: find.byType(AxButton)),
    );
    expect(buttons.map((b) => b.label), ['Overwrite', 'Reload', 'Cancel']);
    expect(buttons.firstWhere((b) => b.label == 'Cancel').kind, ButtonKind.primary);
    expect(buttons.firstWhere((b) => b.label == 'Overwrite').kind, ButtonKind.standard);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();
    expect(chosen, 'cancel');
  });

  testWidgets('toasts: at most three, a repeated message is not stacked again', (tester) async {
    await pumpAt(tester, const Center(child: Text('page')), const Size(900, 600));
    final host = tester.state<ToastHostState>(find.byType(ToastHost));
    host.show('one');
    host.show('two');
    host.show('two');
    host.show('three');
    host.show('four');
    await tester.pumpAndSettle();
    expect(host.messages, ['two', 'three', 'four']);
    await tester.pump(const Duration(seconds: 6));
    await tester.pumpAndSettle();
    expect(host.messages, isEmpty);
  });

  testWidgets('toasts sit above a ToastAvoid region instead of over it', (tester) async {
    await pumpAt(
      tester,
      const Stack(
        children: [
          Positioned(
            right: 16,
            bottom: 36,
            width: 300,
            child: ToastAvoid(child: SizedBox(height: 200)),
          ),
        ],
      ),
      const Size(900, 600),
    );
    await tester.pump();
    tester.state<ToastHostState>(find.byType(ToastHost)).show('Saved');
    await tester.pumpAndSettle();
    final toast = tester.getRect(find.text('Saved'));
    expect(toast.bottom, lessThan(600 - 36 - 200));
  });

  testWidgets('a right-click in a text field shows Cut, Copy, Paste, Select All', (tester) async {
    final text = TextEditingController(text: 'hello world');
    addTearDown(text.dispose);
    await pumpAt(
      tester,
      Center(
        child: SizedBox(
          width: 300,
          child: AxTextInput(label: 'Field', controller: text),
        ),
      ),
      const Size(900, 600),
    );
    await tester.tap(find.byType(EditableText));
    await tester.pump();
    text.selection = const TextSelection(baseOffset: 0, extentOffset: 5);
    await tester.pump();
    await tester.tap(find.byType(EditableText), buttons: kSecondaryButton);
    await tester.pumpAndSettle();
    expect(find.byType(AxTextContextMenu), findsOneWidget);
    expect(find.text('Copy'), findsOneWidget);
    expect(find.text('Select All'), findsOneWidget);
    await tester.tap(find.text('Select All'));
    await tester.pumpAndSettle();
    expect(text.selection, const TextSelection(baseOffset: 0, extentOffset: 11));
  });
}
