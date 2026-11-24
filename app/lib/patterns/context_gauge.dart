import 'package:flutter/widgets.dart';

import '../ui/overlay.dart';
import '../ui/press.dart';
import '../ui/status.dart';
import '../ui/tokens.dart';

/// What fills the context window, in tokens.
class ContextPart {
  const ContextPart(this.name, this.tokens);
  final String name;
  final int tokens;
}

class ContextView {
  const ContextView({
    required this.share,
    required this.provenance,
    this.used,
    this.window,
    this.parts = const [],
    this.note,
  });

  /// 0–1, or null when the harness does not say.
  final double? share;
  final Provenance provenance;
  final int? used, window;
  final List<ContextPart> parts;

  /// What happens at the end: "Summarised at 85%", for example.
  final String? note;
}

/// A count of tokens to two or three figures. The bounds are where rounding would carry over.
String tokens(int n) {
  if (n < 1000) return '$n';
  if (n < 9950) return '${(n / 1000).toStringAsFixed(1)}k';
  if (n < 999500) return '${(n / 1000).round()}k';
  final m = (n / 1000000).toStringAsFixed(1);
  return '${m.endsWith('.0') ? m.substring(0, m.length - 2) : m}M';
}

/// How full the context window is: a level, a figure and, when opened, what fills it.
class ContextGauge extends StatelessWidget {
  const ContextGauge(this.view, {super.key, this.hue});
  final ContextView view;
  final Color? hue;

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors, v = view;
    final share = v.provenance == Provenance.unknown ? null : v.share;
    final high = share != null && share >= .85;
    final figure = share == null
        ? 'not reported'
        : v.used != null && v.window != null
        ? '${tokens(v.used!)} of ${tokens(v.window!)}'
        : '${v.provenance == Provenance.estimated ? 'about ' : ''}${(share * 100).round()}%';
    final face = Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Text('Context', style: t.type.label.ink(c.ink3)),
        const SizedBox(width: 9),
        SizedBox(
          width: 92,
          child: AxMeter(
            label: 'Context used',
            value: v.share,
            provenance: v.provenance,
            thickness: 5,
            color: hue,
          ),
        ),
        const SizedBox(width: 9),
        Text(
          share == null ? figure : '${(share * 100).round()}%',
          style: t.type.labelStrong.ink(high ? c.attentionText : c.ink1),
        ),
        if (share != null && v.used != null && v.window != null) ...[
          const SizedBox(width: 8),
          Text(figure, style: t.type.label.ink(c.ink3)),
        ],
      ],
    );
    if (v.parts.isEmpty && v.note == null) {
      return Semantics(
        label: 'Context, $figure',
        excludeSemantics: true,
        child: Padding(padding: const EdgeInsets.symmetric(horizontal: 8), child: face),
      );
    }
    final free = v.window == null || v.used == null ? null : v.window! - v.used!;
    return AxPopover(
      width: 300,
      anchor: (context, open, toggle) => Pressable(
        label: 'Context, $figure, ${v.provenance.words}',
        expanded: open,
        onPressed: toggle,
        builder: (_, s) => DecoratedBox(
          decoration: BoxDecoration(
            color: open || s.hovered ? c.tinted(hue) : null,
            borderRadius: BorderRadius.circular(AxRadius.key),
          ),
          child: ConstrainedBox(
            constraints: BoxConstraints(minHeight: t.density.control),
            child: Padding(padding: const EdgeInsets.symmetric(horizontal: 8), child: face),
          ),
        ),
      ),
      panel: (context, close) => Focus(
        autofocus: true,
        child: Padding(
          padding: const EdgeInsets.fromLTRB(8, 6, 8, 8),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              for (final p in [...v.parts, if (free != null) ContextPart('Free', free)])
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: 3),
                  child: Row(
                    children: [
                      Expanded(
                        child: Text(
                          p.name,
                          style: t.type.ui.ink(p.name == 'Free' ? c.ink3 : c.ink1),
                        ),
                      ),
                      Text(tokens(p.tokens), style: t.type.label.ink(c.ink3)),
                    ],
                  ),
                ),
              const SizedBox(height: 6),
              Text(
                [
                  '${v.provenance.words[0].toUpperCase()}${v.provenance.words.substring(1)}.',
                  ?v.note,
                ].join(' '),
                style: t.type.small.ink(c.ink3),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
