import 'dart:async';

import 'package:flutter/widgets.dart';

import '../host/proto.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/overlay.dart';
import '../ui/press.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';

/// What becomes of a plan an agent has handed in: accepted as it is, or as its person changes it. Why
/// not is said to the agent, where one writes to it. Accepting it allows what is chosen here, and
/// nothing else.
class PlanBar extends StatefulWidget {
  const PlanBar(this.text, {required this.can, required this.onAccept, super.key});
  final String text;

  /// What the agent may be allowed then.
  final List<Access> can;

  /// Whether it went well. The text is null if it is the plan as it was handed in.
  final Future<bool> Function(Access access, String? text) onAccept;
  @override
  State<PlanBar> createState() => _PlanBarState();
}

class _PlanBarState extends State<PlanBar> {
  late final _text = TextEditingController(text: widget.text)
    ..addListener(() => mounted ? setState(() {}) : null);
  late Access _access = widget.can.contains(Access.ask) ? Access.ask : widget.can.first;
  bool _edits = false, _busy = false;

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  bool get _changed => _edits && _text.text.trim() != widget.text.trim();

  Future<void> _accept() async {
    setState(() => _busy = true);
    await widget.onAccept(_access, _changed ? _text.text.trim() : null);
    if (mounted) setState(() => _busy = false);
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: 'A plan waits for you',
      child: Settle(
        stamp: _edits,
        fresh: true,
        child: Surface(
          c.plate,
          radius: AxRadius.card,
          child: Padding(
            padding: const EdgeInsets.all(AxSpace.card),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              mainAxisSize: MainAxisSize.min,
              children: [
                Row(
                  children: [
                    AxGlyph(AxIcon.list, color: c.ink2),
                    const SizedBox(width: 9),
                    Expanded(
                      child: Text.rich(
                        TextSpan(
                          text: 'Plan ready',
                          style: t.type.uiStrong,
                          children: [
                            TextSpan(
                              text: '   Say below what to change, or accept it.',
                              style: t.type.ui.ink(c.ink3),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ],
                ),
                if (_edits) ...[
                  const SizedBox(height: 10),
                  AxField(
                    label: 'Plan',
                    controller: _text,
                    lines: 8,
                    sunken: true,
                    autofocus: true,
                  ),
                ],
                const SizedBox(height: 10),
                Wrap(
                  alignment: WrapAlignment.end,
                  crossAxisAlignment: WrapCrossAlignment.center,
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    AxButton(
                      label: _edits ? 'Discard edits' : 'Edit plan',
                      tone: AxTone.quiet,
                      onPressed: _busy
                          ? null
                          : () => setState(() {
                              _edits = !_edits;
                              if (!_edits) _text.text = widget.text;
                            }),
                    ),
                    AxSelect<Access>(
                      label: 'Then',
                      items: [
                        for (final (a, words) in const [
                          (Access.ask, 'Ask first'),
                          (Access.work, 'Work without asking'),
                        ])
                          if (widget.can.contains(a)) AxMenuItem(a, words),
                      ],
                      value: _access,
                      onChanged: (v) => setState(() => _access = v),
                    ),
                    AxButton(
                      label: _changed ? 'Accept as edited' : 'Accept',
                      tone: AxTone.primary,
                      onPressed: _busy || _edits && _text.text.trim().isEmpty
                          ? null
                          : () => unawaited(_accept()),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
