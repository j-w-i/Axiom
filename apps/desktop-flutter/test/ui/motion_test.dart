// Reduced motion and the OS preference (AUD-054), stepped and paused loops (AUD-080).

import 'dart:math' as math;

import 'package:axiom_desktop/ui/controls.dart';
import 'package:axiom_desktop/ui/overlays.dart';
import 'package:axiom_desktop/ui/tabs.dart';
import 'package:axiom_desktop/ui/theme.dart';
import 'package:axiom_desktop/ui/tokens.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

/// The app with a motion mode and, optionally, the system asking for reduced motion.
Widget app(Widget home, {MotionMode mode = MotionMode.standard, bool systemReduces = false}) =>
    MediaQuery(
      data: MediaQueryData(disableAnimations: systemReduces),
      child: WidgetsApp(
        color: AxiomColors.dark.ground,
        pageRouteBuilder: <T>(s, b) =>
            PageRouteBuilder<T>(settings: s, pageBuilder: (c, _, _) => b(c)),
        home: home,
        builder: (context, child) => MediaQuery(
          data: MediaQuery.of(context).copyWith(disableAnimations: systemReduces),
          child: AxiomTheme(
            data: AxiomThemeData(colors: AxiomColors.dark, motionMode: mode),
            child: ToastHost(child: child!),
          ),
        ),
      ),
    );

MotionMode modeIn(WidgetTester tester, Finder f) => AxiomTheme.of(tester.element(f)).motion.mode;

double spinnerAngle(WidgetTester tester) {
  final paint = tester.widget<CustomPaint>(
    find.descendant(of: find.byType(Spinner), matching: find.byType(CustomPaint)),
  );
  return (paint.painter! as dynamic).angle as double;
}

void main() {
  test('the system preference raises Standard to Reduced, and leaves Reduced and Off', () {
    expect(effectiveMotion(MotionMode.standard, systemReduces: true), MotionMode.reduced);
    expect(effectiveMotion(MotionMode.standard, systemReduces: false), MotionMode.standard);
    expect(effectiveMotion(MotionMode.off, systemReduces: true), MotionMode.off);
    expect(effectiveMotion(MotionMode.reduced, systemReduces: true), MotionMode.reduced);
  });

  testWidgets('MediaQuery.disableAnimations makes the theme in scope Reduced', (tester) async {
    await tester.pumpWidget(app(const Text('x'), systemReduces: true));
    expect(modeIn(tester, find.text('x')), MotionMode.reduced);
    final m = AxiomTheme.of(tester.element(find.text('x'))).motion;
    expect(m.glides, isFalse);
    expect(m.loops, isFalse);
    expect(m.travels, isFalse);
  });

  testWidgets('Reduced: the spinner is still and switching tabs runs no animation', (tester) async {
    var selected = 'a';
    await tester.pumpWidget(
      app(
        StatefulBuilder(
          builder: (context, set) => Column(
            children: [
              const Spinner(),
              AxTabs(
                label: 'Views',
                tabs: const [
                  TabItem(id: 'a', label: 'First'),
                  TabItem(id: 'b', label: 'Second'),
                ],
                selected: selected,
                onSelect: (id) => set(() => selected = id),
              ),
            ],
          ),
        ),
        systemReduces: true,
      ),
    );
    await tester.pump();
    expect((tester.state(find.byType(Spinner)) as dynamic).turning, isFalse);
    await tester.tap(find.text('Second'));
    await tester.pump();
    await tester.pump();
    expect(tester.hasRunningAnimations, isFalse);
    expect(tester.binding.hasScheduledFrame, isFalse);
  });

  testWidgets('Standard: the spinner turns in eight steps and stops while the window is '
      'inactive', (tester) async {
    await tester.pumpWidget(app(const Center(child: Spinner())));
    final angles = <double>{};
    for (var i = 0; i < 12; i++) {
      await tester.pump(const Duration(milliseconds: 60));
      angles.add(spinnerAngle(tester));
    }
    expect(angles.length, greaterThan(1));
    for (final a in angles) {
      final step = a / (2 * math.pi / Spinner.steps);
      expect((step - step.round()).abs(), lessThan(1e-9), reason: 'a stepped angle');
    }
    WindowActivity.instance.debugSet(AppLifecycleState.inactive);
    await tester.pump();
    expect((tester.state(find.byType(Spinner)) as dynamic).turning, isFalse);
    WindowActivity.instance.debugSet(AppLifecycleState.resumed);
    await tester.pump();
    expect((tester.state(find.byType(Spinner)) as dynamic).turning, isTrue);
  });

  testWidgets('a still spinner (another turns in the region) never ticks', (tester) async {
    await tester.pumpWidget(app(const Center(child: Spinner(still: true))));
    await tester.pump(const Duration(milliseconds: 500));
    expect((tester.state(find.byType(Spinner)) as dynamic).turning, isFalse);
  });

  testWidgets('Off: a dialog is shown at once, with no transition', (tester) async {
    await tester.pumpWidget(
      app(
        Builder(
          builder: (context) => Center(
            child: AxButton(
              label: 'Open',
              onPressed: () => showAxDialog<void>(
                context: context,
                label: 'D',
                builder: (_) => const Text('inside'),
              ),
            ),
          ),
        ),
        mode: MotionMode.off,
      ),
    );
    await tester.tap(find.text('Open'));
    await tester.pump();
    await tester.pump();
    expect(find.text('inside'), findsOneWidget);
    expect(tester.hasRunningAnimations, isFalse);
    expect(find.byType(ScaleTransition), findsNothing);
  });

  testWidgets('Reduced: a dialog fades without scaling', (tester) async {
    await tester.pumpWidget(
      app(
        Builder(
          builder: (context) => Center(
            child: AxButton(
              label: 'Open',
              onPressed: () => showAxDialog<void>(
                context: context,
                label: 'D',
                builder: (_) => const Text('inside'),
              ),
            ),
          ),
        ),
        mode: MotionMode.reduced,
      ),
    );
    await tester.tap(find.text('Open'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 20));
    expect(
      find.ancestor(of: find.text('inside'), matching: find.byType(ScaleTransition)),
      findsNothing,
    );
    await tester.pumpAndSettle();
  });

  testWidgets('Off: a toast appears at once', (tester) async {
    await tester.pumpWidget(app(const Text('page'), mode: MotionMode.off));
    tester.state<ToastHostState>(find.byType(ToastHost)).show('Hello');
    await tester.pump();
    expect(tester.hasRunningAnimations, isFalse);
    expect(find.text('Hello'), findsOneWidget);
    await tester.pump(const Duration(seconds: 6));
  });
}
