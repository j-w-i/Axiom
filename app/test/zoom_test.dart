import 'dart:ui' as ui;

import 'package:axiom/app.dart';
import 'package:axiom/host/host_store.dart';
import 'package:axiom/host/proto.dart';
import 'package:axiom/host/session_model.dart';
import 'package:axiom/shell/find.dart';
import 'package:axiom/shell/settings.dart';
import 'package:axiom/shell/shell.dart';
import 'package:axiom/shell/window.dart';
import 'package:axiom/transcript/transcript_view.dart';
import 'package:axiom/ui/commands.dart';
import 'package:axiom/ui/controls.dart';
import 'package:axiom/ui/overlay.dart';
import 'package:axiom/ui/prefs.dart';
import 'package:axiom/ui/surface.dart';
import 'package:axiom/ui/zoom.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'shot.dart';

const x = Execution(harness: 'axiom', model: 'm', connection: 'c');

/// A conversation with an agent that can be spoken to and asks something long.
SessionModel talk({int said = 1, bool asks = true}) {
  var n = 0;
  Event e(Body b) => Event(++n, n, b);
  return SessionModel('s')..apply([
    e(const SessionCreated('Double token rotation', '/me/web')),
    e(const AgentAdded('a', 'Builder', x, null, access: Access.ask)),
    e(const RunStarted('r', 'a')),
    for (var i = 0; i < said; i++) ...[
      e(ItemStarted('i$i', 'r', 'a', ItemKind.prose, '', null)),
      e(ItemEnded('i$i', Outcome.completed, 'Part $i says **one thing** and `code()`.', null)),
    ],
    if (asks)
      e(
        RequestOpened(
          'q',
          'a',
          'Run a command',
          [for (var i = 0; i < 12; i++) 'make part$i'].join('\n'),
          'in the folder',
          Answerable.live,
          digest: 'd',
        ),
      ),
  ]);
}

HostStore store([SessionModel? m]) =>
    HostStore(paths: HostPaths('/nonexistent'), binary: null, spawn: false)
      ..phase = HostPhase.connected
      ..current = m ?? talk();

/// The whole application, as main.dart puts it together.
Future<ValueNotifier<Prefs>> open(
  WidgetTester t,
  HostStore s, {
  Prefs prefs = const Prefs(look: Look.dark),
  Size size = const Size(1280, 820),
  double dpr = 1,
  double inset = 72,
}) async {
  t.view.devicePixelRatio = dpr;
  t.view.physicalSize = size * dpr;
  t.platformDispatcher.accessibilityFeaturesTestValue = const FakeAccessibilityFeatures(
    disableAnimations: true,
  );
  addTearDown(t.view.reset);
  addTearDown(t.platformDispatcher.clearAccessibilityFeaturesTestValue);
  await t.runAsync(loadFonts);
  await t.runAsync(Shades.load);
  final v = ValueNotifier(prefs);
  addTearDown(v.dispose);
  addTearDown(s.dispose);
  addTearDown(() => t.pumpWidget(const SizedBox()));
  await t.pumpWidget(
    RepaintBoundary(
      key: stage,
      child: AxiomApp(
        prefs: v,
        home: Shell(s, titleInset: inset),
      ),
    ),
  );
  await t.pump(const Duration(milliseconds: 400));
  await t.pump(const Duration(milliseconds: 400));
  return v;
}

Future<void> press(WidgetTester t, LogicalKeyboardKey k, {bool shift = false}) async {
  final held = [
    defaultTargetPlatform == TargetPlatform.macOS
        ? LogicalKeyboardKey.metaLeft
        : LogicalKeyboardKey.controlLeft,
    if (shift) LogicalKeyboardKey.shiftLeft,
  ];
  for (final h in held) {
    await t.sendKeyDownEvent(h);
  }
  if (k == LogicalKeyboardKey.add || k.keyLabel.startsWith('Numpad')) {
    // Not all of the tests' own keyboards have it.
    const at = PhysicalKeyboardKey.equal;
    for (final e in [
      KeyDownEvent(physicalKey: at, logicalKey: k, timeStamp: Duration.zero),
      KeyUpEvent(physicalKey: at, logicalKey: k, timeStamp: Duration.zero),
    ]) {
      HardwareKeyboard.instance.handleKeyEvent(e);
    }
  } else {
    await t.sendKeyEvent(k);
  }
  for (final h in held) {
    await t.sendKeyUpEvent(h);
  }
  await t.pump();
  await t.pump(const Duration(milliseconds: 400));
}

