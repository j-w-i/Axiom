import 'package:axiom/patterns/agent_bar.dart';
import 'package:axiom/patterns/context_gauge.dart';
import 'package:axiom/patterns/execution.dart';
import 'package:axiom/ui/marks.dart';
import 'package:axiom/ui/overlay.dart';
import 'package:axiom/ui/status.dart';
import 'package:axiom/ui/tokens.dart';
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
  final value = {'Model': 'a', 'Harness': 'axiom', 'Effort': 'high'};
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
  Widget build(BuildContext context) => ExecutionKey([
    choice('Model', const [
      AxMenuItem('a', 'Alpha', mark: ServiceMark.claude),
      AxMenuItem('b', 'Beta', mark: ServiceMark.openai),
      AxMenuItem('c', 'Gamma', enabled: false, why: 'signed out'),
    ], (_) => 'Read again: about 142k tokens.'),
    choice('Harness', const [AxMenuItem('axiom', 'Axiom')]),
    choice('Effort', const [AxMenuItem('high', 'High'), AxMenuItem('low', 'Low')]),
  ]);
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

  testWidgets('by keyboard: the question takes the focus, on the safe answer', (t) async {
    final o = await open(t);
    await key(t, LogicalKeyboardKey.enter); // Model
    await key(t, LogicalKeyboardKey.arrowDown); // Alpha
    await key(t, LogicalKeyboardKey.arrowDown); // Beta
    await key(t, LogicalKeyboardKey.enter);
    expect(find.text('Read again: about 142k tokens.'), findsOneWidget);
    await key(t, LogicalKeyboardKey.enter);
    expect(o.changes, isEmpty, reason: 'Enter twice keeps the model');
    // The focus is back on the part, and the same way leads through.
    await key(t, LogicalKeyboardKey.enter);
    await key(t, LogicalKeyboardKey.arrowDown);
    await key(t, LogicalKeyboardKey.arrowDown);
    await key(t, LogicalKeyboardKey.enter);
    await key(t, LogicalKeyboardKey.tab);
    await key(t, LogicalKeyboardKey.enter);
    expect(o.changes, ['Model b']);
    await key(t, LogicalKeyboardKey.escape);
    expect(find.byType(ExecutionPanel), findsNothing);
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
    await t.ensureVisible(find.text('Beta'));
    await t.tap(find.text('Beta'));
    await t.pumpAndSettle();
    final r = t.getRect(find.byType(ExecutionPanel));
    expect(r.left, greaterThanOrEqualTo(0));
    expect(r.right, lessThanOrEqualTo(400));
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

    testWidgets('an unknown level is not a number', (t) async {
      await show(t, const ContextView(share: .4, provenance: Provenance.unknown));
      expect(find.text('not reported'), findsOneWidget);
      expect(find.textContaining('%'), findsNothing);
    });

    testWidgets('an estimate is called one', (t) async {
      final h = t.ensureSemantics();
      await show(t, const ContextView(share: .4, provenance: Provenance.estimated));
      expect(find.bySemanticsLabel('Context, about 40%'), findsOneWidget);
      h.dispose();
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
