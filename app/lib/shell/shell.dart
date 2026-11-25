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
import 'changes.dart';
import 'drafts.dart';
import 'start.dart';
import 'talk.dart';

/// The application's window, on what the host says. Nothing here is specimen data.
class Shell extends StatefulWidget {
  const Shell(this.store, {super.key, this.titleInset = 0});
  final HostStore store;

  /// Room for the window's own buttons.
  final double titleInset;
  @override
  State<Shell> createState() => _ShellState();
}

/// What the display shows.
enum _View { work, start, agent, accounts }

class _ShellState extends State<Shell> with WidgetsBindingObserver {
  String? _agent;
  String? _session;

  /// An agent of a session that is being opened.
  (String, String)? _wanted;

  /// Where it is asked is where it is answered.
  void _answer(Waits w) {
    setState(() {
      _view = _View.work;
      _tab = 0;
      if (widget.store.current?.id == w.session) {
        _agent = w.agent;
      } else {
        _wanted = (w.session, w.agent);
      }
    });
    if (widget.store.current?.id != w.session) {
      _do('The session was not opened', () => widget.store.open(w.session));
    }
  }

  _View _view = _View.work;

  /// Of the agent's views, which.
  int _tab = 0;
  final _drafts = Drafts();
  late final _start = StartDraft(widget.store);

  /// Another agent for a session, while it is set up.
  StartDraft? _more;

  void _addAgent(String session) {
    if (_more?.into != session) {
      _more?.dispose();
      _more = StartDraft(widget.store, into: session)
        ..folder.text = widget.store.current?.workspace ?? '';
    }
    _show(_View.agent);
  }

