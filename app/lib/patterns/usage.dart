import 'package:flutter/widgets.dart';

import '../ui/icons.dart';
import '../ui/marks.dart';
import '../ui/overlay.dart';
import '../ui/press.dart';
import '../ui/status.dart';
import '../ui/surface.dart';
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
    final hue = service.mark?.color(c);
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
            color: open || s.hovered ? c.tinted(hue) : null,
            borderRadius: BorderRadius.circular(AxRadius.key),
          ),
          child: ConstrainedBox(
            constraints: BoxConstraints(minHeight: t.density.control, maxWidth: 220),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 8),
              child: LayoutBuilder(
                builder: (context, box) {
                  final room = box.maxWidth >= 68;
                  return Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      if (service.mark case final m?) ...[
                        AxServiceMark(m, size: 15),
                        const SizedBox(width: 8),
                      ],
                      if (room)
                        AxRing(
                          label: '${service.name}, ${acc.name}',
                          value: tight?.used,
                          provenance: tight?.provenance ?? Provenance.unknown,
                          color: hue,
                        ),
                      if (room) const SizedBox(width: 7),
                      Flexible(
                        child: Text(
                          tight?.used == null ? '–' : tight!.text,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: t.type.label.ink(open || s.hovered ? c.ink1 : c.ink2),
                        ),
                      ),
                    ],
                  );
                },
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
              hue: hue,
              active: a.id == service.active,
              onUse: () {
                close();
                onUse(a.id);
              },
            ),
          ],
          Padding(padding: const EdgeInsets.fromLTRB(8, 8, 8, 4), child: const Hairline()),
          Pressable(
            label: 'Link another ${service.name} account',
            onPressed: () {
              close();
              onLink();
            },
            builder: (_, s) => DecoratedBox(
              decoration: BoxDecoration(
                color: s.hovered || s.focused ? c.tinted(hue) : null,
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
                        child: AxGlyph(AxIcon.plus, size: 12, color: hue ?? c.ink3),
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
  const _Account(this.a, {required this.active, required this.onUse, this.hue});
  final Color? hue;
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
          color: s.hovered || s.focused
              ? c.tinted(hue)
              : active && hue != null
              ? hue!.withValues(alpha: .07)
              : null,
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
                        child: AxGlyph(AxIcon.check, size: 13, color: hue ?? c.liveText),
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
                      AxMeter(
                        label: w.name,
                        value: w.used,
                        provenance: w.provenance,
                        thickness: 3,
                        color: hue,
                      ),
                      if (w.provenance != Provenance.reported) ...[
                        const SizedBox(height: 2),
                        Text(
                          '${w.provenance.words[0].toUpperCase()}${w.provenance.words.substring(1)}',
                          style: t.type.caption.ink(c.ink3),
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
