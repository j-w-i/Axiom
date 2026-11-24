import 'package:flutter/widgets.dart';

import '../host/host_store.dart';
import '../host/proto.dart';
import '../host/session_model.dart';
import '../patterns/models.dart';
import '../patterns/session_panel.dart';
import '../transcript/transcript_view.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/status.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';

/// The application's window, on what the host says. Nothing here is specimen data.
class Shell extends StatefulWidget {
  const Shell(this.store, {super.key, this.titleInset = 0});
  final HostStore store;

  /// Room for the window's own buttons.
  final double titleInset;
  @override
  State<Shell> createState() => _ShellState();
}

class _ShellState extends State<Shell> {
  String? _agent;

  @override
  Widget build(BuildContext context) {
    final c = Ax.of(context).colors;
    return Surface(
      c.chassis,
      radius: 0,
      child: ListenableBuilder(
        listenable: widget.store,
        builder: (context, _) {
          final s = widget.store;
          return Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              SizedBox(
                width: 264,
                child: _Side(
                  store: s,
                  inset: widget.titleInset,
                  agent: _agent,
                  onAgent: (id) => setState(() => _agent = id),
                ),
              ),
              Expanded(
                child: Padding(
                  padding: const EdgeInsets.fromLTRB(0, 8, 8, 8),
                  child: Surface(
                    c.bay,
                    radius: AxRadius.bay,
                    child: switch (s) {
                      HostStore(phase: HostPhase.starting) => const _Word('Starting the host'),
                      HostStore(phase: HostPhase.unreachable, current: null) => _Unreachable(s),
                      HostStore(current: final m?) => _Display(
                        m,
                        agent: _agent,
                        stale: s.phase != HostPhase.connected,
                      ),
                      _ => _Word(
                        s.sessions.isEmpty ? 'No sessions yet' : 'Choose a session',
                        detail: s.sessions.isEmpty
                            ? 'This build replays recorded sessions: axiom replay SESSION SCRIPT. '
                                  'Running agents comes with the native engine.'
                            : null,
                      ),
                    },
                  ),
                ),
              ),
            ],
          );
        },
      ),
    );
  }
}

AgentState phaseOf(AgentPhase p) => switch (p) {
  AgentPhase.running => AgentState.running,
  AgentPhase.asking => AgentState.asking,
  AgentPhase.paused => AgentState.paused,
  AgentPhase.done => AgentState.done,
  AgentPhase.failed => AgentState.failed,
  AgentPhase.waiting || AgentPhase.unknown => AgentState.waiting,
};

Provenance provenanceOf(Known k) => switch (k) {
  Known.reported => Provenance.reported,
  Known.measured => Provenance.measured,
  Known.estimated => Provenance.estimated,
  Known.unknown => Provenance.unknown,
};

AgentView viewOf(AgentModel a) {
  final last = a.last;
  return AgentView(
    id: a.id,
    name: a.name,
    state: phaseOf(a.phase),
    model: a.execution.model ?? '',
    harness: a.execution.harness,
    connection: a.execution.connection ?? '',
    elapsed: '',
    context: a.context,
    contextProvenance: provenanceOf(a.contextKnown),
    recent: [
      if (last != null)
        ActivityView(last.rich ? AxIcon.think : ToolRow.iconOf(last.name), switch (last.kind) {
          ItemKind.tool => last.name,
          ItemKind.reasoning => 'Thinking',
          ItemKind.prompt => 'Reading the request',
          _ => last.open ? 'Writing' : 'Wrote an answer',
        }, failed: last.outcome == Outcome.failed),
    ],
  );
}

class _Side extends StatelessWidget {
  const _Side({
    required this.store,
    required this.inset,
    required this.agent,
    required this.onAgent,
  });
  final HostStore store;
  final double inset;
  final String? agent;
  final ValueChanged<String> onAgent;