  /// Where its person was last is where the window opens, once.
  bool _opened = false;
  void _begin(HostStore s) {
    if (_opened || s.phase != HostPhase.connected) return;
    if (s.current != null || _view != _View.work) {
      _opened = true;
      return;
    }
    final last = s.sessions.firstOrNull;
    if (last == null) return;
    _opened = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && s.current == null) {
        _do('The session was not opened', () => s.open(last.id));
      }
    });
  }

  /// What a key was pressed for and did not come about.
  String? _failed;
  Timer? _forget;

  void _show(_View v) => setState(() => _view = v);

  void _do(String what, Future<void> Function() f) {
    _say(null);
    unawaited(f().catchError((Object e) => _say('$what: ${wordsOf(e)}')));
  }

  void _say(String? failed) {
    _forget?.cancel();
    if (!mounted) return;
    setState(() => _failed = failed);
    if (failed != null) _forget = Timer(const Duration(seconds: 15), () => _say(null));
  }

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  /// What came meanwhile is before its person now.
  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed && mounted) setState(() {});
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _forget?.cancel();
    _drafts.dispose();
    _start.dispose();
    _more?.dispose();
    super.dispose();
  }

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
          _begin(s);
          // An agent is chosen within its session only.
          if (_session != s.current?.id) {
            _session = s.current?.id;
            _agent = _wanted?.$1 == _session ? _wanted!.$2 : null;
            _wanted = null;
          }
          final stale = s.phase != HostPhase.connected;
          return Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              SizedBox(
                width: 264,
                child: FocusTraversalGroup(
                  child: _Side(
                    store: s,
                    inset: widget.titleInset,
                    agent: _agent,
                    onAgent: (id) => setState(() {
                      _agent = id;
                      _view = _View.work;
                    }),
                    onAddAgent: () => _addAgent(s.current!.id),
                    onStart: () => _show(_View.start),
                    onAccounts: () => _show(_View.accounts),
                    onAnswer: _answer,
                    onOpen: (id) {
                      _wanted = null;
                      _show(_View.work);
                      _do('The session was not opened', () => s.open(id));
                    },
                  ),
                ),
              ),
              Expanded(
                child: FocusTraversalGroup(
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
                              ] else if (_failed case final f?) ...[
                                const AxLamp(AgentState.failed),
                                const SizedBox(width: 10),
                                Expanded(
                                  child: Semantics(
                                    liveRegion: true,
                                    child: Text(
                                      oneLine(f, 200),
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: t.type.label.ink(c.faultText),
                                    ),
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
                              // Kept while the host is away: what was written is not lost.
                              _ when _view == _View.start => StartPage(
                                _start,
                                onStarted: (_) => _show(_View.work),
                                onAccounts: () => _show(_View.accounts),
                              ),
                              // Only while its session is the one that is open.
                              _ when _view == _View.agent && _more?.into == s.current?.id =>
                                StartPage(
                                  _more!,
                                  key: ObjectKey(_more),
                                  onStarted: (_) => setState(() {
                                    _agent = _more!.last;
                                    _view = _View.work;
                                  }),
                                  onAccounts: () => _show(_View.accounts),
                                ),
                              _ when _view == _View.accounts => AccountsPage(
                                s,
                                onDone: () => _show(s.current == null ? _View.start : _View.work),
                              ),
                              HostStore(phase: HostPhase.starting, current: null) => const _Word(
                                'Starting the host',
                              ),
                              HostStore(phase: HostPhase.unreachable, current: null) => _Word(
                                'The host is not answering',
                                detail: s.problem,
                              ),
                              HostStore(current: final m?) => _Display(
                                s,
                                m,
                                agent: _agent,
                                stale: stale,
                                drafts: _drafts,
                                tab: _tab,
                                onTab: (i) => setState(() => _tab = i),
                                onAgain: () =>
                                    _do('The session was not opened', () => s.open(m.id)),
                                onStop: (run) =>
                                    _do('It was not stopped', () => s.cancel(m.id, run)),
                              ),
                              _ => _Word(
                                s.sessions.isEmpty ? 'No sessions yet' : 'Choose a session',
                                action: s.sessions.isEmpty
                                    ? ('New session', () => _show(_View.start))
                                    : null,
                              ),
                            },
                          ),
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

/// What a tool is doing, and what it did, as a person would say it.
(String, String)? _verbs(String tool) => switch (tool.toLowerCase()) {
  'read' || 'notebookread' => ('Reading', 'Read'),
  'edit' ||
  'write' ||
  'multiedit' ||
  'notebookedit' ||
  'patch' ||
  'apply_patch' => ('Editing', 'Edited'),
  'search' || 'glob' || 'list' || 'grep' || 'ls' || 'find' => ('Searching', 'Searched'),
  'shell' || 'test' || 'job' || 'bash' || 'execute' || 'exec_command' => ('Running', 'Ran'),
  'fetch' || 'webfetch' || 'websearch' => ('Fetching', 'Fetched'),
  'delegate' || 'agent' || 'task' => ('Working with another agent', 'Worked with another agent'),
  _ => null,
};

String _upper(String s) => s.isEmpty ? s : s[0].toUpperCase() + s.substring(1);

/// What a call is doing: never by the tool's own name alone, if it is one that is known.
String toolWords(ItemModel i) {
  final tool = i.name.toLowerCase();
  final verbs = _verbs(tool);
  final verb = verbs == null
      ? _upper(oneLine(literal(i.name), 40))
      : i.open
      ? verbs.$1
      : verbs.$2;
  if (const {'delegate', 'agent', 'task'}.contains(tool)) return verb;
  String about;
  if (const {'patch', 'apply_patch'}.contains(tool)) {
    final n = ToolRow.patched(i).length;
    about = n == 0 ? '' : (n == 1 ? '1 file' : '$n files');
  } else {
    about = ToolRow.detailOf(i.args).trim();
    if (!about.contains(' ') && about.contains('/')) {
      about = about.split('/').lastWhere((x) => x.isNotEmpty, orElse: () => about);
    }
    about = oneLine(literal(about), 40);
  }
  return about.isEmpty ? verb : '$verb $about';
}

/// The second line of an agent's row: what it wants, how it ended, or what it is doing.
ActivityView? _activity(AgentModel a, HostStore s) {
  final asks = s.current?.requests.values
      .where((r) => r.opened.agent == a.id && r.opened.answerable == Answerable.live)
      .firstOrNull;
  if (asks != null) {
    return ActivityView(AxIcon.shield, _upper(oneLine(literal(asks.opened.what), 60)));
  }
  if (a.ended == Outcome.failed) {
    return const ActivityView(AxIcon.warn, 'Stopped with an error', failed: true);
  }
  final last = a.last;
  if (last == null) return null;
  return ActivityView(last.rich ? AxIcon.think : ToolRow.iconOf(last.name), switch (last.kind) {
    ItemKind.tool => toolWords(last),
    ItemKind.reasoning => 'Thinking',
    ItemKind.prompt => 'Reading the request',
    _ => last.open ? 'Writing' : 'Wrote an answer',
  }, failed: last.outcome == Outcome.failed);
}

/// A run that failed is seen as that, whatever its agent could do next.
AgentState stateOf(AgentModel a) =>
    a.ended == Outcome.failed ? AgentState.failed : phaseOf(a.phase);

AgentView viewOf(AgentModel a, HostStore s) {
  return AgentView(
    id: a.id,
    name: a.name,
    state: stateOf(a),
    // What chooses its model itself is named instead.
    model: oneLine(
      a.execution.model ??
          (a.execution.harness == 'axiom' ? '' : s.harnessName(a.execution.harness)),
      60,
    ),
    harness: a.execution.harness,
    service: markOf(a.execution.provider, a.execution.model, a.execution.harness),
    connection: a.execution.connection ?? '',
    elapsed: '',
    context: a.context,
    contextProvenance: provenanceOf(a.contextKnown),
    recent: [?_activity(a, s)],
  );
}

class _Side extends StatelessWidget {
  const _Side({
    required this.store,
    required this.inset,
    required this.agent,
    required this.onAgent,
    required this.onAddAgent,
    required this.onStart,
    required this.onAccounts,
    required this.onOpen,
    required this.onAnswer,
  });
  final ValueChanged<Waits> onAnswer;
  final HostStore store;
  final double inset;
  final String? agent;
  final ValueChanged<String> onAgent, onOpen;
  final VoidCallback onStart, onAddAgent, onAccounts;

  /// At most four characters; nothing where no time was said.
  static String ageOf(int ms) {
    if (ms <= 0) return '';
    final d = DateTime.now().difference(DateTime.fromMillisecondsSinceEpoch(ms));
    if (d.inMinutes < 1) return 'now';
    if (d.inHours < 1) return '${d.inMinutes}m';
    if (d.inDays < 1) return '${d.inHours}h';
    return d.inDays < 365 ? '${d.inDays}d' : '${d.inDays ~/ 365}y';
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
                  AxIcon.web,
                  label: 'Accounts and servers',
                  tone: AxTone.quiet,
                  onPressed: onAccounts,
                ),
                AxButton.icon(
                  AxIcon.plus,
                  label: 'New session',
                  tone: AxTone.quiet,
                  onPressed: onStart,
                ),
              ],
            ),
          ),
        ),
        Expanded(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(8, 0, 8, 12),
            children: [
              for (final (label, color, all) in [
                (
                  'Needs you',
                  c.ink2,
                  [
                    for (final w in store.needs)
                      if (w.need != Need.finished) w,
                  ],
                ),
                (
                  'Finished, not seen yet',
                  c.ink3,
                  [
                    for (final w in store.needs)
                      if (w.need == Need.finished) w,
                  ],
                ),
              ])
                if (all.isNotEmpty) ...[
                  Semantics(
                    liveRegion: true,
                    child: Padding(
                      padding: const EdgeInsets.fromLTRB(12, 0, 12, 4),
                      child: Row(
                        children: [
                          Expanded(child: Text(label, style: t.type.label.ink(color))),
                          if (all.length > 1)
                            Text('${all.length}', style: t.type.small.ink(c.ink3)),
                        ],
                      ),
                    ),
                  ),
                  for (final w in all)
                    NeedsRow(
                      key: ValueKey((w.session, w.agent, w.request)),
                      agent: w.name.isEmpty ? 'An agent' : oneLine(literal(w.name), 40),
                      what: switch (w.need) {
                        Need.failed => 'failed',
                        Need.limit => 'has reached a limit',
                        Need.finished => 'has finished',
                        _ => oneLine(literal(w.what), 60),
                      },
                      state: switch (w.need) {
                        Need.failed || Need.limit => AgentState.failed,
                        Need.finished => AgentState.done,
                        _ => AgentState.asking,
                      },
                      subject: oneLine(literal(w.subject), 120),
                      session: w.title.isEmpty ? 'Session' : oneLine(literal(w.title), 60),
                      age: ageOf(w.at),
                      onOpen: () => onAnswer(w),
                    ),
                  const SizedBox(height: 16),
                ],
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
                        agents: [for (final a in m.agents.values) viewOf(a, store)],
                      ),
                      selected: m.agents.containsKey(agent) ? agent : m.agents.keys.firstOrNull,
                      onSelect: onAgent,
                      // A replay's session has nobody to work beside.
                      onAddAgent: m.workspace == null ? null : onAddAgent,
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
                  SessionView(
                    id: s.id,
                    title: oneLine(s.title),
                    age: ageOf(s.updated),
                    working: s.running,
                  ),
                  onOpen: () => onOpen(s.id),
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
  const _Display(
    this.store,
    this.session, {
    required this.agent,
    required this.stale,
    required this.drafts,
    required this.tab,
    required this.onTab,
    required this.onAgain,
    required this.onStop,
  });
  final HostStore store;
  final Drafts drafts;
  final int tab;
  final ValueChanged<int> onTab;
  final ValueChanged<String> onStop;
  final SessionModel session;
  final String? agent;
  final bool stale;
  final VoidCallback onAgain;
  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: session,
    builder: (context, _) {
      final a = session.agents[agent] ?? session.agents.values.firstOrNull;
      if (session.fault case final f?) {
        return _Word('This session cannot be shown', detail: f, action: ('Try again', onAgain));
      }
      if (!session.loaded) return _Word(stale ? 'Waiting for the host' : 'Reading the session');
      if (a == null) return const _Word('No agent has worked here yet');
      return _work(context, a);
    },
  );

  /// Somebody looks: the window has their attention.
  static bool get _attended => switch (WidgetsBinding.instance.lifecycleState) {
    null || AppLifecycleState.resumed => true,
    _ => false,
  };

  Widget _work(BuildContext context, AgentModel a) {
    final c = Ax.of(context).colors;
    final acts = a.access != null;
    if (_attended &&
        store.needs.any((w) => w.session == session.id && w.agent == a.id && w.need != Need.asks)) {
      WidgetsBinding.instance.addPostFrameCallback((_) => store.seen(session.id, a.id));
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        // The transcript between these two looks after itself.
        ListenableBuilder(listenable: a, builder: (context, _) => _bar(context, a)),
        ColoredBox(color: c.bayRule, child: const SizedBox(height: 1)),
        if (acts && tab == 1)
          Expanded(
            child: a.apart
                ? CopyView(
                    store,
                    session,
                    a,
                    draft: drafts.of(session.id, a.id),
                    key: ValueKey((session.id, a.id)),
                  )
                : ChangesView(
                    store,
                    session,
                    agent: a.name,
                    draft: drafts.of(session.id, a.id),
                    key: ValueKey(session.id),
                  ),
          )
        else
          Expanded(
            child: LayoutBuilder(
              builder: (context, box) => Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Expanded(child: TranscriptView(a)),
                  // In a window too small for all of it, its end is in sight and the rest scrolls.
                  ConstrainedBox(
                    constraints: BoxConstraints(maxHeight: box.maxHeight),
                    child: SingleChildScrollView(
                      reverse: true,
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          if (session.requests.values.any(
                            (r) => !acts || r.opened.answerable != Answerable.live,
                          ))
                            _Asked(session, all: !acts),
                          if (acts)
                            ListenableBuilder(
                              listenable: a,
                              builder: (context, _) => Talk(
                                store,
                                session,
                                a,
                                draft: drafts.of(session.id, a.id),
                                live: !stale,
                                // What is asked stays in sight beside its keys.
                                room: box.maxHeight * .35,
                                key: ValueKey('${session.id} ${a.id}'),
                              ),
                            ),
                        ],
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
      ],
    );
  }

  Widget _bar(BuildContext context, AgentModel a) {
    final t = Ax.of(context), c = t.colors;
    final x = a.execution;
    final acts = a.access != null;
    final run = a.run;
    final works = a.phase == AgentPhase.running || a.phase == AgentPhase.asking;
    return AgentBar(
      name: a.name,
      state: stateOf(a),
      mark: markOf(x.provider, x.model, x.harness),
      tabs: [('Conversation', null), if (acts) ('Changes', null)],
      index: acts ? tab : 0,
      onTab: onTab,
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (!acts)
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
          if (!acts && a.context != null) ...[
            const SizedBox(width: 10),
            Flexible(
              child: ContextGauge(
                ContextView(share: a.context, provenance: provenanceOf(a.contextKnown)),
                hue: ServiceMark.of(x.model)?.color(c),
              ),
            ),
          ],
          if (acts && works && run != null && !stale)
            AxButton(
              label: 'Stop',
              icon: AxIcon.stop,
              tone: AxTone.quiet,
              onPressed: () => onStop(run),
            ),
          if (stale) ...[
            const SizedBox(width: 10),
            Flexible(
              child: Text(
                'not live',
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: t.type.label.ink(c.attentionText),
              ),
            ),
          ],
        ],
      ),
    );
  }
}

/// What agents of the session have asked and nobody can answer here: it is history, or a replay's.
class _Asked extends StatelessWidget {
  const _Asked(this.session, {required this.all});
  final SessionModel session;
  final bool all;
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
                if (all || r.opened.answerable != Answerable.live)
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
