import 'package:flutter/widgets.dart';

import '../ui/icons.dart';
import '../ui/overlay.dart';
import '../ui/press.dart';
import '../ui/status.dart';
import '../ui/tokens.dart';
import 'models.dart';

/// A service in the header: the active account's tightest limit, opening every linked account.
/// Axiom never moves work between accounts by itself (docs/research/subscription-auth.md).
class UsageKey extends StatelessWidget {
  const UsageKey(this.service, {required this.onUse, required this.onLink, super.key});
  final ServiceView service;
  final ValueChanged<String> onUse;
  final VoidCallback onLink;

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final acc = service.accounts.firstWhere((a) => a.id == service.active);
    final known = acc.windows.where((w) => w.used != null).toList()
      ..sort((a, b) => b.used!.compareTo(a.used!));
    final tight = known.firstOrNull ?? acc.windows.firstOrNull;
    return AxPopover(
      width: 312,
      alignEnd: true,
      anchor: (context, open, toggle) => Pressable(
        label: '${service.name} usage, ${acc.name}',
        expanded: open,
        onPressed: toggle,
        builder: (context, s) => DecoratedBox(
          decoration: BoxDecoration(
            color: open || s.hovered ? c.hover : null,
            borderRadius: BorderRadius.circular(AxRadius.key),
          ),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 8),
            child: ConstrainedBox(
              constraints: const BoxConstraints(minWidth: 112, maxWidth: 190, minHeight: 30),
              child: IntrinsicWidth(
                child: Stack(
                  alignment: Alignment.center,
                  children: [
                    Row(
                      children: [
                        Text(service.name, style: t.type.small.ink(c.ink2)),
                        const SizedBox(width: 4),
                        AxGlyph(AxIcon.down, size: 8, color: open || s.hovered ? c.ink2 : c.ink4),
                        const SizedBox(width: 8),
                        Expanded(
                          child: Text(
                            tight?.text ?? '–',
                            maxLines: 1,
                            textAlign: TextAlign.end,
                            overflow: TextOverflow.ellipsis,
                            style: t.type.small.ink(c.ink3),
                          ),
                        ),
                      ],
                    ),
                    Positioned(
                      left: 0,
                      right: 0,
                      bottom: 3,
                      child: AxMeter(
                        label: '${service.name}, ${acc.name}',
                        value: tight?.used,
                        provenance: tight?.provenance ?? Provenance.unknown,
                        thickness: 2,
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
      panel: (context, close) => Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final (i, a) in service.accounts.indexed) ...[
            if (i > 0) const SizedBox(height: 6),
            _Account(
              a,
              active: a.id == service.active,
              onUse: () {
                close();
                onUse(a.id);
              },
            ),
          ],
          Padding(
            padding: const EdgeInsets.fromLTRB(8, 8, 8, 4),
            child: ColoredBox(color: c.bayRule, child: const SizedBox(height: 1)),
          ),
          Pressable(
            label: 'Link another ${service.name} account',
            onPressed: () {
              close();
              onLink();
            },
            builder: (_, s) => DecoratedBox(
              decoration: BoxDecoration(
                color: s.hovered || s.focused ? c.hover : null,
                borderRadius: BorderRadius.circular(AxRadius.key),
              ),
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 5),
                child: Row(
                  children: [
                    SizedBox(
                      width: 20,
                      child: Align(
                        alignment: Alignment.centerLeft,
                        child: AxGlyph(AxIcon.plus, size: 12, color: c.ink3),
                      ),
                    ),
                    Expanded(
                      child: Text(
                        'Link another ${service.name} account',
                        style: t.type.ui.ink(c.ink2),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _Account extends StatelessWidget {
  const _Account(this.a, {required this.active, required this.onUse});
  final AccountView a;
  final bool active;
  final VoidCallback onUse;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Pressable(
      label: '${a.name}, ${a.lane}, ${a.identity}',
      selected: active,
      hint: active ? 'In use' : 'Use for new agents',
      onPressed: onUse,
      builder: (context, s) => DecoratedBox(
        decoration: BoxDecoration(
          color: s.hovered || s.focused ? c.hover : null,
          borderRadius: BorderRadius.circular(AxRadius.key),
        ),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(8, 7, 8, 8),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              SizedBox(
                width: 20,
                height: 18,
                child: active
                    ? Align(
                        alignment: Alignment.centerLeft,
                        child: AxGlyph(AxIcon.check, size: 12, color: c.liveText),
                      )
                    : null,
              ),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Text(
                      a.name,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: t.type.uiStrong,
                    ),
                    Text(
                      '${a.lane}   ${a.identity}',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: t.type.small.ink(c.ink3),
                    ),
                    if (a.problem case final p?) ...[
                      const SizedBox(height: 4),
                      Text(p, style: t.type.small.ink(c.attentionText)),
                    ],
                    for (final w in a.windows) ...[
                      const SizedBox(height: 7),
                      Row(
                        children: [
                          Text(w.name, style: t.type.small.ink(c.ink2)),
                          const Spacer(),
                          Text(
                            [w.text, if (w.resets != null) 'resets ${w.resets}'].join(', '),
                            style: t.type.small.ink(c.ink3),
                          ),
                        ],
                      ),
                      const SizedBox(height: 3),
                      AxMeter(label: w.name, value: w.used, provenance: w.provenance, thickness: 3),
                      if (w.provenance != Provenance.reported) ...[
                        const SizedBox(height: 2),
                        Text(
                          '${w.provenance.words[0].toUpperCase()}${w.provenance.words.substring(1)}',
                          style: t.type.caption.ink(c.ink4),
                        ),
                      ],
                    ],
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
