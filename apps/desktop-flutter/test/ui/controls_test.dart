// Behaviour of the design system's controls (FL-DS-001): names and roles, keyboard operation,
// visible focus, overlays that give focus back.

import 'dart:ui' show CheckedState, SemanticsRole, Tristate;

import 'package:axiom_desktop/main.dart';
import 'package:axiom_desktop/ui/controls.dart';
import 'package:axiom_desktop/ui/gallery.dart';
import 'package:axiom_desktop/ui/icons.dart';
import 'package:axiom_desktop/ui/overlays.dart';
import 'package:axiom_desktop/ui/pressable.dart';
import 'package:axiom_desktop/ui/splitter.dart';
import 'package:axiom_desktop/ui/tabs.dart';
import 'package:axiom_desktop/ui/text_input.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

Future<void> pumpApp(WidgetTester tester, Widget home, {Size size = const Size(1100, 1500)}) async {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(AxiomApp(home: home));
  await tester.pump();
}

Future<void> key(WidgetTester tester, LogicalKeyboardKey k) async {
  await tester.sendKeyEvent(k);
  await tester.pump();
}

bool ringVisible(WidgetTester tester, Finder of) => tester
    .widgetList<FocusRing>(find.descendant(of: of, matching: find.byType(FocusRing)))
    .any((r) => r.visible);

