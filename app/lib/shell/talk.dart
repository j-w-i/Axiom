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
import '../ui/controls.dart';
import '../ui/marks.dart';
import '../ui/overlay.dart';
import '../ui/status.dart';
import '../ui/tokens.dart';
import 'drafts.dart';
import 'outside.dart';
import 'start.dart';

/// What an agent asks, and where it is spoken to. Only for an agent that can act: a replayed one is
/// only shown.
class Talk extends StatefulWidget {
  const Talk(
    this.store,
    this.session,
    this.agent, {
    required this.draft,
    required this.live,
    this.room = double.infinity,
    super.key,
  });

  /// How high the words of a question may be.
  final double room;
  final HostStore store;
  final SessionModel session;
  final AgentModel agent;
  final Draft draft;

  /// The host answers.
  final bool live;
  @override
  State<Talk> createState() => _TalkState();
}

class _TalkState extends State<Talk> {
  /// Answered from here, until the host says it is closed: not answered twice.
  final _answered = <String>{};

  /// What goes wrong is said where this agent is spoken to, also if somebody has looked elsewhere
  /// meanwhile.
  Future<bool> _do(Future<void> Function() f, {VoidCallback? failed}) async {
    final problem = widget.draft.problem..value = null;
    try {
      await f();
      return true;
    } on Object catch (e) {
      failed?.call();
      problem.value = wordsOf(e);
      return false;
    }
  }

  Future<bool> _say(String text) {
    final d = widget.draft, s = widget.store;
    final session = widget.session.id, agent = widget.agent.id;
    return _do(() async {
      await s.prompt(session, agent, text, key: d.keyFor(text));
      d.heard();
    });
  }

  /// The models of the account, if they have not been asked for.
  void _look() {
    final c = widget.agent.execution.connection;
    // A program's account lists nothing.
    if (c == null || widget.agent.execution.harness != 'axiom') return;
    if (widget.store.modelsOf(c) != null) return;
    unawaited(_do(() => widget.store.models(c)));
  }

  void _answer(RequestOpened r, Answer a, {String? words}) {
    setState(() => _answered.add(r.request));
    unawaited(
      _do(
        () => widget.store.answer(widget.session.id, r, a, words: words),
        failed: () => _answered.remove(r.request),
      ),
    );
  }

