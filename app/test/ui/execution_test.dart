import 'dart:ui' show Tristate;

import 'package:axiom/gallery/workbench.dart';
import 'package:axiom/patterns/agent_bar.dart';
import 'package:axiom/patterns/context_gauge.dart';
import 'package:axiom/patterns/execution.dart';
import 'package:axiom/ui/marks.dart';
import 'package:axiom/ui/overlay.dart';
import 'package:axiom/ui/status.dart';
import 'package:axiom/ui/tokens.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter/widgets.dart';

import '../shot.dart';

/// Holds the choices as the app does: what is chosen is what the owner keeps.
class Owner extends StatefulWidget {
  const Owner({super.key});
  @override
  State<Owner> createState() => OwnerState();
}

class OwnerState extends State<Owner> {
  final value = {'Model': 'a', 'Harness': 'axiom', 'Effort': 'high', 'Access': 'tree'};
  String agent = 'agent', cost = 'Read again: about 142k tokens.';
  bool betaWorks = true, betaIs = true, reversed = false, more = false, models = true;
  void change(VoidCallback f) => setState(f);
  final changes = <String>[];

  ExecutionChoice choice(
    String k,
    List<AxMenuItem<String>> items, [
    String? Function(String)? cost,
  ]) => ExecutionChoice(
    k,
    items,
    value[k]!,
    (v) => setState(() {
      value[k] = v;
      changes.add('$k $v');
    }),
    cost: cost,
  );

  @override
  Widget build(BuildContext context) {
    final all = [
      if (models)
        choice('Model', [
          const AxMenuItem('a', 'Alpha', mark: ServiceMark.claude),
          if (betaIs)
            AxMenuItem(
              'b',
              'Beta',
              mark: ServiceMark.openai,
              enabled: betaWorks,
              why: 'signed out',
            ),
          const AxMenuItem('c', 'Gamma', enabled: false, why: 'signed out'),
        ], (_) => cost),
      choice('Harness', const [AxMenuItem('axiom', 'Axiom')]),
      choice('Effort', const [AxMenuItem('high', 'High'), AxMenuItem('low', 'Low')]),
      if (more) choice('Access', const [AxMenuItem('tree', 'Tree')]),
    ];
    return ExecutionKey(reversed ? all.reversed.toList() : all, of: agent);
  }
}

