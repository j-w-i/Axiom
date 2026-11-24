import 'package:flutter/widgets.dart';

import '../patterns/composer.dart';
import '../patterns/session_panel.dart';
import '../patterns/transcript_parts.dart';
import '../transcript/transcript_view.dart';
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
  final _transcript = s.transcript();
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
          final wide = box.maxWidth >= 860;
          return Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (wide)
                SizedBox(
                  width: box.maxWidth >= 1080 ? 264 : 224,
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
                          if (box.maxWidth >= 1000)
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
                                    Expanded(child: line(box.maxWidth >= 1240)),
                                    AxTabs(
                                      tabs: const [
                                        ('Conversation', null),
                                        ('Changes', '3'),
                                        ('Terminal', null),
                                      ],
                                      hue: agent.mark?.color(c),
                                      index: _tab,
                                      onChanged: (i) => setState(() => _tab = i),
                                    ),
                                  ],
                                ),
                              ),
                              ColoredBox(color: c.bayRule, child: const SizedBox(height: 1)),
                              Expanded(
                                child: TranscriptView(
                                  _transcript.agents[_agent] ?? _transcript.agents['builder']!,
                                ),
                              ),
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
            padding: EdgeInsets.only(left: inset, right: 9),
            child: Row(
              children: [
                const AxMark(size: 17),
                const SizedBox(width: 9),
                Text('Axiom', style: t.type.uiStrong),
                const SizedBox(width: 10),
                Expanded(
                  child: Text(
                    'web-store',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: t.type.ui.ink(c.ink3),
                  ),
                ),
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
