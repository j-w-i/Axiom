import 'dart:async';

import 'package:flutter/widgets.dart';

import '../host/host_store.dart';
import '../host/proto.dart';
import '../host/session_model.dart';
import '../patterns/composer.dart';
import '../patterns/context_gauge.dart';
import '../patterns/execution.dart';
import '../patterns/models.dart';
import '../patterns/transcript_parts.dart';
import '../transcript/safe_text.dart';
import '../ui/marks.dart';
import '../ui/overlay.dart';
import '../ui/status.dart';
import '../ui/tokens.dart';
import 'start.dart';

/// What an agent asks, and where it is spoken to. Only for an agent that can act: a replayed one is
/// only shown.
class Talk extends StatefulWidget {
  const Talk(this.store, this.session, this.agent, {required this.live, super.key});
  final HostStore store;
  final SessionModel session;
  final AgentModel agent;

  /// The host answers.
  final bool live;
  @override
  State<Talk> createState() => _TalkState();
}

class _TalkState extends State<Talk> {
  String? _problem;

  /// Answered from here, until the host says it is closed: not answered twice.
  final _answered = <String>{};

  @override
  void didUpdateWidget(Talk old) {
    super.didUpdateWidget(old);
    if (old.agent != widget.agent) _problem = null;
  }

  Future<bool> _do(Future<void> Function() f, {VoidCallback? failed}) async {
    setState(() => _problem = null);
    try {
      await f();
      return true;
    } on Object catch (e) {
      failed?.call();
      if (mounted) setState(() => _problem = wordsOf(e));
      return false;
    }
  }

  void _answer(RequestOpened r, Answer a) {
    setState(() => _answered.add(r.request));
    unawaited(
      _do(
        () => widget.store.answer(widget.session.id, r, a),
        failed: () => _answered.remove(r.request),
      ),
    );
  }

  List<ExecutionChoice> _choices(HostStore s, AgentModel a) {
    final x = a.execution;
    final connection = s.connections.where((c) => c.id == x.connection).firstOrNull;
    final models = s.modelsOf(x.connection ?? '');
    final has = models?.any((m) => m.id == x.model) ?? false;
    Execution with_({String? model, String? connection, String? provider}) => Execution(
      harness: x.harness,
      provider: provider ?? x.provider,
      model: model ?? x.model,
      connection: connection ?? x.connection,
      effort: x.effort,
      environment: x.environment,
    );
    void set({Execution? execution, Access? access}) => unawaited(
      _do(() => s.setAgent(widget.session.id, a.id, execution: execution, access: access)),
    );
    return [
      ExecutionChoice(
        'Model',
        [
          if (!has && x.model != null)
            AxMenuItem(x.model!, oneLine(x.model!, 60), mark: markOf(x.provider, x.model)),
          for (final m in models ?? const <ModelInfo>[])
            AxMenuItem(m.id, oneLine(m.name ?? m.id, 60), mark: ServiceMark.of(m.id)),
        ],
        x.model ?? '',
        (v) => set(execution: with_(model: v)),
        cost: (_) => a.items.isEmpty
            ? null
            : 'The whole conversation has to be read again, none of it from the cache. Reasoning '
                  'this model kept is not carried over.',
      ),
      ExecutionChoice(
        'Harness',
        [AxMenuItem(x.harness, x.harness == 'axiom' ? 'Axiom' : oneLine(x.harness, 40))],
        x.harness,
        (_) {},
      ),
      ExecutionChoice(
        'Account',
        [
          if (connection == null && x.connection != null)
            AxMenuItem(x.connection!, 'Removed', enabled: false, why: 'add it again'),
          for (final c in s.connections)
            AxMenuItem(
              c.id,
              oneLine(c.name, 60),
              detail: providerName(c.provider),
              mark: markOf(c.provider, null),
              enabled: c.provider == x.provider,
              why: c.provider == x.provider ? null : 'another provider: choose its model first',
            ),
        ],
        x.connection ?? '',
        (v) => set(execution: with_(connection: v)),
      ),
      ExecutionChoice(
        'Access',
        [for (final i in accessItems) AxMenuItem(i.value.name, i.label, detail: i.detail)],
        (a.access ?? Access.ask).name,
        (v) => set(access: Access.values.byName(v)),
      ),
    ];
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final s = widget.store, a = widget.agent;
    final asked = [
      for (final r in widget.session.requests.values)
        if (r.opened.answerable == Answerable.live && !_answered.contains(r.opened.request))
          r.opened,
    ];
    final works = a.phase == AgentPhase.running || a.phase == AgentPhase.asking;
    final first = asked.firstOrNull;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        if (first != null && widget.live)
          Padding(
            padding: const EdgeInsets.fromLTRB(12, 0, 12, 12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                if (asked.length > 1)
                  Padding(
                    padding: const EdgeInsets.fromLTRB(12, 0, 12, 6),
                    child: Text(
                      '${asked.length - 1} more after this',
                      style: t.type.label.ink(c.attentionText),
                    ),
                  ),
                RequestBar(
                  key: ValueKey(first.request),
                  RequestView(
                    agent: widget.session.agents[first.agent]?.name ?? 'An agent',
                    what: first.what,
                    subject: first.subject,
                    scope: first.scope,
                    always: 'Allow this for the session',
                  ),
                  onOnce: () => _answer(first, Answer.once),
                  onAlways: () => _answer(first, Answer.session),
                  onDeny: () => _answer(first, Answer.deny),
                ),
              ],
            ),
          ),
        if (_problem ?? a.error case final p?)
          Semantics(
            liveRegion: true,
            child: Padding(
              padding: const EdgeInsets.fromLTRB(24, 0, 24, 10),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const Padding(padding: EdgeInsets.only(top: 3), child: AxLamp(AgentState.failed)),
                  const SizedBox(width: 9),
                  Expanded(
                    child: Text(
                      visible(cutTo(p, 600)),
                      maxLines: 4,
                      overflow: TextOverflow.ellipsis,
                      style: t.type.ui.ink(c.faultText),
                    ),
                  ),
                ],
              ),
            ),
          ),
        Composer(
          key: ValueKey(a.id),
          placeholder: works
              ? 'Say more to ${a.name}. It is heard before the next step.'
              : 'Say something to ${a.name}',
          action: works ? 'Queue' : 'Send',
          enabled: widget.live,
          leading: [
            ExecutionKey(_choices(s, a), of: a.id),
            if (a.context != null)
              ContextGauge(
                ContextView(
                  share: a.context,
                  provenance: switch (a.contextKnown) {
                    Known.reported => Provenance.reported,
                    Known.measured => Provenance.measured,
                    Known.estimated => Provenance.estimated,
                    Known.unknown => Provenance.unknown,
                  },
                ),
                hue: markOf(a.execution.provider, a.execution.model)?.color(c),
              ),
          ],
          onSubmit: (text) => _do(() => s.prompt(widget.session.id, a.id, text)),
        ),
      ],
    );
  }
}
