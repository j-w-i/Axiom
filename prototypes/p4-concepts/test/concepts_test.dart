import 'package:flutter_test/flutter_test.dart';
import 'package:p4_concepts/concept_a.dart';
import 'package:p4_concepts/concept_b.dart';
import 'package:p4_concepts/concept_c.dart';
import 'package:p4_concepts/concept_d.dart';
import 'shot.dart';
void main() { testWidgets('a', (t) => shot(t, const ConceptA(), 'a')); testWidgets('b', (t) => shot(t, const ConceptB(), 'b')); testWidgets('c', (t) => shot(t, const ConceptC(), 'c')); testWidgets('d', (t) => shot(t, const ConceptD(), 'd')); testWidgets('d2', (t) => shot(t, const ConceptD(score: true), 'd-score')); }