  List<ExecutionChoice> _choices(HostStore s, AgentModel a) {
    final x = a.execution;
    final other = x.harness != 'axiom';
    final program = other ? s.harnesses.where((h) => h.id == x.harness).firstOrNull : null;
    final hands = x.environment == axiomTools;
    final can = hands ? null : program?.access;
    final connection = s.connections.where((c) => c.id == x.connection).firstOrNull;
    final models = s.modelsOf(x.connection ?? '');
    final has = models?.any((m) => m.id == x.model) ?? false;
    final tries = program?.effort ?? models?.any((m) => m.id == x.model && m.effort) ?? false;
    Execution with_({
      String? model,
      String? connection,
      String? provider,
      bool? axioms,
      String? effort,
    }) => Execution(
      harness: x.harness,
      provider: provider ?? x.provider,
      model: model ?? x.model,
      connection: connection == null
          ? x.connection
          : connection.isEmpty
          ? null
          : connection,
      effort: effort == null
          ? x.effort
          : effort.isEmpty
          ? null
          : effort,
      environment: axioms == null
          ? x.environment
          : axioms
          ? axiomTools
          : null,
    );
    void set({Execution? execution, Access? access}) => unawaited(
      _do(() => s.setAgent(widget.session.id, a.id, execution: execution, access: access)),
    );
    return [
      ExecutionChoice(
        'Model',
        [
          if (other && x.model == null)
            AxMenuItem('', 'Its own model', mark: markOf(null, null, x.harness)),
          if (!has && x.model != null)
            AxMenuItem(
              x.model!,
              oneLine(x.model!, 60),
              mark: markOf(x.provider, x.model, x.harness),
            ),
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
      if (tries || x.effort != null)
        ExecutionChoice(
          'Effort',
          [
            ...efforts,
            if (x.effort case final e? when !efforts.any((i) => i.value == e))
              AxMenuItem(e, oneLine(e, 30)),
          ],
          x.effort ?? '',
          (v) => set(execution: with_(effort: v)),
        ),
      ExecutionChoice(
        'Harness',
        [
          AxMenuItem(
            x.harness,
            oneLine(s.harnessName(x.harness), 40),
            mark: markOf(null, null, x.harness),
          ),
        ],
        x.harness,
        (_) {},
      ),
      ExecutionChoice(
        'Account',
        [
          if (other) const AxMenuItem('', 'Its own sign-in'),
          if (connection == null && x.connection != null)
            AxMenuItem(x.connection!, 'Removed', enabled: false, why: 'add it again'),
          for (final c in other ? s.accountsOf(x.harness) : s.own)
            AxMenuItem(
              c.id,
              oneLine(c.name, 60),
              detail: providerName(c.provider),
              mark: markOf(c.provider, null),
              enabled: other || c.provider == x.provider,
              why: other || c.provider == x.provider
                  ? null
                  : 'another provider: choose its model first',
            ),
        ],
        x.connection ?? '',
        (v) => set(execution: with_(connection: v)),
        // What was said stays with the account it was said on.
        cost: (_) => !other || a.items.isEmpty
            ? null
            : 'The other account does not know this conversation: it begins anew there. Nothing is '
                  'sent from one account to the other.',
      ),
      if (program?.hands ?? hands)
        ExecutionChoice(
          'Tools',
          [
            const AxMenuItem(
              'axiom',
              'Axiom\'s',
              detail: 'asked about and contained',
              mark: ServiceMark.axiom,
            ),
            AxMenuItem(
              '',
              'Its own',
              detail: 'outside Axiom\'s sandbox',
              mark: markOf(null, null, x.harness),
              // It would not be refused only when it is started.
              enabled: !hands || (program?.access.contains(a.access) ?? false),
              why: 'it has no mode for this access',
            ),
          ],
          hands ? 'axiom' : '',
          (v) => set(execution: with_(axioms: v.isNotEmpty)),
          cost: (v) => v.isNotEmpty
              ? null
              : '${s.harnessName(x.harness)} then works with its own tools, outside Axiom\'s sandbox '
                    'and without Axiom\'s questions.',
        ),
      ExecutionChoice(
        'Access',
        [
          for (final i in accessItems)
            AxMenuItem(
              i.value.name,
              i.label,
              detail: i.detail,
              enabled: can == null || can.contains(i.value),
              why: '${s.harnessName(x.harness)} has no mode for it',
            ),
        ],
        (a.access ?? Access.ask).name,
        (v) => set(access: Access.values.byName(v)),
      ),
    ];
  }

  @override
  Widget build(BuildContext context) => ValueListenableBuilder(
    valueListenable: widget.draft.problem,
    builder: (context, problem, _) => _build(context, problem),
  );

  Widget _build(BuildContext context, String? problem) {
    final t = Ax.of(context), c = t.colors;
    final s = widget.store, a = widget.agent;
    final asked = [
      for (final r in widget.session.requests.values)
        if (r.opened.answerable == Answerable.live && !_answered.contains(r.opened.request))
          r.opened,
    ];
    final works = a.phase == AgentPhase.running || a.phase == AgentPhase.asking;
    final first = asked.firstOrNull;
    final plan = s.connections.any((x) => x.id == a.execution.connection && x.plan);
    void usage() => unawaited(
      _do(() async {
        if (!await browse(planUsage)) throw Failed(0, 'browser', 'it is at $planUsage');
      }),
    );
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
                RequestBar(
                  key: ValueKey(first.request),
                  RequestView(
                    agent: widget.session.agents[first.agent]?.name ?? 'An agent',
                    what: first.what,
                    subject: first.subject,
                    scope: first.scope,
                    always: first.session ? 'Allow this for the session' : null,
                    id: first.request,
                  ),
                  room: widget.room,
                  waiting: asked.length - 1,
                  onOnce: () => _answer(first, Answer.once),
                  onAlways: () => _answer(first, Answer.session),
                  onDeny: () => _answer(first, Answer.deny),
                  onDenyWith: (words) => _answer(first, Answer.deny, words: words),
                ),
              ],
            ),
          ),
        // How a run failed is said once, by the conversation.
        if (problem case final p?)
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
            ExecutionKey(_choices(s, a), of: a.id, onOpen: _look),
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
                hue: markOf(a.execution.provider, a.execution.model, a.execution.harness)?.color(c),
              ),
            if (plan)
              Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  const SizedBox(width: 4),
                  Flexible(
                    child: Text(
                      'Using ChatGPT plan',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: t.type.small.ink(c.ink3),
                    ),
                  ),
                  const SizedBox(width: 6),
                  AxButton(
                    label: 'Manage usage',
                    // At the plan's limit it is the main act: OpenAI asks that of whoever uses a
                    // plan. Nothing else is taken in the plan's place.
                    tone: (a.error ?? '').startsWith('billing') ? AxTone.primary : AxTone.quiet,
                    size: 24,
                    onPressed: usage,
                  ),
                ],
              ),
          ],
          controller: widget.draft.text,
          onSubmit: _say,
        ),
      ],
    );
  }
}