  static String ageOf(int ms) {
    final d = DateTime.now().difference(DateTime.fromMillisecondsSinceEpoch(ms));
    if (d.inMinutes < 1) return 'now';
    if (d.inHours < 1) return '${d.inMinutes}m';
    if (d.inDays < 1) return '${d.inHours}h';
    return '${d.inDays}d';
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final m = store.current;
    final others = store.sessions.where((s) => s.id != m?.id).toList();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SizedBox(
          height: 44,
          child: Padding(
            padding: EdgeInsets.only(left: inset + 20, right: 10),
            child: Row(
              children: [
                const AxMark(size: 17),
                const SizedBox(width: 9),
                Expanded(child: Text('Axiom', style: t.type.uiStrong)),
                AxButton.icon(
                  AxIcon.plus,
                  label: 'New session',
                  tone: AxTone.quiet,
                  onPressed: store.phase != HostPhase.connected
                      ? null
                      : () async => store.open(await store.create('New session')),
                ),
              ],
            ),
          ),
        ),
        if (store.phase == HostPhase.unreachable)
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 0, 12, 10),
            child: Row(
              children: [
                const AxLamp(AgentState.failed, width: 12),
                const SizedBox(width: 10),
                Expanded(
                  child: Text(
                    'The host is not answering. Trying again.',
                    style: t.type.label.ink(c.faultText),
                  ),
                ),
              ],
            ),
          ),
        Expanded(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(8, 0, 8, 12),
            children: [
              if (m != null) ...[
                ListenableBuilder(
                  listenable: m,
                  builder: (_, _) => ListenableBuilder(
                    listenable: Listenable.merge(m.agents.values),
                    builder: (_, _) => SessionPanel(
                      session: SessionView(
                        id: m.id,
                        title: m.title.isEmpty ? 'Session' : m.title,
                        age: '',
                        agents: [for (final a in m.agents.values) viewOf(a)],
                      ),
                      selected: agent ?? m.agents.keys.firstOrNull,
                      onSelect: onAgent,
                    ),
                  ),
                ),
                const SizedBox(height: 14),
              ],
              if (others.isNotEmpty)
                Padding(
                  padding: const EdgeInsets.fromLTRB(12, 0, 12, 4),
                  child: Text(
                    m == null ? 'Sessions' : 'Other sessions',
                    style: t.type.label.ink(c.ink3),
                  ),
                ),
              for (final s in others)
                SessionRow(
                  SessionView(id: s.id, title: s.title, age: ageOf(s.updated)),
                  onOpen: () => store.open(s.id),
                ),
            ],
          ),
        ),
      ],
    );
  }
}

class _Display extends StatelessWidget {
  const _Display(this.session, {required this.agent, required this.stale});
  final SessionModel session;
  final String? agent;
  final bool stale;
  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: session,
    builder: (context, _) {
      final t = Ax.of(context), c = t.colors;
      final a = session.agents[agent] ?? session.agents.values.firstOrNull;
      if (a == null) return const _Word('No agent has worked here yet');
      final x = a.execution;
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          SizedBox(
            height: 36,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 24),
              child: Row(
                children: [
                  Text(a.name, style: t.type.uiStrong),
                  const SizedBox(width: 14),
                  Expanded(
                    child: Text(
                      [
                        ?x.model,
                        if (x.harness == 'replay') 'replay' else x.harness,
                        ?x.connection,
                      ].join('   '),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: t.type.label.ink(c.ink3),
                    ),
                  ),
                  if (stale) Text('not live', style: t.type.label.ink(c.attentionText)),
                ],
              ),
            ),
          ),
          ColoredBox(color: c.bayRule, child: const SizedBox(height: 1)),
          Expanded(child: TranscriptView(a)),
        ],
      );
    },
  );
}

class _Word extends StatelessWidget {
  const _Word(this.text, {this.detail});
  final String text;
  final String? detail;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 380),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(text, style: t.type.title.ink(c.ink2)),
            if (detail case final d?) ...[
              const SizedBox(height: 8),
              Text(d, textAlign: TextAlign.center, style: t.type.ui.ink(c.ink3)),
            ],
          ],
        ),
      ),
    );
  }
}

class _Unreachable extends StatelessWidget {
  const _Unreachable(this.store);
  final HostStore store;
  @override
  Widget build(BuildContext context) => _Word(
    'The host is not answering',
    detail: store.binary == null
        ? 'axiom-host was not found beside the application.'
        : store.problem,
  );
}
