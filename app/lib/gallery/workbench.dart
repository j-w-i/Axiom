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
      color: c.bay.fill,
      child: LayoutBuilder(
        builder: (context, box) {
          final wide = box.maxWidth >= 860;
          return Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (wide) ...[
                SizedBox(
                  width: box.maxWidth >= 1080 ? 260 : 224,
                  child: ColoredBox(
                    color: c.chassis.fill,
                    child: FocusTraversalGroup(
                      child: _Side(
                        inset: widget.titleInset,
                        selected: _agent,
                        onSelect: (id) => setState(() => _agent = id),
                      ),
                    ),
                  ),
                ),
                const Hairline(vertical: true),
              ],
              Expanded(
                child: FocusTraversalGroup(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      SizedBox(
                        height: 44,
                        child: Row(
                          children: [
                            SizedBox(width: wide ? 20 : widget.titleInset + 12),
                            Expanded(
                              flex: 2,
                              child: Text.rich(
                                TextSpan(
                                  children: [
                                    TextSpan(text: 'web-store', style: t.type.ui.ink(c.ink3)),
                                    TextSpan(text: '  /  ', style: t.type.ui.ink(c.ink4)),
                                    TextSpan(text: s.session.title),
                                  ],
                                ),
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: t.type.uiStrong,
                              ),
                            ),
                            if (box.maxWidth >= 1000)
                              Expanded(
                                flex: 3,
                                child: Row(
                                  mainAxisAlignment: MainAxisAlignment.end,
                                  children: [
                                    for (final v in s.services.take(box.maxWidth >= 1300 ? 3 : 2))
                                      Flexible(
                                        child: UsageKey(
                                          s.ServiceViewX(v).withActive(_active[v.name]!),
                                          onUse: (id) => setState(() => _active[v.name] = id),
                                          onLink: () {},
                                        ),
                                      ),
                                  ],
                                ),
                              ),
                            const SizedBox(width: 4),
                            AxButton.icon(
                              AxIcon.panel,
                              label: 'Hide the panel',
                              tone: AxTone.quiet,
                              onPressed: () {},
                            ),
                            const SizedBox(width: 12),
                          ],
                        ),
                      ),
                      AgentBar(
                        name: agent.name,
                        state: agent.state,
                        mark: mark,
                        tabs: const [('Conversation', null), ('Changes', '3'), ('Terminal', null)],
                        index: _tab,
                        onTab: (i) => setState(() => _tab = i),
                        trailing: AxButton(label: 'Stop', icon: AxIcon.stop, onPressed: () {}),
                      ),
                      const Hairline(),
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
            16,
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
                placeholder: 'Message $name. It is sent before the next step.',
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
          height: 44,
          child: Padding(
            padding: EdgeInsets.only(left: inset, right: 8),
            child: Row(
              children: [
                const AxMark(size: 16),
                const SizedBox(width: 8),
                Expanded(child: Text('Axiom', style: t.type.uiStrong)),
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
          padding: const EdgeInsets.fromLTRB(8, 2, 8, 0),
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
            padding: const EdgeInsets.fromLTRB(8, 0, 8, 12),
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
                icon: AxIcon.folder,
                trailing: AxButton.icon(
                  AxIcon.plus,
                  label: 'New session in web-store',
                  tone: AxTone.quiet,
                  size: 24,
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
        const Hairline(),
        Padding(
          padding: const EdgeInsets.all(8),
          child: NavRow(AxIcon.settings, 'Accounts', keys: '⌘,', onPressed: () {}),
        ),
      ],
    );
  }
}