void main() {
  Future<OwnerState> open(WidgetTester t) async {
    await mount(
      t,
      const Align(alignment: Alignment.bottomLeft, child: Owner()),
      size: const Size(900, 600),
      dpr: 1,
    );
    await t.tap(find.byType(ExecutionKey));
    await t.pumpAndSettle();
    return t.state<OwnerState>(find.byType(Owner));
  }

  Future<void> tap(WidgetTester t, String label) async {
    await t.tap(find.bySemanticsLabel(label).last);
    await t.pumpAndSettle();
    await t.pump(const Duration(milliseconds: 600));
  }

  Future<void> key(WidgetTester t, LogicalKeyboardKey k) async {
    await t.sendKeyEvent(k);
    await t.pumpAndSettle();
  }

  testWidgets('the key names the model and the harness, and opens every part', (t) async {
    final h = t.ensureSemantics();
    await open(t);
    expect(find.bySemanticsLabel('Runs on Alpha, Axiom. Change'), findsOneWidget);
    for (final l in ['Model: Alpha', 'Harness: Axiom', 'Effort: High']) {
      expect(find.bySemanticsLabel(l), findsOneWidget);
    }
    expect(find.text('Beta'), findsNothing, reason: 'one part is open at a time');
    await tap(t, 'Model: Alpha');
    expect(find.text('Beta'), findsOneWidget);
    await tap(t, 'Effort: High');
    expect(find.text('Beta'), findsNothing);
    expect(find.text('Low'), findsOneWidget);
    h.dispose();
  });

  testWidgets('a model is not changed until its cost is accepted', (t) async {
    final h = t.ensureSemantics();
    final o = await open(t);
    await tap(t, 'Model: Alpha');
    await tap(t, 'Beta');
    expect(find.text('Read again: about 142k tokens.'), findsOneWidget);
    expect(o.changes, isEmpty);
    await tap(t, 'Keep Alpha');
    expect(o.changes, isEmpty);
    expect(find.text('Read again: about 142k tokens.'), findsNothing);
    await tap(t, 'Model: Alpha');
    await tap(t, 'Beta');
    await tap(t, 'Change to Beta');
    expect(o.changes, ['Model b']);
    expect(find.bySemanticsLabel('Model: Beta'), findsOneWidget);
    expect(find.bySemanticsLabel('Runs on Beta, Axiom. Change'), findsOneWidget);
    h.dispose();
  });

  testWidgets('closing the panel on a question leaves everything as it was', (t) async {
    final o = await open(t);
    await tap(t, 'Model: Alpha');
    await tap(t, 'Beta');
    await key(t, LogicalKeyboardKey.escape);
    expect(find.byType(ExecutionPanel), findsNothing);
    expect(o.changes, isEmpty);
    await t.tap(find.byType(ExecutionKey));
    await t.pumpAndSettle();
    expect(find.text('Read again: about 142k tokens.'), findsNothing);
  });

  testWidgets('opening another part drops the question', (t) async {
    final o = await open(t);
    await tap(t, 'Model: Alpha');
    await tap(t, 'Beta');
    await tap(t, 'Effort: High');
    expect(find.text('Read again: about 142k tokens.'), findsNothing);
    expect(o.changes, isEmpty);
  });

  testWidgets('what costs nothing is changed at once; what is chosen asks nothing', (t) async {
    final o = await open(t);
    await tap(t, 'Effort: High');
    await tap(t, 'Low');
    expect(o.changes, ['Effort low']);
    await tap(t, 'Model: Alpha');
    await tap(t, 'Alpha');
    expect(o.changes, ['Effort low']);
    expect(find.text('Read again: about 142k tokens.'), findsNothing);
  });

  testWidgets('a model that cannot be used says why and is not asked about', (t) async {
    final o = await open(t);
    await tap(t, 'Model: Alpha');
    expect(find.text('signed out'), findsOneWidget);
    await tap(t, 'Gamma');
    expect(find.text('Read again: about 142k tokens.'), findsNothing);
    expect(o.changes, isEmpty);
  });

  const words = 'Read again: about 142k tokens.';
  Future<OwnerState> asked(WidgetTester t) async {
    final o = await open(t);
    await tap(t, 'Model: Alpha');
    await tap(t, 'Beta');
    expect(find.text(words), findsOneWidget);
    return o;
  }

  testWidgets('by keyboard: the question takes the focus, on the safe answer', (t) async {
    final h = t.ensureSemantics();
    final o = await open(t);
    Future<void> toBeta() async {
      await key(t, LogicalKeyboardKey.enter); // Model
      await key(t, LogicalKeyboardKey.arrowDown); // Alpha
      await key(t, LogicalKeyboardKey.arrowDown); // Beta
      await key(t, LogicalKeyboardKey.enter);
    }

    bool focused(String l) =>
        t.getSemantics(find.bySemanticsLabel(l).last).flagsCollection.isFocused == Tristate.isTrue;
    await toBeta();
    expect(focused('Keep Alpha'), isTrue);
    await key(t, LogicalKeyboardKey.enter);
    expect(o.changes, isEmpty, reason: 'Enter twice keeps the model');
    expect(find.text(words), findsNothing, reason: 'the second Enter answered');
    expect(focused('Model: Alpha'), isTrue, reason: 'back on the part it was about');
    await toBeta();
    await key(t, LogicalKeyboardKey.tab);
    expect(focused('Change to Beta'), isTrue);
    await key(t, LogicalKeyboardKey.enter);
    expect(o.changes, ['Model b']);
    expect(focused('Model: Beta'), isTrue);
    await key(t, LogicalKeyboardKey.escape);
    expect(find.byType(ExecutionPanel), findsNothing);
    h.dispose();
  });

  testWidgets('a screen reader is told what it costs, at once', (t) async {
    final h = t.ensureSemantics();
    await asked(t);
    final d = t.getSemantics(find.bySemanticsLabel(words)).getSemanticsData();
    expect(d.flagsCollection.isLiveRegion, isTrue);
    expect(
      t.getSemantics(find.bySemanticsLabel('Model: Alpha')).flagsCollection.isExpanded,
      Tristate.isTrue,
    );
    h.dispose();
  });

  // The panel hangs over its key: what it grows by moves the list under the pointer.
  for (final (first, gap) in [
    ('Effort: High', 120),
    ('Model: Alpha', 120),
    ('Model: Alpha', 300),
  ]) {
    testWidgets('clicking on without moving changes nothing: $first, $gap ms', (t) async {
      final o = await open(t);
      final at = t.getRect(find.bySemanticsLabel(first));
      for (final dx in [.1, .5, .9]) {
        for (final dy in [.1, .5, .9]) {
          final p = Offset(at.left + at.width * dx, at.top + at.height * dy);
          for (var n = 0; n < 2; n++) {
            await t.tapAt(p);
            await t.pump(Duration(milliseconds: gap));
          }
          await t.pump(const Duration(milliseconds: 600));
          expect(o.changes, isEmpty);
          await key(t, LogicalKeyboardKey.escape);
          await t.tap(find.byType(ExecutionKey));
          await t.pumpAndSettle();
          await t.pump(const Duration(milliseconds: 600));
        }
      }
    });
  }

  testWidgets('a second click on a model does not answer its question', (t) async {
    final o = await open(t);
    await tap(t, 'Model: Alpha');
    final at = t.getRect(find.bySemanticsLabel('Beta'));
    for (final dx in [.05, .3, .5, .7, .9]) {
      final p = Offset(at.left + at.width * dx, at.center.dy);
      await t.tapAt(p);
      await t.pump(const Duration(milliseconds: 200));
      await t.tapAt(p);
      await t.pump(const Duration(milliseconds: 600));
      expect(o.changes, isEmpty);
      await tap(t, 'Keep Alpha');
      await tap(t, 'Model: Alpha');
    }
  });

  testWidgets('a pointer that has moved is not held up', (t) async {
    final o = await open(t);
    final g = await t.createGesture(kind: PointerDeviceKind.mouse);
    await g.addPointer(location: t.getCenter(find.bySemanticsLabel('Effort: High')));
    await g.down(t.getCenter(find.bySemanticsLabel('Effort: High')));
    await g.up();
    await t.pump();
    final low = t.getCenter(find.bySemanticsLabel('Low'));
    await g.moveTo(low);
    await t.pump();
    await g.down(low);
    await g.up();
    await t.pump();
    expect(o.changes, ['Effort low']);
    await t.pump(const Duration(milliseconds: 600));
  });

  for (final (what, change) in <(String, void Function(OwnerState))>[
    ('the model changed elsewhere', (o) => o.value['Model'] = 'b'),
    ('the model went', (o) => o.betaIs = false),
    ('the model cannot be used any more', (o) => o.betaWorks = false),
    ('the cost changed', (o) => o.cost = 'Read again: about 150k tokens.'),
    ('the part went', (o) => o.models = false),
    ('the key is another agent\'s', (o) => o.agent = 'other'),
  ]) {
    testWidgets('a question does not outlive what it was about: $what', (t) async {
      final o = await asked(t);
      o.change(() => change(o));
      await t.pumpAndSettle();
      expect(find.textContaining('Read again'), findsNothing);
      expect(find.textContaining('Change to'), findsNothing);
      o.change(() {
        o
          ..value['Model'] = 'a'
          ..betaIs = true
          ..betaWorks = true
          ..cost = words
          ..models = true
          ..agent = 'agent';
      });
      await t.pumpAndSettle();
      expect(find.textContaining('Read again'), findsNothing, reason: 'nor does it come back');
      expect(o.changes, isEmpty);
    });
  }

  testWidgets('a rebuild that changes nothing leaves the question', (t) async {
    final o = await asked(t);
    o.change(() => o.more = true);
    await t.pumpAndSettle();
    expect(find.text(words), findsOneWidget);
    await t.pump(const Duration(milliseconds: 600));
    await tap(t, 'Change to Beta');
    expect(o.changes, ['Model b']);
  });

  testWidgets('parts come, go and move, and the keyboard stays in the panel', (t) async {
    final h = t.ensureSemantics();
    final o = await open(t);
    bool inside() {
      final p = FocusManager.instance.primaryFocus!.context!;
      return p.findAncestorWidgetOfExactType<ExecutionPanel>() != null;
    }

    await key(t, LogicalKeyboardKey.arrowDown);
    for (final change in <VoidCallback>[
      () => o.reversed = true,
      () => o.more = true,
      () => o.models = false,
      () => o.reversed = false,
    ]) {
      o.change(change);
      await t.pumpAndSettle();
      expect(inside(), isTrue);
    }
    h.dispose();
  });

  testWidgets('the part with the focus goes, and the first one takes it', (t) async {
    final h = t.ensureSemantics();
    final o = await open(t);
    o.change(() => o.models = false);
    await t.pumpAndSettle();
    final d = t.getSemantics(find.bySemanticsLabel('Harness: Axiom'));
    expect(d.flagsCollection.isFocused, Tristate.isTrue);
    h.dispose();
  });

  testWidgets('a panel that grows stays on the side it opened on', (t) async {
    await mount(
      t,
      const Align(alignment: Alignment(-1, .05), child: Owner()),
      size: const Size(900, 600),
      dpr: 1,
    );
    await t.tap(find.byType(ExecutionKey));
    await t.pumpAndSettle();
    final key = t.getRect(find.byType(ExecutionKey));
    expect(t.getRect(find.byType(ExecutionPanel)).top, greaterThan(key.bottom));
    await t.pump(const Duration(milliseconds: 600));
    await t.tap(find.bySemanticsLabel('Model: Alpha'));
    await t.pumpAndSettle();
    await t.pump(const Duration(milliseconds: 600));
    await t.tap(find.text('Beta'));
    await t.pumpAndSettle();
    expect(t.getRect(find.byType(ExecutionPanel)).top, greaterThan(key.bottom));
    await t.pump(const Duration(milliseconds: 600));
  });

  testWidgets('the workbench fits at large text', (t) async {
    for (final scale in [1.0, 1.5, 2.0]) {
      for (final size in const [Size(1440, 900), Size(1100, 700), Size(900, 700), Size(760, 640)]) {
        await mount(t, const Workbench(), size: size, dpr: 1, textScale: scale);
        expect(t.takeException(), isNull, reason: '$size at $scale');
      }
    }
  });

  testWidgets('among many models the question is where it can be seen', (t) async {
    await mount(
      t,
      Align(
        alignment: Alignment.bottomLeft,
        child: ExecutionKey(of: 'a', [
          ExecutionChoice(
            'Model',
            [for (var i = 0; i < 60; i++) AxMenuItem('$i', 'Model $i')],
            '0',
            (_) {},
            cost: (_) => 'It costs.',
          ),
        ]),
      ),
      size: const Size(900, 600),
      dpr: 1,
    );
    await t.tap(find.byType(ExecutionKey));
    await t.pumpAndSettle();
    await t.tap(find.bySemanticsLabel('Model: Model 0'));
    await t.pumpAndSettle();
    await t.pump(const Duration(milliseconds: 600));
    // The last one that is in view.
    final last = [
      for (var i = 1; i < 60; i++)
        if (t.getRect(find.text('Model $i')).bottom < 548) i,
    ].last;
    await t.tap(find.text('Model $last'));
    await t.pumpAndSettle();
    expect(t.getRect(find.text('It costs.')).bottom, lessThan(592));
    expect(t.getRect(find.text('Change to Model $last')).bottom, lessThan(592));
  });

  testWidgets('the panel stays in a small window at large text', (t) async {
    await mount(
      t,
      const Align(alignment: Alignment.bottomLeft, child: Owner()),
      size: const Size(400, 260),
      dpr: 1,
      textScale: 1.6,
    );
    await t.tap(find.byType(ExecutionKey));
    await t.pumpAndSettle();
    await t.tap(find.bySemanticsLabel('Model: Alpha'));
    await t.pumpAndSettle();
    await t.pump(const Duration(milliseconds: 600));
    await t.ensureVisible(find.text('Beta'));
    await t.tap(find.text('Beta'));
    await t.pumpAndSettle();
    await t.pump(const Duration(milliseconds: 600));
    for (final w in ['Keep Alpha', 'Change to Beta']) {
      final r = t.getRect(find.text(w));
      expect(
        r.left >= 0 && r.right <= 400 && r.top >= 0 && r.bottom <= 260,
        isTrue,
        reason: '$w $r',
      );
    }
    expect(t.takeException(), isNull);
  });

  group('context', () {
    Future<void> show(WidgetTester t, ContextView v) => mount(
      t,
      Align(alignment: Alignment.bottomLeft, child: ContextGauge(v)),
      size: const Size(900, 600),
      dpr: 1,
    );

    testWidgets('says how full it is, and of what when asked', (t) async {
      await show(
        t,
        const ContextView(
          share: .71,
          used: 142000,
          window: 200000,
          provenance: Provenance.reported,
          parts: [ContextPart('Conversation', 142000)],
        ),
      );
      expect(find.text('71%'), findsOneWidget);
      expect(find.text('142k of 200k'), findsOneWidget);
      await t.tap(find.byType(ContextGauge));
      await t.pumpAndSettle();
      expect(find.text('Conversation'), findsOneWidget);
      expect(find.text('58k'), findsOneWidget, reason: 'what is free');
    });

    testWidgets('its level has the hue of its service', (t) async {
      const hue = Color(0xFF123456);
      await mount(
        t,
        const Align(
          alignment: Alignment.bottomLeft,
          child: ContextGauge(ContextView(share: .4, provenance: Provenance.reported), hue: hue),
        ),
        dpr: 1,
      );
      expect(t.widget<AxMeter>(find.byType(AxMeter)).color, hue);
    });

    testWidgets('an unknown level is not a number', (t) async {
      await show(t, const ContextView(share: .4, provenance: Provenance.unknown));
      expect(find.text('not reported'), findsOneWidget);
      expect(find.textContaining('%'), findsNothing);
    });

    testWidgets('an estimate is called one', (t) async {
      final h = t.ensureSemantics();
      await show(t, const ContextView(share: .4, provenance: Provenance.estimated));
      expect(find.bySemanticsLabel('Context, about 40%, estimated'), findsOneWidget);
      expect(find.text('about 40%'), findsOneWidget);
      await show(
        t,
        const ContextView(share: .4, used: 80000, window: 200000, provenance: Provenance.estimated),
      );
      expect(find.text('about 80k of 200k'), findsOneWidget);
      await show(t, const ContextView(share: .4, provenance: Provenance.reported));
      expect(
        find.bySemanticsLabel('Context, 40%, reported by the provider'),
        findsOneWidget,
        reason: 'where it is from is said',
      );
      h.dispose();
    });

    testWidgets('figures that cannot be are not shown as they are', (t) async {
      for (final (share, want) in [
        (1.2, '100%'),
        (-.1, '0%'),
        (double.nan, 'not reported'),
        (double.infinity, 'not reported'),
      ]) {
        await show(
          t,
          ContextView(
            share: share,
            used: 240000,
            window: 200000,
            provenance: Provenance.reported,
            note: 'n',
          ),
        );
        expect(find.text(want), findsOneWidget);
        await t.tap(find.byType(ContextGauge));
        await t.pumpAndSettle();
        expect(find.textContaining('-'), findsNothing);
        await t.sendKeyEvent(LogicalKeyboardKey.escape);
        await t.pumpAndSettle();
      }
    });

    testWidgets('what there is no room for is left out, the figure last', (t) async {
      for (final scale in [1.0, 2.0]) {
        for (var w = 60.0; w < 560; w += 20) {
          await mount(
            t,
            Align(
              alignment: Alignment.bottomLeft,
              child: SizedBox(
                width: w,
                child: const ContextGauge(
                  ContextView(
                    share: .71,
                    used: 142000,
                    window: 200000,
                    provenance: Provenance.estimated,
                    note: 'n',
                  ),
                ),
              ),
            ),
            size: const Size(600, 200),
            dpr: 1,
            textScale: scale,
          );
          expect(t.takeException(), isNull, reason: '$w at $scale');
          expect(find.textContaining('71%'), findsOneWidget);
          if (w > 500) expect(find.text('Context'), findsOneWidget);
        }
      }
    });

    testWidgets('close to the end it is marked', (t) async {
      await show(t, const ContextView(share: .9, provenance: Provenance.reported));
      expect(t.widget<Text>(find.text('90%')).style!.color, AxColors.dark.attentionText);
    });

    test('figures', () {
      expect(
        [
          for (final n in [0, 940, 1200, 9960, 142000, 999900, 1000000, 1050000]) tokens(n),
        ],
        ['0', '940', '1.2k', '10k', '142k', '1M', '1M', '1.1M'],
      );
    });
  });

  testWidgets('a long name leaves the views their place, at large text', (t) async {
    await mount(
      t,
      Align(
        alignment: Alignment.topLeft,
        child: AgentBar(
          name: 'An agent with a very long name indeed, longer than anyone needs',
          state: AgentState.running,
          mark: ServiceMark.claude,
          tabs: const [('Conversation', null), ('Changes', '3'), ('Terminal', null)],
          trailing: const Text('Opus 5.5   replay', maxLines: 1, overflow: TextOverflow.ellipsis),
        ),
      ),
      size: const Size(760, 300),
      dpr: 1,
      textScale: 1.5,
    );
    expect(t.takeException(), isNull);
    final name = t.getRect(find.textContaining('An agent'));
    expect(name.right, lessThan(t.getRect(find.text('Conversation').last).left));
    expect(t.getRect(find.text('Terminal').last).right, lessThanOrEqualTo(760));
  });

  testWidgets('the bar fits at any width, with everything in it', (t) async {
    for (final scale in [1.0, 1.5, 2.0]) {
      for (final name in ['Builder', 'An agent with quite a long name']) {
        for (var w = 320.0; w < 1300; w += 60) {
          await mount(
            t,
            Align(
              alignment: Alignment.topLeft,
              child: SizedBox(
                width: w,
                child: AgentBar(
                  name: name,
                  state: AgentState.running,
                  mark: ServiceMark.claude,
                  tabs: const [('Conversation', null), ('Changes', '3'), ('Terminal', null)],
                  trailing: const ContextGauge(
                    ContextView(
                      share: .71,
                      used: 142000,
                      window: 200000,
                      provenance: Provenance.reported,
                    ),
                  ),
                ),
              ),
            ),
            size: const Size(1300, 200),
            dpr: 1,
            textScale: scale,
          );
          expect(t.takeException(), isNull, reason: '$name, $w at $scale');
        }
      }
    }
  });

  testWidgets('its views can be chosen', (t) async {
    final got = <int>[];
    await mount(
      t,
      Align(
        alignment: Alignment.topLeft,
        child: AgentBar(
          name: 'Builder',
          state: AgentState.running,
          tabs: const [('Conversation', null), ('Changes', '3')],
          onTab: got.add,
        ),
      ),
      dpr: 1,
    );
    await t.tap(find.text('Changes').last);
    expect(got, [1]);
    expect(find.byType(AxLamp), findsOneWidget);
  });

  testWidgets('the name and the views share a line', (t) async {
    await mount(
      t,
      const Align(
        alignment: Alignment.topLeft,
        child: AgentBar(
          name: 'Builder',
          state: AgentState.running,
          mark: ServiceMark.openai,
          tabs: [('Conversation', null)],
        ),
      ),
      dpr: 1,
    );
    final a = t.getCenter(find.text('Builder')).dy;
    final b = t.getCenter(find.text('Conversation').last).dy;
    expect((a - b).abs(), lessThan(.6));
  });
}