void main() {
  testWidgets('a button: Tab focuses it with a visible ring; Enter and Space activate it', (
    tester,
  ) async {
    var n = 0;
    await pumpApp(
      tester,
      Center(
        child: AxButton(label: 'Run', onPressed: () => n++),
      ),
    );
    await key(tester, LogicalKeyboardKey.tab);
    expect(ringVisible(tester, find.byType(AxButton)), isTrue);
    await key(tester, LogicalKeyboardKey.enter);
    await key(tester, LogicalKeyboardKey.space);
    expect(n, 2);
  });

  testWidgets('a disabled button is announced as disabled and takes no focus', (tester) async {
    final handle = tester.ensureSemantics();
    await pumpApp(
      tester,
      const Column(
        children: [
          AxButton(label: 'Off', onPressed: null),
          AxButton(label: 'On', onPressed: _noop),
        ],
      ),
    );
    await key(tester, LogicalKeyboardKey.tab);
    expect(ringVisible(tester, find.widgetWithText(AxButton, 'Off')), isFalse);
    expect(ringVisible(tester, find.widgetWithText(AxButton, 'On')), isTrue);
    final off = tester.getSemantics(find.widgetWithText(AxButton, 'Off'));
    expect(off.label, 'Off');
    expect(off.flagsCollection.isEnabled, Tristate.isFalse);
    handle.dispose();
  });

  testWidgets('the hint line follows hover and keyboard focus', (tester) async {
    await pumpApp(
      tester,
      Center(
        child: AxIconButton(
          icon: AxiomIcons.sidebar,
          label: 'Toggle side bar',
          shortcut: 'Mod+B',
          onPressed: () {},
        ),
      ),
    );
    final hint = HintScope.maybeOf(tester.element(find.byType(AxIconButton)))!;
    await key(tester, LogicalKeyboardKey.tab);
    expect(hint.value, 'Toggle side bar — Mod+B');
  });

  testWidgets('a tooltip shows the name and the shortcut as keycaps on keyboard focus', (
    tester,
  ) async {
    await pumpApp(
      tester,
      Center(
        child: AxIconButton(
          icon: AxiomIcons.sidebar,
          label: 'Toggle side bar',
          shortcut: 'Mod+B',
          onPressed: () {},
        ),
      ),
    );
    await key(tester, LogicalKeyboardKey.tab);
    await tester.pump();
    expect(find.text('Toggle side bar'), findsOneWidget);
    expect(find.byType(Keycaps), findsOneWidget);
  });

  testWidgets('a menu: Enter opens it on the first item; arrows skip headings and disabled items; '
      'Enter chooses; Escape closes and focus returns to the trigger', (tester) async {
    final chosen = <String>[];
    await pumpApp(
      tester,
      Align(
        alignment: Alignment.topLeft,
        child: AxMenu(
          label: 'File menu',
          entries: () => [
            const MenuEntry.heading('Workspace'),
            MenuEntry.item('Open', () => chosen.add('Open')),
            const MenuEntry.item('Disabled', null, enabled: false),
            MenuEntry.item('Close', () => chosen.add('Close')),
          ],
          trigger: (context, s, open) => const Text('File'),
        ),
      ),
    );
    await key(tester, LogicalKeyboardKey.tab);
    await key(tester, LogicalKeyboardKey.enter);
    await tester.pump();
    expect(find.text('Open'), findsOneWidget);
    // The first item has focus: Enter chooses it.
    await key(tester, LogicalKeyboardKey.enter);
    await tester.pump();
    expect(chosen, ['Open']);
    expect(find.text('Open'), findsNothing);
    // Again: ↓ skips the disabled item.
    await key(tester, LogicalKeyboardKey.enter);
    await tester.pump();
    await key(tester, LogicalKeyboardKey.arrowDown);
    await key(tester, LogicalKeyboardKey.enter);
    await tester.pump();
    expect(chosen, ['Open', 'Close']);
    // Open again and Escape: focus is back on the trigger.
    await key(tester, LogicalKeyboardKey.enter);
    await tester.pump();
    expect(find.text('Open'), findsOneWidget);
    await key(tester, LogicalKeyboardKey.escape);
    await tester.pump();
    expect(find.text('Open'), findsNothing);
    expect(FocusManager.instance.primaryFocus?.debugLabel, 'menu trigger');
  });

  testWidgets('tabs: role tab and selected; → selects the next tab and moves focus', (
    tester,
  ) async {
    final handle = tester.ensureSemantics();
    var selected = 'a';
    await pumpApp(
      tester,
      StatefulBuilder(
        builder: (context, setState) => Center(
          child: AxTabs(
            label: 'Groups',
            tabs: const [
              TabItem(id: 'a', label: 'Alpha'),
              TabItem(id: 'b', label: 'Beta'),
            ],
            selected: selected,
            onSelect: (id) => setState(() => selected = id),
          ),
        ),
      ),
    );
    final alpha = tester.getSemantics(find.text('Alpha'));
    expect(alpha.getSemanticsData().role, SemanticsRole.tab);
    expect(alpha.flagsCollection.isSelected, Tristate.isTrue);
    await key(tester, LogicalKeyboardKey.tab);
    await key(tester, LogicalKeyboardKey.arrowRight);
    await tester.pump();
    expect(selected, 'b');
    expect(tester.getSemantics(find.text('Beta')).flagsCollection.isSelected, Tristate.isTrue);
    handle.dispose();
  });

  testWidgets('a splitter: → widens by 16, Home resets, the value is announced', (tester) async {
    final handle = tester.ensureSemantics();
    var w = 200.0;
    await pumpApp(
      tester,
      StatefulBuilder(
        builder: (context, setState) => Row(
          children: [
            SizedBox(width: w),
            AxSplitter(
              label: 'Resize the side bar',
              value: w,
              min: 100,
              max: 400,
              defaultValue: 200,
              onChanged: (v) => setState(() => w = v),
            ),
            const Expanded(child: SizedBox()),
          ],
        ),
      ),
    );
    await key(tester, LogicalKeyboardKey.tab);
    await key(tester, LogicalKeyboardKey.arrowRight);
    expect(w, 216);
    expect(tester.getSemantics(find.byType(AxSplitter)).value, '216');
    await key(tester, LogicalKeyboardKey.home);
    expect(w, 200);
    handle.dispose();
  });

  testWidgets('a text field is announced with its name; the placeholder goes when text is typed', (
    tester,
  ) async {
    final handle = tester.ensureSemantics();
    await pumpApp(
      tester,
      const Center(
        child: SizedBox(
          width: 300,
          child: AxTextInput(label: 'Search', placeholder: 'Search files'),
        ),
      ),
    );
    expect(tester.getSemantics(find.byType(EditableText)).label, contains('Search'));
    await tester.enterText(find.byType(EditableText), 'needle');
    await tester.pump();
    expect(find.text('Search files'), findsNothing);
    handle.dispose();
  });

  testWidgets('a toast is announced (live region) and takes no focus', (tester) async {
    final handle = tester.ensureSemantics();
    await pumpApp(
      tester,
      Center(
        child: AxButton(label: 'Go', onPressed: _noop),
      ),
    );
    await key(tester, LogicalKeyboardKey.tab);
    final before = FocusManager.instance.primaryFocus;
    ToastHost.of(tester.element(find.byType(AxButton))).show('Saved', kind: ToastKind.ok);
    await tester.pump();
    final node = tester.getSemantics(find.bySemanticsLabel('Saved'));
    expect(node.flagsCollection.isLiveRegion, isTrue);
    expect(FocusManager.instance.primaryFocus, before);
    await tester.pump(const Duration(seconds: 6));
    handle.dispose();
  });

  testWidgets('the gallery: every interactive element has a name', (tester) async {
    final handle = tester.ensureSemantics();
    await pumpApp(tester, const Gallery());
    await tester.pump(const Duration(milliseconds: 100));
    final unnamed = <String>[];
    void visit(SemanticsNode n) {
      final d = n.getSemanticsData();
      final f = d.flagsCollection;
      final interactive =
          f.isButton ||
          f.isTextField ||
          f.isSlider ||
          f.isLink ||
          d.role == SemanticsRole.tab ||
          f.isToggled != Tristate.none ||
          f.isChecked != CheckedState.none;
      if (interactive && d.label.trim().isEmpty) unnamed.add(d.toString());
      n.visitChildren((c) {
        visit(c);
        return true;
      });
    }

    visit(RendererBinding.instance.renderViews.first.owner!.semanticsOwner!.rootSemanticsNode!);
    expect(unnamed, isEmpty);
    handle.dispose();
  });
}

void _noop() {}
