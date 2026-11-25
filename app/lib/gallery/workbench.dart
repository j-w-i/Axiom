import 'package:flutter/widgets.dart';

import '../patterns/agent_bar.dart';
import '../patterns/composer.dart';
import '../patterns/context_gauge.dart';
import '../patterns/execution.dart';
import '../patterns/session_panel.dart';
import '../patterns/transcript_parts.dart';
import '../transcript/transcript_view.dart';
import '../patterns/usage.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
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
    'Account': 'o1',
    'Effort': 'high',
    'Access': 'tree',
  };
  final _active = {for (final v in s.services) v.name: v.active};

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final agent = s.agents.firstWhere((a) => a.id == _agent);
    final choices = [
      for (final (k, items) in [
        ('Model', _choice['Harness'] == 'axiom' ? s.axiomModels : s.models),
        ('Harness', s.harnessesForGpt),
        ('Account', s.otherConnections[_choice['Model']] ?? s.connections),
        ('Effort', s.efforts),
        ('Access', s.access),
      ])
        ExecutionChoice(
          k,
          items,
          _choice[k]!,
          (v) => setState(() {
            _choice[k] = v;
            // A model is reached by an account of its own service.
            if (k == 'Model') {
              _choice['Account'] = (s.otherConnections[v] ?? s.connections).first.value;
            }
          }),
          cost: switch (k) {
            'Model' =>
              (v) =>
                  'The whole conversation has to be read again: about ${tokens(s.contextView.used!)} '
                  'tokens, none of them from the cache. Reasoning this model kept is not carried over.',
            'Harness' =>
              (v) =>
                  'Another harness cannot take over this one\'s session. It begins with the '
                  'conversation as text: about ${tokens(s.contextView.used!)} tokens to read.',
            _ => null,
          },
        ),
    ];
    final mark = agent.id == 'builder' ? choices.first.item?.mark : agent.mark;
    return ColoredBox(
      color: c.chassis.fill,
      child: LayoutBuilder(
        builder: (context, box) {
          final wide = box.maxWidth >= 860;
          return Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (wide)
                SizedBox(
                  width: box.maxWidth >= 1080 ? 272 : 232,
                  child: FocusTraversalGroup(
                    child: _Side(
                      inset: widget.titleInset,
                      selected: _agent,
                      onSelect: (id) => setState(() => _agent = id),
                    ),
                  ),
                ),
              Expanded(
                child: FocusTraversalGroup(
                  child: Padding(
                    padding: EdgeInsets.fromLTRB(wide ? 0 : 8, 8, 8, 8),
                    child: Surface(
                      c.canvas,
                      radius: AxRadius.canvas,
                      child: ClipPath(
                        clipper: const ContinuousClip(AxRadius.canvas),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            AgentBar(
                              name: agent.name,
                              state: agent.state,
                              mark: mark,
                              inset: wide ? 24 : widget.titleInset + 12,
                              tabs: const [
                                ('Conversation', null),
                                ('Changes', '3'),
                                ('Terminal', null),
                              ],
                              index: _tab,
                              onTab: (i) => setState(() => _tab = i),
                              trailing: Row(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  if (box.maxWidth >= 1080)
                                    for (final v in s.services)
                                      Flexible(
                                        child: UsageKey(
                                          s.ServiceViewX(v).withActive(_active[v.name]!),
                                          onUse: (id) => setState(() => _active[v.name] = id),
                                          onLink: () {},
                                        ),
                                      ),
                                  const SizedBox(width: 8),
                                  Flexible(
                                    child: AxButton(
                                      label: 'Stop',
                                      icon: AxIcon.stop,
                                      onPressed: () {},
                                    ),
                                  ),
                                  const SizedBox(width: 6),
                                  AxButton.icon(
                                    AxIcon.panel,
                                    label: 'Hide the panel',
                                    tone: AxTone.quiet,
                                    onPressed: () {},
                                  ),
                                ],
                              ),
                            ),
                            Expanded(
                              child: Row(
                                crossAxisAlignment: CrossAxisAlignment.stretch,
                                children: [
                                  Expanded(child: _talk(choices, agent.name, agent.id)),
                                  if (box.maxWidth >= 1180)
                                    SidePanel(
                                      groups: [
                                        planGroup(context, s.session.tasks),
                                        (
                                          'Changed files',
                                          const ChangedBy(57, 2),
                                          [
                                            for (final (path, a, r) in const [
                                              ('src/auth/refresh.ts', 7, 2),
                                              ('src/auth/inflight.ts', 31, 0),
                                              ('test/refresh.test.ts', 19, 0),
                                            ])
                                              ChangedRow(path, added: a, removed: r, onOpen: () {}),
                                          ],
                                        ),
                                      ],
                                    ),
                                ],
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ],
          );
        },
      ),
    );
  }

  Widget _talk(List<ExecutionChoice> choices, String name, String id) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      Expanded(child: TranscriptView(_transcript.agents[_agent] ?? _transcript.agents['builder']!)),
      LayoutBuilder(
        builder: (context, box) => Padding(
          padding: EdgeInsets.fromLTRB(
            AxSpace.beside(box.maxWidth),
            0,
            AxSpace.beside(box.maxWidth),
            20,
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (_request) ...[
                RequestBar(
                  s.request,
                  onOnce: () => setState(() => _request = false),
                  onAlways: () => setState(() => _request = false),
                  onDeny: () => setState(() => _request = false),
                ),
                const SizedBox(height: 10),
              ],
              Composer(
                placeholder: 'Message $name',
                action: 'Queue',
                leading: [
                  ExecutionKey(choices, of: id),
                  const ContextGauge(s.contextView),
                ],
                onAttach: () {},
                onSubmit: (_) => true,
              ),
            ],
          ),
        ),
      ),
    ],
  );
}

