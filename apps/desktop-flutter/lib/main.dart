import 'package:flutter/widgets.dart';

/// Axiom's desktop shell (ADR-0020). FL-ARCH-001 is the empty window; the shell arrives in
/// FL-SHELL-001.
void main() {
  runApp(const AxiomApp());
}

class AxiomApp extends StatelessWidget {
  const AxiomApp({super.key});

  @override
  Widget build(BuildContext context) {
    return WidgetsApp(title: 'Axiom', color: Color(0xFF0E0F11), builder: _empty);
  }
}

Widget _empty(BuildContext context, Widget? child) =>
    const ColoredBox(color: Color(0xFF0E0F11), child: SizedBox.expand());
