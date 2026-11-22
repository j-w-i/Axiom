// The Inspector's wording (M5-UI-001): exact sources open at line and column, guesses say they
// are uncertain; nodes and verdicts in words.

import 'package:axiom_desktop/daemon/protocol.g.dart';
import 'package:axiom_desktop/state/inspector.dart';
import 'package:flutter_test/flutter_test.dart';

SourceCandidate cand(
  String file,
  int line,
  int? col, {
  MappingMethod method = MappingMethod.buildInstrumentation,
  SourceRelation relation = SourceRelation.instantiation,
  double confidence = 1,
  bool? generated,
}) => SourceCandidate(
  location: Location(file: file, line: line, column: col),
  method: method,
  relation: relation,
  confidence: confidence,
  generated: generated,
);

void main() {
  test('a build stamp is exact; an owner guess is uncertain with its confidence', () {
    final exact = cand('src/App.jsx', 11, 9);
    expect(isExact(exact), isTrue);
    expect(candidateLabel(exact), 'src/App.jsx:11:9');

    final owner = cand(
      'src/App.jsx',
      7,
      5,
      relation: SourceRelation.owner,
      method: MappingMethod.runtimeMetadata,
      confidence: 0.5,
    );
    expect(isExact(owner), isFalse);
    expect(candidateLabel(owner), 'Uncertain: rendered by src/App.jsx:7:5 (50% confident)');

    // A stamp in generated code is never presented as the place to edit.
    expect(isExact(cand('dist/x.js', 1, 1, generated: true)), isFalse);
    expect(locationLabel(const Location(file: 'a.js', line: 3)), 'a.js:3');
  });

  test('a node in a line: type, name, text (shortened)', () {
    UiNode n({String? name, String? text}) => UiNode(
      id: 'n1',
      surface: 'page',
      typeName: 'button',
      kind: NodeKind.element,
      visibility: const Visibility(visible: true),
      name: name,
      text: text,
    );
    expect(nodeLabel(n(text: 'Pay now')), 'button "Pay now"');
    expect(nodeLabel(n(name: 'pay')), 'button#pay');
    expect(nodeLabel(n(text: 'x' * 50)), 'button "${'x' * 40}…"');
    expect(propertyText('24px'), '24px');
    expect(propertyText(16), '16');
    expect(verdictLabel(Verdict.inconclusive), 'Inconclusive');
  });
}