Future<ByteData> picture(WidgetTester t, double dpr) async => (await t.runAsync(() async {
  final r = stage.currentContext!.findRenderObject()! as RenderRepaintBoundary;
  final img = await r.toImage(pixelRatio: dpr);
  return (await img.toByteData(format: ui.ImageByteFormat.rawRgba))!;
}))!;

void main() {
  testWidgets('it fills the window, and all of it is as much larger', (t) async {
    late Size room;
    late double dpr, scale;
    final prefs = ValueNotifier(const Prefs());
    addTearDown(prefs.dispose);
    addTearDown(t.view.reset);
    t.view.devicePixelRatio = 2;
    t.view.physicalSize = const Size(1600, 1200);
    t.view.padding = const FakeViewPadding(top: 40);
    const inner = Key('inner');
    var born = 0;
    await t.pumpWidget(
      AxiomApp(
        prefs: prefs,
        home: _Counts(
          () => born++,
          Builder(
            builder: (context) {
              room = MediaQuery.sizeOf(context);
              dpr = MediaQuery.devicePixelRatioOf(context);
              scale = Zoom.of(context);
              return const Align(
                alignment: Alignment.bottomRight,
                child: SizedBox(key: inner, width: 100, height: 50),
              );
            },
          ),
        ),
      ),
    );
    for (final z in Prefs.zooms) {
      prefs.value = Prefs(zoom: z);
      await t.pump();
      expect(room.width, moreOrLessEquals(800 / z));
      expect(room.height, moreOrLessEquals(600 / z));
      expect(dpr, moreOrLessEquals(2 * z));
      expect(scale, z);
      final r = t.getRect(find.byKey(inner));
      expect(r.width, moreOrLessEquals(100 * z));
      expect(r.height, moreOrLessEquals(50 * z));
      expect(r.right, moreOrLessEquals(800));
      expect(r.bottom, moreOrLessEquals(600));
    }
    expect(born, 1, reason: 'what is on the screen stays');
  });

  testWidgets('it is drawn for the size it is seen at, and not enlarged', (t) async {
    // Twice as large is what a display with twice as many dots shows of half the room.
    // Where the system has buttons of its own in the window they do not grow.
    await open(t, store(), size: const Size(760, 520), dpr: 2, inset: 0);
    final sharp = await picture(t, 2);
    await t.pumpWidget(const SizedBox());
    await open(
      t,
      store(),
      prefs: const Prefs(look: Look.dark, zoom: 2),
      size: const Size(1520, 1040),
      inset: 0,
    );
    final zoomed = await picture(t, 1);
    expect(zoomed.lengthInBytes, sharp.lengthInBytes);
    var apart = 0;
    for (var i = 0; i < sharp.lengthInBytes; i++) {
      if ((sharp.getUint8(i) - zoomed.getUint8(i)).abs() > 8) apart++;
    }
    expect(apart / sharp.lengthInBytes, lessThan(.002));
  });

  testWidgets('nothing overflows or is out of reach in the smallest window, at any size', (
    t,
  ) async {
    for (final zoom in Prefs.zooms) {
      for (final text in [1.0, 2.0]) {
        for (final page in [
          null,
          Command.start,
          Command.accounts,
          Command.settings,
          Command.find,
        ]) {
          final s = store();
          await open(
            t,
            s,
            prefs: Prefs(zoom: zoom, text: text),
            size: const Size(760, 520),
          );
          if (page != null) {
            await press(
              t,
              page == Command.settings ? LogicalKeyboardKey.comma : page.key!,
              shift: page.shift,
            );
          }
          expect(t.takeException(), isNull, reason: '$zoom $text $page');
          // Every key of the header is inside the window, and clear of the window's own buttons.
          if (page == null) {
            for (final k in ['Search', 'Accounts', 'New session']) {
              final f = find.bySemanticsLabel(k).hitTestable();
              expect(f, findsWidgets, reason: '$k at $zoom');
              final r = t.getRect(f.first);
              expect(r.overlaps(const Rect.fromLTRB(0, 0, 72, 40)), isFalse, reason: '$k at $zoom');
              expect(r.right, lessThanOrEqualTo(760));
            }
          }
          if (page == null && text == 1 && (zoom == 2 || zoom == .8)) {
            await snap(t, 'zoom-${(zoom * 100).round()}-narrow');
          }
          await t.pumpWidget(const SizedBox());
        }
      }
    }
  });

  testWidgets('a request can be read to its end and answered in the smallest window', (t) async {
    await open(t, store(), prefs: const Prefs(zoom: 2), size: const Size(760, 520));
    final deny = find.text('Deny');
    await t.ensureVisible(deny);
    await t.pump(const Duration(seconds: 1));
    expect(deny.hitTestable(), findsOneWidget);
    expect((Offset.zero & const Size(760, 520)).contains(t.getCenter(deny)), isTrue);
    // A yes only when it has been read to its end, by the wheel.
    bool can() => t.widget<AxButton>(find.widgetWithText(AxButton, 'Allow once')).onPressed != null;
    expect(can(), isFalse);
    final lines = find.textContaining('make part0');
    await t.ensureVisible(lines);
    await t.pump();
    for (var i = 0; i < 12 && !can(); i++) {
      final on = t.getRect(lines).intersect(const Rect.fromLTRB(0, 160, 760, 290));
      t.binding.handlePointerEvent(
        PointerScrollEvent(position: on.center, scrollDelta: const Offset(0, 40)),
      );
      await t.pump(const Duration(milliseconds: 300));
    }
    await t.pump(const Duration(seconds: 1));
    expect(can(), isTrue);
  });

  testWidgets('what is under the window\'s own buttons is free of keys', (t) async {
    for (final zoom in Prefs.zooms) {
      await open(
        t,
        store(),
        prefs: Prefs(zoom: zoom),
        size: const Size(1760, 1000),
      );
      final hide = t.getRect(find.bySemanticsLabel('Hide the sidebar'));
      expect(hide.left, greaterThan(72), reason: '$zoom');
      expect(t.getRect(find.text('Axiom')).left, greaterThanOrEqualTo(72), reason: '$zoom');
      await t.pumpWidget(const SizedBox());
    }
  });

  testWidgets('menus hang where they belong, and are pressed where they are seen', (t) async {
    for (final zoom in Prefs.zooms) {
      final prefs = await open(
        t,
        store(),
        prefs: Prefs(zoom: zoom),
        size: zoom > 1 ? const Size(760, 520) : const Size(1280, 820),
      );
      await press(t, LogicalKeyboardKey.comma, shift: true);
      final anchor = find.bySemanticsLabel(RegExp('^Appearance: '));
      await t.ensureVisible(anchor);
      await t.pump();
      final at = t.getRect(anchor);
      await t.tap(anchor);
      await t.pump();
      await t.pump(const Duration(milliseconds: 400));
      final menu = t.getRect(find.byType(AxMenu<Look>));
      expect(menu.left, moreOrLessEquals(at.left + 6 * zoom, epsilon: .01), reason: '$zoom');
      expect(menu.top, moreOrLessEquals(at.bottom + 11 * zoom, epsilon: .01), reason: '$zoom');
      await t.pump(const Duration(seconds: 1));
      await t.tap(find.descendant(of: find.byType(AxMenu<Look>), matching: find.text('Light')));
      await t.pump();
      expect(prefs.value.look, Look.light, reason: '$zoom');
      await t.pumpWidget(const SizedBox());
    }
  });

  testWidgets('a menu does not stay behind when everything changes its size', (t) async {
    await open(t, store());
    await press(t, LogicalKeyboardKey.comma, shift: true);
    await t.tap(find.bySemanticsLabel(RegExp('^Appearance: ')));
    await t.pump();
    expect(find.byType(AxMenu<Look>), findsOneWidget);
    await press(t, LogicalKeyboardKey.equal);
    expect(find.byType(AxMenu<Look>), findsNothing);
  });

  testWidgets('keys make it larger and smaller, to an end, and as it was', (t) async {
    final mac = defaultTargetPlatform == TargetPlatform.macOS;
    final prefs = await open(t, store());
    double z() => prefs.value.zoom;
    await press(t, LogicalKeyboardKey.equal);
    expect(z(), 1.1);
    // As plus is typed with Shift, on a keyboard that has it as a key, and on the pad.
    await press(t, LogicalKeyboardKey.equal, shift: true);
    await press(t, LogicalKeyboardKey.add, shift: true);
    expect(z(), 1.5);
    await press(t, LogicalKeyboardKey.add);
    await press(t, LogicalKeyboardKey.numpadAdd);
    await press(t, LogicalKeyboardKey.add);
    expect(z(), 2);
    await press(t, LogicalKeyboardKey.numpadSubtract);
    expect(z(), 1.75);
    for (var i = 0; i < 12; i++) {
      await press(t, LogicalKeyboardKey.minus);
    }
    expect(z(), .8);
    await press(t, LogicalKeyboardKey.minus, shift: true);
    await press(t, LogicalKeyboardKey.digit0);
    expect(z(), 1);
    await press(t, LogicalKeyboardKey.minus);
    await press(t, LogicalKeyboardKey.numpad0);
    expect(z(), 1);
    // Text is another thing.
    expect(prefs.value.text, 1);
    expect(Command.zoomIn.keys, mac ? '⌘+' : 'Ctrl++');
    expect(Command.zoomOut.keys, mac ? '⌘-' : 'Ctrl+-');
    expect(Command.zoomReset.keys, mac ? '⌘0' : 'Ctrl+0');
    // Neither by the letter alone.
    await t.sendKeyEvent(LogicalKeyboardKey.equal);
    await t.pump();
    expect(z(), 1);
  }, variant: TargetPlatformVariant.desktop());

  testWidgets('it and the size of text are found, also those without a key', (t) async {
    final prefs = await open(t, store());
    Future<void> ask(String words) async {
      await press(t, Command.find.key!);
      await t.enterText(
        find.descendant(of: find.byType(SearchBox), matching: find.byType(EditableText)),
        words,
      );
      await t.pump();
      await t.sendKeyEvent(LogicalKeyboardKey.enter);
      await t.pump();
    }

    await ask('zoom in');
    expect(prefs.value.zoom, 1.1);
    await ask('zoom out');
    expect(prefs.value.zoom, 1);
    await ask('zoom out');
    await ask('actual size');
    expect(prefs.value.zoom, 1);
    await ask('larger text');
    expect(prefs.value.text, 1.15);
    await ask('usual text');
    expect((prefs.value.text, prefs.value.zoom), (1, 1));
    expect(Command.larger.keys, isNull);
  });

  testWidgets('it is chosen on the page', (t) async {
    final prefs = await open(t, store());
    await press(t, LogicalKeyboardKey.comma, shift: true);
    expect(find.byType(SettingsPage), findsOneWidget);
    await t.tap(find.bySemanticsLabel('Zoom: 100%'));
    await t.pump();
    await t.pump(const Duration(seconds: 1));
    await t.tap(find.descendant(of: find.byType(AxMenu<double>), matching: find.text('150%')));
    await t.pump();
    expect(prefs.value.zoom, 1.5);
    expect(find.bySemanticsLabel('Zoom: 150%'), findsOneWidget);
  });

  testWidgets('the window is moved by its header at every size', (t) async {
    final was = window, asked = <String>[];
    window = (what) async => asked.add(what);
    addTearDown(() => window = was);
    for (final zoom in [.8, 2.0]) {
      await open(t, store(), prefs: Prefs(zoom: zoom));
      await t.drag(
        find.byType(MoveArea).last,
        const Offset(40, 10),
        kind: PointerDeviceKind.mouse,
        warnIfMissed: false,
      );
      await t.pump();
      expect(asked, ['move'], reason: '$zoom');
      asked.clear();
      await t.pumpWidget(const SizedBox());
    }
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('the panel\'s edge goes with the hand', (t) async {
    for (final zoom in [.8, 1.0, 1.5]) {
      await open(
        t,
        store(talk(asks: false)),
        prefs: Prefs(zoom: zoom),
        size: const Size(1800, 900),
      );
      await press(t, Command.panel.key!);
      final handle = find.bySemanticsLabel('Width of the panel');
      final before = t.getCenter(handle).dx;
      final g = await t.startGesture(t.getCenter(handle), kind: PointerDeviceKind.mouse);
      // Past where it is taken for a drag.
      await g.moveBy(const Offset(-30, 0));
      await t.pump();
      final from = t.getCenter(handle).dx;
      await g.moveBy(const Offset(-20, 0));
      await t.pump();
      expect(t.getCenter(handle).dx, moreOrLessEquals(from - 20, epsilon: 1.01), reason: '$zoom');
      expect(from, lessThan(before));
      await g.up();
      await t.pump();
      await t.pumpWidget(const SizedBox());
    }
  });

  testWidgets('of a long conversation only what is seen is built, and the place is kept', (
    t,
  ) async {
    final s = store(talk(said: 3000, asks: false));
    final prefs = await open(t, s);
    int built() => t.widgetList(find.byType(BlockView, skipOffstage: false)).length;
    final view = find.byType(TranscriptView);
    await t.drag(view, const Offset(0, 900));
    await t.pump(const Duration(seconds: 1));
    String? first() {
      final top = t.getRect(view).top;
      String? best;
      var at = double.infinity;
      for (final e in find.byType(BlockView).evaluate()) {
        final r = t.getRect(find.byWidget(e.widget));
        if (r.bottom > top && r.top < at) {
          at = r.top;
          best = (e.widget as BlockView).block.text;
        }
      }
      return best;
    }

    final reading = first();
    for (final zoom in [2.0, .8, 1.0]) {
      prefs.value = prefs.value.copyWith(zoom: zoom);
      await t.pump();
      await t.pump(const Duration(milliseconds: 400));
      expect(built(), lessThan(140), reason: '$zoom');
      expect(t.takeException(), isNull);
    }
    expect(first(), reading);
  });

  testWidgets('who reads along at the end stays there', (t) async {
    final prefs = await open(t, store(talk(said: 300, asks: false)));
    for (final zoom in [2.0, .8, 1.5]) {
      prefs.value = prefs.value.copyWith(zoom: zoom);
      await t.pump();
      await t.pump(const Duration(milliseconds: 400));
      await t.pump(const Duration(milliseconds: 400));
      final last = t.getRect(find.textContaining('Part 299'));
      final view = t.getRect(find.byType(TranscriptView));
      expect(last.bottom, lessThanOrEqualTo(view.bottom + 1), reason: '$zoom');
      expect(last.top, greaterThan(view.center.dy), reason: '$zoom');
    }
  });

  testWidgets('the keyboard\'s ring and the field follow', (t) async {
    await open(t, store(talk(asks: false)), prefs: const Prefs(zoom: 2));
    final field = find.byType(EditableText).first;
    await t.tap(field);
    await t.pump();
    await t.enterText(field, 'words');
    expect(t.widget<EditableText>(field).controller.text, 'words');
    await t.sendKeyEvent(LogicalKeyboardKey.tab);
    await t.pump();
    final on = FocusManager.instance.primaryFocus!;
    expect((Offset.zero & const Size(1280, 820)).contains(on.rect.center), isTrue);
  });
}

class _Counts extends StatefulWidget {
  const _Counts(this.born, this.child);
  final VoidCallback born;
  final Widget child;
  @override
  State<_Counts> createState() => _CountsState();
}

class _CountsState extends State<_Counts> {
  @override
  void initState() {
    super.initState();
    widget.born();
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
