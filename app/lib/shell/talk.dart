import 'dart:async';

import 'package:flutter/widgets.dart';

import '../host/host_store.dart';
import '../host/proto.dart';
import '../host/session_model.dart';
import '../patterns/composer.dart';
import '../patterns/context_gauge.dart';
import '../patterns/execution.dart';
import '../patterns/model_name.dart';
import '../patterns/models.dart';
import '../patterns/transcript_parts.dart';
import '../transcript/safe_text.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/marks.dart';
import '../ui/overlay.dart';
import '../ui/status.dart';
import '../ui/tokens.dart';
import 'drafts.dart';
import 'outside.dart';
import 'plan.dart';
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
    this.onAgent,
    this.onStop,
    this.drafts,
    super.key,
  });

  /// Where what could not be said to a new agent is kept.
  final Drafts? drafts;

  /// Ends the run that is on.
  final VoidCallback? onStop;

  /// Shows another agent of the session.
  final ValueChanged<String>? onAgent;

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

  /// Makes the branch, and says it there.
  Future<bool> _branch(Branching b, String text) {
    final d = widget.draft, s = widget.store;
    final session = widget.session, of = widget.agent;
    final drafts = widget.drafts, show = widget.onAgent;
    return _do(() async {
      final files = b.withFiles;
      final name = nextName(of.name, session.agents.values.map((a) => a.name));
      final agent = await s.branch(session.id, of.id, b.item, name, files: files);
      d.cancelBranch();
      show?.call(agent);
      // From here on it is the new one's: also what goes wrong.
      final there = drafts?.of(session.id, agent);
      try {
        await s.prompt(session.id, agent, text, key: freshKey());
      } on Object catch (e) {
        there
          ?..add(text)
          ..problem.value = wordsOf(e);
      }
    });
  }

  Widget _branchHeader(BuildContext context, Branching b) {
    final t = Ax.of(context), c = t.colors;
    return Padding(
      padding: const EdgeInsets.fromLTRB(AxSpace.edge, 8, 8, 0),
      child: Row(
        children: [
          AxGlyph(AxIcon.branch, color: c.ink2),
          const SizedBox(width: 8),
          Expanded(
            child: Semantics(
              liveRegion: true,
              child: Text(
                b.words != null
                    ? 'Said differently, in a new branch'
                    : 'In a new branch, from this answer on',
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
                style: t.type.ui.ink(c.ink2),
              ),
            ),
          ),
          AxButton(
            label: 'Cancel',
            tone: AxTone.quiet,
            size: 24,
            onPressed: widget.draft.cancelBranch,
          ),
        ],
      ),
    );
  }

  /// What the branch works in.
  Widget _branchFiles(BuildContext context, Branching b) {
    final t = Ax.of(context), c = t.colors;
    final words = switch (b) {
      Branching(apart: false) => 'In the same folder. Its files stay as they are.',
      Branching(files: false) => 'In a fresh copy of the folder. Its files then are not kept.',
      _ => null,
    };
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
      child: words != null
          ? Text(words, style: t.type.small.ink(c.ink3))
          : AxCheck(
              label: 'With its files as they were then',
              value: b.withFiles,
              onChanged: (v) => setState(() => b.withFiles = v),
            ),
    );
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
    final connection = s.connections.where((c) => c.id == x.connection).firstOrNull;
    final models = s.modelsOf(x.connection ?? '');
    final names = namesOf(models ?? const []);
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
    void set({Execution? execution}) =>
        unawaited(_do(() => s.setAgent(widget.session.id, a.id, execution: execution)));
    return [
      ExecutionChoice(
        'Model',
        [
          if (other && x.model == null)
            AxMenuItem('', 'Default model', mark: markOf(null, null, x.harness)),
          if (!has && x.model != null)
            AxMenuItem(
              x.model!,
              oneLine(modelName(x.model!), 60),
              mark: markOf(x.provider, x.model, x.harness),
            ),
          for (final m in models ?? const <ModelInfo>[]) modelItem(m, names),
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
          if (other) const AxMenuItem('', 'Default sign-in'),
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
            const AxMenuItem('axiom', 'Axiom tools', detail: 'sandboxed', mark: ServiceMark.axiom),
            AxMenuItem(
              '',
              'Built-in tools',
              detail: 'not sandboxed',
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
    ];
  }

  Widget _access(HostStore s, AgentModel a) {
    final x = a.execution;
    final program = x.harness == 'axiom' || x.environment == axiomTools
        ? null
        : s.harnesses.where((h) => h.id == x.harness).firstOrNull;
    return AccessKey(
      a.access ?? Access.ask,
      can: program?.access ?? Access.values,
      of: s.harnessName(x.harness),
      plan: a.planning,
      plans: program == null,
      onChanged: (v, plan) => unawaited(
        _do(
          () => plan
              ? s.setAgent(widget.session.id, a.id, plan: true)
              : s.setAgent(widget.session.id, a.id, access: v),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: Listenable.merge([widget.draft.problem, widget.draft.branching]),
    builder: (context, _) =>
        _build(context, widget.draft.problem.value, widget.draft.branching.value),
  );

  Widget _build(BuildContext context, String? problem, Branching? branch) {
    final t = Ax.of(context), c = t.colors;
    final s = widget.store, a = widget.agent;
    final asked = [
      for (final r in widget.session.requests.values)
        if (r.opened.answerable == Answerable.live && !_answered.contains(r.opened.request))
          r.opened,
    ];
    // What another agent asks is asked where that agent is; here only that it does.
    final mine = [
      for (final r in asked)
        if (r.agent == a.id || !widget.session.agents.containsKey(r.agent)) r,
    ];
    final others = {
      for (final r in asked)
        if (!mine.contains(r)) r.agent,
    };
    final works = a.phase == AgentPhase.running || a.phase == AgentPhase.asking;
    final first = mine.firstOrNull;
    final plan = s.connections.any((x) => x.id == a.execution.connection && x.plan);
    void usage() => unawaited(
      _do(() async {
        if (!await browse(planUsage)) throw Failed(0, 'browser', 'it is at $planUsage');
      }),
    );
    return LayoutBuilder(
      builder: (context, box) => Padding(
        padding: EdgeInsets.fromLTRB(
          AxSpace.beside(box.maxWidth),
          0,
          AxSpace.beside(box.maxWidth),
          16,
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: [
            if (first != null && widget.live)
              Padding(
                padding: const EdgeInsets.only(bottom: 8),
                child: RequestBar(
                  key: ValueKey(first.request),
                  RequestView(
                    agent: widget.session.agents[first.agent]?.name ?? 'An agent',
                    what: first.what,
                    subject: first.subject,
                    scope: first.scope,
                    always: first.session ? 'Allow for this session' : null,
                    id: first.request,
                  ),
                  room: widget.room,
                  waiting: mine.length - 1,
                  onOnce: () => _answer(first, Answer.once),
                  onAlways: () => _answer(first, Answer.session),
                  onDeny: () => _answer(first, Answer.deny),
                  onDenyWith: (words) => _answer(first, Answer.deny, words: words),
                ),
              ),
            if (widget.live)
              for (final id in others)
                Padding(
                  padding: const EdgeInsets.only(bottom: 8),
                  child: Row(
                    children: [
                      const AxLamp(AgentState.asking),
                      const SizedBox(width: 8),
                      Expanded(
                        child: Text(
                          '${oneLine(literal(widget.session.agents[id]!.name), 60)} is waiting for you',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: t.type.ui.ink(c.ink2),
                        ),
                      ),
                      if (widget.onAgent case final go?)
                        AxButton(label: 'Review', onPressed: () => go(id)),
                    ],
                  ),
                ),
            if (a.proposed case final p? when widget.live && branch == null)
              Padding(
                padding: const EdgeInsets.only(bottom: 8),
                child: PlanBar(
                  p.source,
                  key: ValueKey(p.id),
                  can: const [Access.ask, Access.work],
                  onAccept: (access, text) =>
                      _do(() => s.acceptPlan(widget.session.id, a.id, p.id, access, text: text)),
                ),
              ),
            // How a run failed is said once, by the conversation.
            if (problem case final p?)
              Semantics(
                liveRegion: true,
                child: Padding(
                  padding: const EdgeInsets.only(bottom: 10),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      const Padding(
                        padding: EdgeInsets.only(top: 3),
                        child: AxLamp(AgentState.failed),
                      ),
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
              key: ValueKey((a.id, branch)),
              // The keyboard goes where the words are.
              autofocus: branch != null,
              header: branch == null ? null : _branchHeader(context, branch),
              placeholder: branch != null
                  ? 'Message the new branch'
                  : a.proposed != null
                  ? 'What should be different'
                  : works
                  ? 'Message ${a.name}. Sent before its next step'
                  : 'Message ${a.name}',
              action: works && branch == null ? 'Queue' : 'Send',
              enabled: widget.live,
              onStop: widget.onStop,
              leading: [
                if (branch != null)
                  _branchFiles(context, branch)
                else ...[
                  ExecutionKey(_choices(s, a), of: a.id, onOpen: _look),
                  _access(s, a),
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
                          tone: (a.error ?? '').startsWith('billing')
                              ? AxTone.primary
                              : AxTone.quiet,
                          size: 24,
                          onPressed: usage,
                        ),
                      ],
                    ),
                ],
              ],
              controller: widget.draft.text,
              onSubmit: branch == null ? _say : (text) => _branch(branch, text),
              earlier: () => widget.agent.said,
            ),
          ],
        ),
      ),
    );
  }
}
