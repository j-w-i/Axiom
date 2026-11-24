import 'package:flutter/widgets.dart';

import '../patterns/composer.dart';
import '../patterns/session_panel.dart';
import '../patterns/transcript_parts.dart';
import '../patterns/usage.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/status.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';
import 'specimen.dart' as s;

/// The components at work together, on specimen content. It proves the system, not the product.
class Workbench extends StatefulWidget {
  const Workbench({super.key, this.titleInset = 78});
  final double titleInset;
  @override
  State<Workbench> createState() => _WorkbenchState();
}

class _WorkbenchState extends State<Workbench> {
  String _agent = 'builder';
  int _tab = 0;
  bool _request = true;
  final _choice = {
    'Harness': 'codex',
    'Model': 'gpt',
    'Connection': 'o1',
    'Effort': 'high',
    'Access': 'tree',
  };
  final _active = {for (final v in s.services) v.name: v.active};

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final agent = s.agents.firstWhere((a) => a.id == _agent);
    Widget line(bool wide) => ExecutionLine([
      for (final (k, items) in [
        ('Model', s.models),
        ('Harness', s.harnesses),
        ('Connection', s.connections),
        if (wide) ('Effort', s.efforts),
        if (wide) ('Access', s.access),
      ])
        ExecutionChoice(k, items, _choice[k]!, (v) => setState(() => _choice[k] = v)),
    ]);
    return Surface(
      c.chassis,
      radius: 0,
      child: LayoutBuilder(
        builder: (context, box) {
          final wide = box.maxWidth >= 1080;
          return Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (wide)
                SizedBox(
                  width: 264,
                  child: _Side(
                    inset: widget.titleInset,
                    selected: _agent,
                    onSelect: (id) => setState(() => _agent = id),
                  ),
                ),
              Expanded(
                child: Column(
                  children: [
                    SizedBox(
                      height: 44,
                      child: Row(
                        children: [
                          SizedBox(width: wide ? 10 : widget.titleInset),
                          Expanded(
                            child: Row(
                              children: [
                                Text('3 running', style: t.type.label.ink(c.ink3)),
                                const SizedBox(width: 14),
                                const AxLamp(AgentState.asking),
                                const SizedBox(width: 7),
                                Text('1 asking', style: t.type.label.ink(c.attentionText)),
                              ],
                            ),
                          ),
                          if (box.maxWidth >= 900)
                            for (final v in s.services)
                              UsageKey(
                                s.ServiceViewX(v).withActive(_active[v.name]!),
                                onUse: (id) => setState(() => _active[v.name] = id),
                                onLink: () {},
                              ),
                          const SizedBox(width: 4),
                          AxButton.icon(
                            AxIcon.search,
                            label: 'Find or run',
                            shortcut: '⌘K',
                            tone: AxTone.quiet,
                            onPressed: () {},
                          ),
                          const SizedBox(width: 8),
                        ],
                      ),
                    ),
                    Expanded(
                      child: Padding(
                        padding: EdgeInsets.fromLTRB(wide ? 0 : 8, 0, 8, 8),
                        child: Surface(
                          c.bay,
                          radius: AxRadius.bay,
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.stretch,
                            children: [
                              Padding(
                                padding: const EdgeInsets.only(left: 24, right: 14),
                                child: Row(
                                  children: [
                                    Text(agent.name, style: t.type.uiStrong),
                                    const SizedBox(width: 8),
                                    Expanded(child: line(wide)),
                                    AxTabs(
                                      tabs: const [
                                        ('Conversation', null),
                                        ('Changes', '3'),
                                        ('Terminal', null),
                                      ],
                                      index: _tab,
                                      onChanged: (i) => setState(() => _tab = i),
                                    ),
                                  ],
                                ),
                              ),
                              ColoredBox(color: c.bayRule, child: const SizedBox(height: 1)),
                              const Expanded(child: _Folio()),
                              if (_request)
                                Padding(
                                  padding: const EdgeInsets.fromLTRB(12, 0, 12, 12),
                                  child: RequestBar(
                                    s.request,
                                    onOnce: () => setState(() => _request = false),
                                    onAlways: () => setState(() => _request = false),
                                    onDeny: () => setState(() => _request = false),
                                  ),
                                ),
                              Composer(
                                placeholder:
                                    'Steer ${agent.name}. It reads this after the current tool call.',
                                action: 'Queue',
                                onAttach: () {},
                                onSubmit: (_) {},
                              ),
                            ],
                          ),
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ],
          );
        },
      ),
    );
  }
}

class _Side extends StatelessWidget {
  const _Side({required this.inset, required this.selected, required this.onSelect});
  final double inset;
  final String selected;
  final ValueChanged<String> onSelect;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SizedBox(
          height: 44,
          child: Padding(
            padding: EdgeInsets.only(left: inset, right: 10),
            child: Row(
              children: [
                const AxMark(size: 17),
                const SizedBox(width: 9),
                Expanded(child: Text('meridian', style: t.type.uiStrong)),
                AxButton.icon(
                  AxIcon.plus,
                  label: 'New session',
                  tone: AxTone.quiet,
                  onPressed: () {},
                ),
              ],
            ),
          ),
        ),
        Expanded(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(8, 0, 8, 12),
            children: [
              SessionPanel(
                session: s.session,
                selected: selected,
                onSelect: onSelect,
                onAddAgent: () {},
                onPause: (_) {},
                onStop: (_) {},
              ),
              const SizedBox(height: 14),
              Padding(
                padding: const EdgeInsets.fromLTRB(12, 0, 12, 4),
                child: Text('Other sessions', style: t.type.label.ink(c.ink3)),
              ),
              for (final o in s.otherSessions) SessionRow(o, onOpen: () {}),
            ],
          ),
        ),
      ],
    );
  }
}