class _Side extends StatelessWidget {
  const _Side({required this.inset, required this.selected, required this.onSelect});
  final double inset;
  final String selected;
  final ValueChanged<String> onSelect;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SizedBox(
          height: 64,
          child: Padding(
            padding: EdgeInsets.only(top: 8, left: inset, right: 10),
            child: Row(
              children: [
                const AxMark(size: 18),
                const SizedBox(width: 9),
                Expanded(child: Text('Axiom', style: t.type.uiStrong.copyWith(fontSize: 14))),
                AxButton.icon(
                  AxIcon.sidebar,
                  label: 'Hide the sidebar',
                  tone: AxTone.quiet,
                  onPressed: () {},
                ),
              ],
            ),
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(10, 0, 10, 0),
          child: Column(
            children: [
              NavRow(AxIcon.plus, 'New session', keys: '⌘N', act: true, onPressed: () {}),
              const SizedBox(height: 3),
              NavRow(AxIcon.search, 'Search', keys: '⌘K', onPressed: () {}),
            ],
          ),
        ),
        Expanded(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(10, 0, 10, 12),
            children: [
              const SideHeading('Needs you'),
              NeedsRow(
                agent: 'Builder',
                what: 'asks to run a command',
                subject: s.request.subject,
                session: s.session.title,
                age: 'now',
                onOpen: () {},
              ),
              SideHeading(
                'web-store',
                act: AxButton.icon(
                  AxIcon.plus,
                  label: 'New session in web-store',
                  tone: AxTone.quiet,
                  size: 28,
                  onPressed: () {},
                ),
              ),
              SessionPanel(
                session: s.session,
                selected: selected,
                onSelect: onSelect,
                onAddAgent: () {},
                onPause: (_) {},
                onStop: (_) {},
              ),
              for (final o in s.otherSessions) SessionRow(o, onOpen: () {}),
            ],
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(10, 6, 10, 12),
          child: NavRow(AxIcon.settings, 'Accounts', keys: '⌘,', onPressed: () {}),
        ),
      ],
    );
  }
}
