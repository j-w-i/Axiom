import 'package:flutter/widgets.dart';

import '../patterns/agent_bar.dart';
import '../patterns/beside.dart';
import '../patterns/composer.dart';
import '../patterns/context_gauge.dart';
import '../patterns/execution.dart';
import '../patterns/models.dart';
import '../patterns/session_panel.dart';
import '../patterns/transcript_parts.dart';
import '../transcript/transcript_view.dart';
import '../patterns/usage.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/marks.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';
import 'specimen.dart' as s;

/// The components at work together, on specimen content. It proves the system, not the product.
class Workbench extends StatefulWidget {
  const Workbench({super.key, this.titleInset = 78, this.panel = true, this.terminal = false});
  final double titleInset;
  final bool panel, terminal;
  @override
  State<Workbench> createState() => _WorkbenchState();
}

class _WorkbenchState extends State<Workbench> {
  final _transcript = s.transcript();
  String _agent = 'builder';
  late bool _panel = widget.panel, _terminal = widget.terminal;
  final _opened = <String>{};
  var _sizes = Beside.usual;
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
                  width: box.maxWidth >= 1080 ? 264 : 228,
                  child: FocusTraversalGroup(
                    child: _Side(
                      inset: widget.titleInset,
                      selected: _agent,
                      onSelect: (id) => setState(() => _agent = id),
                      active: _active,
                      onUse: (name, id) => setState(() => _active[name] = id),
                    ),
                  ),
                ),
              Expanded(
                child: FocusTraversalGroup(
                  child: Padding(
                    padding: EdgeInsets.fromLTRB(wide ? 0 : 8, 8, 8, 8),
                    child: AxCanvas(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          _bar(agent, mark, wide: wide),
                          Expanded(
                            child: LayoutBuilder(
                              builder: (context, room) => Beside(
                                main: Column(
                                  crossAxisAlignment: CrossAxisAlignment.stretch,
                                  children: [
                                    Expanded(child: _talk(choices, agent.name, agent.id)),
                                    if (_terminal) _drawer(context),
                                  ],
                                ),
                                panel: _panel && Beside.fits(room.maxWidth) ? _side() : null,
                                wide: _opened.isNotEmpty,
                                sizes: _sizes,
                                onSizes: (v) => setState(() => _sizes = v),
                              ),
                            ),
                          ),
                        ],
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

  Widget _bar(AgentView agent, ServiceMark? mark, {required bool wide}) => AgentBar(
    name: agent.name,
    state: agent.state,
    mark: mark,
    inset: wide ? 20 : widget.titleInset + 12,
    trailing: Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Flexible(
          child: AxButton(label: 'Stop', icon: AxIcon.stop, iconFilled: true, onPressed: () {}),
        ),
        const SizedBox(width: 6),
        ChangesKey(
          files: 3,
          open: _panel,
          figure: _panel ? null : const ChangedBy(57, 2),
          onPressed: () => setState(() => _panel = !_panel),
        ),
      ],
    ),
  );

  Widget _side() {
    final t = Ax.of(context);
    return ListView(
      padding: const EdgeInsets.fromLTRB(6, 0, 14, 16),
      children: [
        PlanView(s.session.tasks),
        const PanelHeading('3 changed files', ChangedBy(57, 2)),
        for (final (i, (path, a, r)) in s.changed.indexed)
          CardPart(
            first: i == 0,
            last: i == s.changed.length - 1,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 6),
                  child: ChangedRow(
                    path,
                    added: a,
                    removed: r,
                    open: _opened.contains(path),
                    onPressed: () => setState(
                      () => _opened.contains(path) ? _opened.remove(path) : _opened.add(path),
                    ),
                  ),
                ),
                if (_opened.contains(path))
                  const Padding(
                    padding: EdgeInsets.fromLTRB(10, 0, 10, 10),
                    child: DiffLines(s.diff),
                  ),
                if (i < s.changed.length - 1)
                  const Padding(padding: EdgeInsets.symmetric(horizontal: 12), child: Hairline()),
              ],
            ),
          ),
        const SizedBox(height: 6),
        PanelGroup(
          'Terminals',
          AxButton.icon(
            AxIcon.plus,
            label: 'New terminal',
            tone: AxTone.quiet,
            size: 26,
            onPressed: () {},
          ),
          [
            PanelRow(
              AxIcon.terminal,
              'pnpm test auth',
              state: 'running',
              open: _terminal,
              onPressed: () => setState(() => _terminal = !_terminal),
            ),
            PanelRow(AxIcon.terminal, 'pnpm dev', state: ':5173', onPressed: () {}),
          ],
          acts: true,
        ),
        SizedBox(height: t.density.row / 4),
      ],
    );
  }

  Widget _drawer(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Padding(
      padding: const EdgeInsets.fromLTRB(12, 0, 4, 12),
      child: Surface(
        c.block,
        radius: AxRadius.card,
        child: SizedBox(
          height: 180,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(14, 6, 6, 2),
                child: Row(
                  children: [
                    AxGlyph(AxIcon.terminal, color: c.ink3),
                    const SizedBox(width: 9),
                    Expanded(child: Text('pnpm test auth', style: t.type.code.ink(c.ink1))),
                    AxButton.icon(
                      AxIcon.close,
                      label: 'Close the terminal',
                      tone: AxTone.quiet,
                      size: 26,
                      onPressed: () => setState(() => _terminal = false),
                    ),
                  ],
                ),
              ),
              Expanded(
                child: SingleChildScrollView(
                  padding: const EdgeInsets.fromLTRB(14, 2, 14, 10),
                  child: Text(
                    '  PASS  src/auth/refresh.test.ts (6)\n'
                    '  PASS  src/auth/inflight.test.ts (4)\n'
                    '  Tests 10 passed\n\n'
                    '  Waiting for file changes',
                    style: t.type.code.ink(c.ink2),
                  ),
                ),
              ),
            ],
          ),
        ),
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
                const SizedBox(height: 8),
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
  const _Side({
    required this.inset,
    required this.selected,
    required this.onSelect,
    required this.active,
    required this.onUse,
  });
  final Map<String, String> active;
  final void Function(String service, String account) onUse;
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
          height: 58,
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
          padding: const EdgeInsets.fromLTRB(10, 4, 10, 10),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              // Limits are an account's, and stand with the accounts.
              Row(
                children: [
                  for (final v in s.services)
                    Flexible(
                      child: UsageKey(
                        s.ServiceViewX(v).withActive(active[v.name]!),
                        onUse: (id) => onUse(v.name, id),
                        onLink: () {},
                      ),
                    ),
                ],
              ),
              const SizedBox(height: 2),
              NavRow(AxIcon.account, 'Accounts', keys: '⌘,', onPressed: () {}),
            ],
          ),
        ),
      ],
    );
  }
}
