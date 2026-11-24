import 'dart:async';

import 'package:flutter/widgets.dart';

import '../host/host_store.dart';
import '../host/proto.dart';
import '../host/session_model.dart';
import '../patterns/agent_bar.dart';
import '../patterns/context_gauge.dart';
import '../patterns/models.dart';
import '../patterns/session_panel.dart';
import '../transcript/safe_text.dart';
import '../transcript/transcript_view.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/marks.dart';
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
  String? _session;

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Surface(
      c.chassis,
      radius: 0,
      child: ListenableBuilder(
        listenable: widget.store,
        builder: (context, _) {
          final s = widget.store;
          // An agent is chosen within its session only.
          if (_session != s.current?.id) {
            _session = s.current?.id;
            _agent = null;
          }
          final stale = s.phase != HostPhase.connected;
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
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    SizedBox(
                      height: 44,
                      child: Padding(
                        padding: const EdgeInsets.only(left: 12, right: 20),
                        child: Row(
                          children: [
                            if (stale && s.phase != HostPhase.starting) ...[
                              const AxLamp(AgentState.failed),
                              const SizedBox(width: 10),
                              Expanded(
                                child: Text(
                                  s.binary == null && s.current == null
                                      ? 'axiom-host was not found beside the application'
                                      : 'The host is not answering. Trying again.',
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: t.type.label.ink(c.faultText),
                                ),
                              ),
                            ] else if (s.damaged case final d?) ...[
                              const AxLamp(AgentState.failed),
                              const SizedBox(width: 10),
                              Expanded(
                                child: Text(
                                  'The journal is damaged: ${oneLine(d, 200)}',
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: t.type.label.ink(c.faultText),
                                ),
                              ),
                            ],
                          ],
                        ),
                      ),
                    ),
                    Expanded(
                      child: Padding(
                        padding: const EdgeInsets.only(right: 8, bottom: 8),
                        child: Surface(
                          c.bay,
                          radius: AxRadius.bay,
                          child: switch (s) {
                            HostStore(phase: HostPhase.starting, current: null) => const _Word(
                              'Starting the host',
                            ),
                            HostStore(phase: HostPhase.unreachable, current: null) => _Word(
                              'The host is not answering',
                              detail: s.problem,
                            ),
                            HostStore(current: final m?) => _Display(
                              m,
                              agent: _agent,
                              stale: stale,
                              onAgain: () => unawaited(s.open(m.id).catchError((_) {})),
                            ),
                            _ => _Word(
                              s.sessions.isEmpty ? 'No sessions yet' : 'Choose a session',
                              detail: s.sessions.isEmpty
                                  ? 'This build replays recorded sessions: axiom replay SESSION '
                                        'SCRIPT. Running agents comes with the native engine.'
                                  : null,
                            ),
                          },
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
    model: oneLine(a.execution.model ?? '', 60),
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
                      : () async {
                          try {
                            await store.open(await store.create('New session'));
                          } on Exception {
                            // The connection's loss is shown by the store itself.
                          }
                        },
                ),
              ],
            ),
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
                      selected: m.agents.containsKey(agent) ? agent : m.agents.keys.firstOrNull,
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
                  SessionView(id: s.id, title: oneLine(s.title), age: ageOf(s.updated)),
                  onOpen: () => unawaited(store.open(s.id).catchError((_) {})),
                ),
              if (store.older)
                Padding(
                  padding: const EdgeInsets.fromLTRB(12, 6, 12, 0),
                  child: Text(
                    'The last ${HostStore.listed}. Older ones are in the journal.',
                    style: t.type.small.ink(c.ink3),
                  ),
                ),
            ],
          ),
        ),
      ],
    );
  }
}

class _Display extends StatelessWidget {
  const _Display(this.session, {required this.agent, required this.stale, required this.onAgain});
  final SessionModel session;
  final String? agent;
  final bool stale;
  final VoidCallback onAgain;
  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: session,
    builder: (context, _) {
      final t = Ax.of(context), c = t.colors;
      final a = session.agents[agent] ?? session.agents.values.firstOrNull;
      if (session.fault case final f?) {
        return _Word('This session cannot be shown', detail: f, action: ('Try again', onAgain));
      }
      if (!session.loaded) return _Word(stale ? 'Waiting for the host' : 'Reading the session');
      if (a == null) return const _Word('No agent has worked here yet');
      final x = a.execution;
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          AgentBar(
            name: a.name,
            state: phaseOf(a.phase),
            mark: ServiceMark.of(x.model),
            tabs: const [('Conversation', null)],
            trailing: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Flexible(
                  child: Text(
                    [
                      for (final p in [x.model, x.harness, x.connection])
                        if (p != null) oneLine(p, 60),
                    ].join('   '),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: t.type.label.ink(c.ink3),
                  ),
                ),
                if (a.context != null) ...[
                  const SizedBox(width: 10),
                  ContextGauge(
                    ContextView(share: a.context, provenance: provenanceOf(a.contextKnown)),
                    hue: ServiceMark.of(x.model)?.color(c),
                  ),
                ],
                if (stale) ...[
                  const SizedBox(width: 10),
                  Text('not live', style: t.type.label.ink(c.attentionText)),
                ],
              ],
            ),
          ),
          ColoredBox(color: c.bayRule, child: const SizedBox(height: 1)),
          Expanded(child: TranscriptView(a)),
          if (session.requests.isNotEmpty) _Asked(session),
        ],
      );
    },
  );
}

/// What agents of the session have asked. Nothing here can be answered yet: this build replays.
class _Asked extends StatelessWidget {
  const _Asked(this.session);
  final SessionModel session;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return ConstrainedBox(
      constraints: const BoxConstraints(maxHeight: 180),
      child: ColoredBox(
        color: c.block.bottom,
        child: SingleChildScrollView(
          padding: const EdgeInsets.fromLTRB(24, 10, 24, 12),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              for (final r in session.requests.values)
                Padding(
                  padding: const EdgeInsets.only(bottom: 6),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        '${session.agents[r.opened.agent]?.name ?? 'An agent'} ${literal(r.opened.what)}'
                        '${r.opened.answerable == Answerable.no ? ' (no longer waiting)' : ''}',
                        style: t.type.label.ink(c.attentionText),
                      ),
                      Text(literal(r.opened.subject), style: t.type.code.ink(c.ink1)),
                      Text(literal(r.opened.scope), style: t.type.label.ink(c.ink3)),
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

class _Word extends StatelessWidget {
  const _Word(this.text, {this.detail, this.action});
  final String text;
  final String? detail;
  final (String, VoidCallback)? action;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Center(
      child: SingleChildScrollView(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 380),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(text, textAlign: TextAlign.center, style: t.type.title.ink(c.ink2)),
              if (detail case final d?) ...[
                const SizedBox(height: 8),
                Text(visible(d), textAlign: TextAlign.center, style: t.type.ui.ink(c.ink3)),
              ],
              if (action case (final l, final f)?) ...[
                const SizedBox(height: 14),
                AxButton(label: l, onPressed: f),
              ],
            ],
          ),
        ),
      ),
    );
  }
}
