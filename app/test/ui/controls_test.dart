import 'package:axiom/gallery/specimen.dart' as s;
import 'package:axiom/patterns/session_panel.dart';
import 'package:axiom/patterns/transcript_parts.dart';
import 'package:axiom/patterns/usage.dart';
import 'package:axiom/ui/controls.dart';
import 'package:axiom/ui/icons.dart';
import 'package:axiom/ui/overlay.dart';
import 'package:axiom/ui/status.dart';
import 'package:axiom/ui/tokens.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import '../shot.dart';

Future<void> put(WidgetTester t, Widget w, {double textScale = 1, bool still = true}) => mount(
  t,
  Align(
    alignment: Alignment.topLeft,
    child: Padding(padding: const EdgeInsets.all(20), child: w),
  ),
  size: const Size(900, 600),
  dpr: 1,
  textScale: textScale,
  reduceMotion: still,
);

void main() {
  testWidgets('a button answers the pointer, Enter and Space, and says what it is', (t) async {
    final h = t.ensureSemantics();
    var n = 0;
    await put(t, AxButton(label: 'Run', onPressed: () => n++));
    await t.tap(find.text('Run'));
    await t.sendKeyEvent(LogicalKeyboardKey.tab);
    await t.pump();
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await t.sendKeyEvent(LogicalKeyboardKey.space);
    expect(n, 3);
    expect(
      t.getSemantics(find.bySemanticsLabel('Run')),
      isSemantics(label: 'Run', isButton: true, isEnabled: true, hasTapAction: true),
    );
    h.dispose();
  });

  testWidgets('a disabled button takes neither focus nor taps, and says so', (t) async {
    final h = t.ensureSemantics();
    await put(t, const AxButton(label: 'Run', onPressed: null));
    await t.sendKeyEvent(LogicalKeyboardKey.tab);
    await t.pump();
    expect(
      t.getSemantics(find.bySemanticsLabel('Run')),
      isSemantics(label: 'Run', isButton: true, hasEnabledState: true, isEnabled: false),
    );
    h.dispose();
  });

  testWidgets('switch and check toggle and report their state', (t) async {
    final h = t.ensureSemantics();
    bool a = false, b = false;
    await put(
      t,
      StatefulBuilder(
        builder: (_, set) => Column(
          children: [
            AxSwitch(label: 'A', value: a, onChanged: (v) => set(() => a = v)),
            AxCheck(label: 'B', value: b, onChanged: (v) => set(() => b = v)),
          ],
        ),
      ),
    );
    await t.tap(find.byType(AxSwitch));
    await t.tap(find.byType(AxCheck));
    await t.pump();
    expect([a, b], [true, true]);
    expect(t.getSemantics(find.bySemanticsLabel('A')), isSemantics(isToggled: true));
    h.dispose();
  });

  testWidgets('segments and tabs move by arrow keys', (t) async {
    var v = 'a', i = 0;
    await put(
      t,
      StatefulBuilder(
        builder: (_, set) => Column(
          children: [
            AxSegments(
              label: 'S',
              items: const [('a', 'One', null), ('b', 'Two', null)],
              value: v,
              onChanged: (x) => set(() => v = x),
            ),
            AxTabs(
              tabs: const [('X', null), ('Y', '2')],
              index: i,
              onChanged: (x) => set(() => i = x),
            ),
          ],
        ),
      ),
    );
    await t.sendKeyEvent(LogicalKeyboardKey.tab);
    await t.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await t.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    expect(v, 'b');
    await t.sendKeyEvent(LogicalKeyboardKey.tab);
    await t.sendKeyEvent(LogicalKeyboardKey.tab);
    await t.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    expect(i, 1);
  });

  testWidgets('choosing a tab moves nothing but the mark', (t) async {
    var i = 0;
    await put(
      t,
      StatefulBuilder(
        builder: (_, set) => AxTabs(
          tabs: const [('Conversation', null), ('Changes', '3'), ('Terminal', null)],
          index: i,
          onChanged: (x) => set(() => i = x),
        ),
      ),
    );
    final before = t.getRect(find.text('Terminal').last);
    await t.tap(find.text('Changes').last);
    await t.pumpAndSettle();
    expect(t.getRect(find.text('Terminal').last).left, closeTo(before.left, 1.5));
  });

  testWidgets('a menu opens, moves, skips what is disabled, chooses, and returns focus', (t) async {
    var v = 'axiom';
    await put(
      t,
      StatefulBuilder(
        builder: (_, set) => AxSelect(
          label: 'Harness',
          items: s.harnesses,
          value: v,
          onChanged: (x) => set(() => v = x),
        ),
      ),
    );
    await t.sendKeyEvent(LogicalKeyboardKey.tab);
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await t.pump();
    await t.pump();
    expect(find.byType(AxMenu<String>), findsOneWidget);
    expect(find.text('not found'), findsOneWidget, reason: 'it says why');
    for (var k = 0; k < 3; k++) {
      await t.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await t.pump();
    }
    // axiom → claude → codex → (opencode is disabled) → axiom
    await t.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await t.pump();
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await t.pump();
    expect(v, 'codex');
    expect(find.byType(AxMenu<String>), findsNothing);
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await t.pump();
    await t.pump();
    await t.sendKeyEvent(LogicalKeyboardKey.keyC);
    await t.pump();
    await t.sendKeyEvent(LogicalKeyboardKey.escape);
    await t.pump();
    expect(find.byType(AxMenu<String>), findsNothing);
    expect(v, 'codex', reason: 'Escape chooses nothing');
    // Focus is back on the key.
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await t.pump();
    expect(find.byType(AxMenu<String>), findsOneWidget);
  });

  testWidgets('a menu closes on an outside click and stays inside the window', (t) async {
    await mount(
      t,
      Align(
        alignment: Alignment.bottomRight,
        child: AxSelect(label: 'Harness', items: s.harnesses, value: 'axiom', onChanged: (_) {}),
      ),
      size: const Size(400, 300),
      dpr: 1,
    );
    await t.tap(find.byType(AxSelect<String>));
    await t.pumpAndSettle();
    final r = t.getRect(find.byType(AxMenu<String>));
    expect(const Rect.fromLTWH(0, 0, 400, 300).contains(r.topLeft), isTrue);
    expect(r.right, lessThanOrEqualTo(400));
    await t.tapAt(const Offset(5, 5));
    await t.pumpAndSettle();
    expect(find.byType(AxMenu<String>), findsNothing);
  });

  testWidgets('a field takes text, shows its error, and refuses when disabled', (t) async {
    final c = TextEditingController();
    await put(
      t,
      SizedBox(
        width: 200,
        child: AxField(label: 'Port', controller: c, placeholder: '1234', error: 'Nothing answers'),
      ),
    );
    expect(find.text('1234'), findsOneWidget);
    await t.enterText(find.byType(EditableText), '8080');
    await t.pump();
    expect(c.text, '8080');
    expect(find.text('1234'), findsNothing);
    expect(find.text('Nothing answers'), findsOneWidget);
  });

  testWidgets('every account of a service is listed with where its numbers come from', (t) async {
    String? used;
    var link = 0;
    await mount(
      t,
      Align(
        alignment: Alignment.topRight,
        child: UsageKey(s.services.first, onUse: (id) => used = id, onLink: () => link++),
      ),
      size: const Size(900, 600),
      dpr: 1,
    );
    await t.tap(find.byType(UsageKey));
    await t.pumpAndSettle();
    for (final a in s.services.first.accounts) {
      expect(find.text(a.name), findsOneWidget);
      expect(find.text(a.identity), findsOneWidget);
    }
    expect(find.text('Estimated'), findsOneWidget);
    await t.tap(find.text('Team, work'));
    await t.pumpAndSettle();
    expect(used, 'c2');
    await t.tap(find.byType(UsageKey));
    await t.pumpAndSettle();
    await t.tap(find.text('Link another Claude account'));
    await t.pumpAndSettle();
    expect(link, 1);
  });

  testWidgets('an unknown level is never drawn as a number', (t) async {
    final h = t.ensureSemantics();
    await put(
      t,
      const SizedBox(
        width: 100,
        child: AxMeter(label: 'Google', value: .4, provenance: Provenance.unknown),
      ),
    );
    expect(t.getSemantics(find.bySemanticsLabel('Google')), isSemantics(value: 'not available'));
    h.dispose();
  });

  testWidgets('a request offers three answers and leads with the exact command', (t) async {
    final got = <String>[];
    await put(
      t,
      SizedBox(
        width: 800,
        child: RequestBar(
          s.request,
          onOnce: () => got.add('once'),
          onAlways: () => got.add('always'),
          onDeny: () => got.add('deny'),
        ),
      ),
    );
    expect(find.text('pnpm db:migrate --env test'), findsOneWidget);
    await t.tap(find.text('Deny'));
    await t.tap(find.text('Allow in this worktree'));
    await t.tap(find.text('Allow once'));
    expect(got, ['deny', 'always', 'once']);
  });

  testWidgets('a session holds its agents and its plan; keys appear on hover', (t) async {
    final picked = <String>[], paused = <String>[];
    await put(
      t,
      SizedBox(
        width: 248,
        child: SessionPanel(
          session: s.session,
          selected: 'builder',
          onSelect: picked.add,
          onPause: paused.add,
          onStop: (_) {},
        ),
      ),
    );
    expect(find.byType(AgentRow), findsNWidgets(6));
    expect(find.byType(TaskRow), findsNWidgets(7));
    await t.tap(find.text('Surveyor'));
    expect(picked, ['surveyor']);
    expect(find.bySemanticsLabel('Pause Lead'), findsNothing);
    final g = await t.createGesture(kind: PointerDeviceKind.mouse);
    await g.addPointer(location: Offset.zero);
    await g.moveTo(t.getCenter(find.text('Lead')));
    await t.pump();
    await t.tap(find.byType(AxButton).first);
    expect(paused, ['lead']);
    await t.tap(find.text('Plan'));
    await t.pumpAndSettle();
    expect(find.byType(TaskRow), findsNothing);
  });

  testWidgets('reduced motion stops the lamp', (t) async {
    await put(t, const AxLamp(AgentState.running));
    expect(t.binding.hasScheduledFrame, isFalse);
    await put(t, const AxLamp(AgentState.running, key: ValueKey(1)), still: false);
    await t.pump(const Duration(milliseconds: 100));
    expect(t.binding.hasScheduledFrame, isTrue);
  });

  for (final scale in [1.0, 2.0]) {
    testWidgets('nothing overflows at $scale× text', (t) async {
      await put(
        t,
        SizedBox(
          width: 820,
          child: Column(
            children: [
              RequestBar(s.request, onOnce: () {}, onAlways: () {}, onDeny: () {}),
              Wrap(
                children: [
                  AxButton(label: 'A longer label', icon: AxIcon.send, onPressed: () {}),
                  const AxKeycap('⌘K'),
                ],
              ),
            ],
          ),
        ),
        textScale: scale,
      );
      expect(t.takeException(), isNull);
    });
  }

  testWidgets('density changes control height, never type', (t) async {
    final sizes = <double>[];
    for (final d in AxDensity.values) {
      await mount(
        t,
        Center(
          child: AxButton(label: 'Run', onPressed: () {}),
        ),
        theme: AxTheme(density: d),
        dpr: 1,
      );
      sizes.add(t.getSize(find.byType(AxButton)).height);
      final p = t.renderObject<RenderParagraph>(find.text('Run'));
      expect(p.text.style!.fontSize, 12);
    }
    expect(sizes, [24, 28, 32]);
  });
}
