import 'package:flutter_test/flutter_test.dart';
import 'package:p4_concepts/concept_e.dart';
import 'shot.dart';
void main() { testWidgets('e', (t) => shot(t, const ConceptE(), 'e')); testWidgets('e2', (t) => shot(t, const ConceptE(score: true), 'e-score')); }