/// Prose on the left, the work it caused on the right. Narrow displays stack them.
class _Folio extends StatelessWidget {
  const _Folio();
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    Text p(String x, [Color? k]) => Text(x, style: k == null ? t.type.prose : t.type.prose.ink(k));
    final turns = <(Widget, Widget)>[
      (
        p(
          'Two tabs can both see an expired token and rotate it twice. Follow docs/refresh.md: '
          're-check expiry inside the lock and make rotation single-flight per account.',
          c.voice,
        ),
        const WorkLine(
          icon: AxIcon.attach,
          name: 'Attached',
          detail: 'docs/refresh.md',
          result: '84 lines',
        ),
      ),
      (
        p(
          'I will read the refresh path and its callers first, then run the new test to see it fail.',
        ),
        const Column(
          children: [
            WorkLine(
              icon: AxIcon.file,
              name: 'Read',
              detail: 'src/auth/refresh.ts',
              result: '220 lines',
            ),
            WorkLine(
              icon: AxIcon.search,
              name: 'Search',
              detail: 'locks.acquire',
              result: '9 in 4 files',
            ),
            WorkLine(
              icon: AxIcon.terminal,
              name: 'Test',
              detail: 'pnpm test auth',
              result: '2 failed',
              failed: true,
            ),
          ],
        ),
      ),
      (
        p(
          'The expiry check happens before the lock is held, so the second tab acts on a token the '
          'first already replaced. I am moving the read inside the critical section and adding a '
          'single-flight map keyed by account.\n\nBoth writers now go through one entry point. The '
          'store keeps its interface, so nothing outside the auth module changes.',
        ),
        const DiffBlock(path: 'src/auth/refresh.ts', lines: s.diff),
      ),
    ];
    return LayoutBuilder(
      builder: (context, box) {
        final two = box.maxWidth >= 860;
        return Align(
          alignment: Alignment.topCenter,
          child: ListView(
            reverse: true,
            shrinkWrap: true,
            padding: const EdgeInsets.fromLTRB(24, 18, 20, 22),
            children: [
              for (final (l, r) in turns.reversed)
                Padding(
                  padding: const EdgeInsets.only(top: 22),
                  child: two
                      ? Row(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Expanded(flex: 54, child: l),
                            const SizedBox(width: 44),
                            Expanded(flex: 46, child: r),
                          ],
                        )
                      : Column(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [l, const SizedBox(height: 10), r],
                        ),
                ),
            ],
          ),
        );
      },
    );
  }
}
