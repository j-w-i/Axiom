import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../host/host_store.dart';
import '../host/proto.dart';
import '../host/session_model.dart';
import '../patterns/agent_bar.dart';
import '../patterns/context_gauge.dart';
import '../patterns/models.dart';
import '../patterns/session_panel.dart';
import '../patterns/transcript_parts.dart';
import '../transcript/safe_text.dart';
import '../transcript/transcript_view.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/marks.dart';
import '../ui/overlay.dart';
import '../ui/press.dart';
import '../ui/status.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';
import 'changes.dart';
import 'drafts.dart';
import 'find.dart';
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
      _do('The session could not be opened', () => widget.store.open(w.session));
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
        _do('The session could not be opened', () => s.open(last.id));
      }
    });
  }

  /// What a key was pressed for and did not come about.
  String? _failed;
  Timer? _forget;

  void _show(_View v) => setState(() => _view = v);

  /// The list beside the display, and the panel on its other side, as their person wants them.
  bool _list = true, _panel = true;
  bool _finding = false;

  /// What has changed in the open session's folder.
  FolderChanges? _changes;
  FolderChanges _changesOf(SessionModel m) {
    if (!identical(_changes?.session, m)) {
      _changes?.dispose();
      _changes = FolderChanges(widget.store, m);
    }
    return _changes!;
  }

  /// Back from a page to what was shown before it.
  void _back() => _show(_View.work);

  void _startIn(String? folder) {
    if (folder != null) _start.folder.text = folder;
    _show(_View.start);
  }

  static final _mac = defaultTargetPlatform == TargetPlatform.macOS;
  static String keysOf(String key) => _mac ? '⌘$key' : 'Ctrl+$key';

  /// The window's own keys, wherever the keyboard is.
  bool _key(KeyEvent e) {
    final k = HardwareKeyboard.instance;
    if (e is! KeyDownEvent || k.isAltPressed || k.isShiftPressed) return false;
    if (_mac ? !k.isMetaPressed || k.isControlPressed : !k.isControlPressed || k.isMetaPressed) {
      return false;
    }
    switch (e.logicalKey) {
      case LogicalKeyboardKey.keyN:
        _show(_View.start);
      case LogicalKeyboardKey.keyK:
        setState(() => _finding = !_finding);
      case LogicalKeyboardKey.comma:
        _show(_View.accounts);
      case LogicalKeyboardKey.keyB:
        setState(() => _list = !_list);
      default:
        return false;
    }
    return true;
  }

  List<Found> _everything(HostStore s) => [
    Found('New session', () => _startIn(null), icon: AxIcon.plus, keys: keysOf('N')),
    Found('Accounts', () => _show(_View.accounts), icon: AxIcon.settings, keys: keysOf(',')),
    if (s.current case final m?) ...[
      if (m.workspace != null)
        Found(
          'Add an agent',
          () => _addAgent(m.id),
          detail: oneLine(m.title, 60),
          icon: AxIcon.plus,
        ),
      for (final a in m.agents.values)
        Found(
          oneLine(literal(a.name), 40),
          () => setState(() {
            _agent = a.id;
            _view = _View.work;
          }),
          detail: oneLine(m.title, 60),
          mark: markOf(a.execution.provider, a.execution.model, a.execution.harness),
        ),
    ],
    for (final x in s.sessions)
      Found(
        oneLine(x.title, 80),
        () => _open(x.id),
        detail: x.workspace == null ? null : folderName(x.workspace!),
      ),
  ];

  void _open(String id) {
    _wanted = null;
    _show(_View.work);
    if (widget.store.current?.id == id) return;
    _do('The session could not be opened', () => widget.store.open(id));
  }

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
    HardwareKeyboard.instance.addHandler(_key);
  }

  /// What came meanwhile is before its person now.
  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed && mounted) setState(() {});
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    HardwareKeyboard.instance.removeHandler(_key);
    _changes?.dispose();
    _forget?.cancel();
    _drafts.dispose();
    _start.dispose();
    _more?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return ColoredBox(
      color: c.bay.fill,
      child: ListenableBuilder(
        listenable: widget.store,
        builder: (context, _) {
          final s = widget.store;
          _begin(s);
          // An agent is chosen within its session only.
          if (_session != s.current?.id) {
            _session = s.current?.id;
            _agent = switch (_wanted) {
              (final session, final agent) when session == _session => agent,
              _ => null,
            };
            _wanted = null;
          }
          final stale = s.phase != HostPhase.connected;
          final width = MediaQuery.sizeOf(context).width;
          final narrow = width < 860, beside = !narrow && _list;
          final roomy = width >= 1180;
          Widget side([VoidCallback? close]) => _Side(
            store: s,
            inset: widget.titleInset,
            agent: _agent,
            view: _view,
            close: close,
            onAgent: (id) => setState(() {
              _agent = id;
              _view = _View.work;
            }),
            onAddAgent: () => _addAgent(s.current!.id),
            onStart: _startIn,
            onFind: () => setState(() => _finding = true),
            onAccounts: () => _show(_View.accounts),
            onHide: () => setState(() => _list = false),
            onAnswer: _answer,
            onOpen: _open,
          );
          final page = switch (_view) {
            _View.start => 'New session',
            _View.agent => 'New agent',
            _View.accounts => 'Accounts',
            _View.work => null,
          };
          final window = Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (beside) ...[
                SizedBox(
                  width: width < 1080 ? 224 : 260,
                  child: ColoredBox(
                    color: c.chassis.fill,
                    child: FocusTraversalGroup(child: side()),
                  ),
                ),
                const Hairline(vertical: true),
              ],
              Expanded(
                // Kept when the window is made wider or narrower.
                key: const ValueKey('display'),
                child: FocusTraversalGroup(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      SizedBox(
                        height: 44,
                        child: _Strip(
                          problem: switch (s) {
                            _ when stale && s.phase != HostPhase.starting =>
                              s.binary == null && s.current == null
                                  ? 'axiom-host was not found beside the application'
                                  : 'The host is not answering. Trying again.',
                            HostStore(:final damaged?) =>
                              'The journal is damaged: ${oneLine(damaged, 200)}',
                            _ => _failed == null ? null : oneLine(_failed!, 200),
                          },
                          page: page,
                          session: s.current,
                          inset: beside ? 0 : widget.titleInset,
                          needs: s.needs,
                          list: narrow ? side : null,
                          onList: beside || narrow ? null : () => setState(() => _list = true),
                          panel: roomy && page == null && s.current?.workspace != null
                              ? _panel
                              : null,
                          onPanel: () => setState(() => _panel = !_panel),
                          onAnswer: _answer,
                          onStart: () => _startIn(null),
                          onAccounts: () => _show(_View.accounts),
                          onFind: () => setState(() => _finding = true),
                        ),
                      ),
                      if (page != null) const Hairline(),
                      Expanded(
                        child: switch (s) {
                          // Kept while the host is away: what was written is not lost.
                          _ when _view == _View.start => StartPage(
                            _start,
                            onStarted: (_) => _show(_View.work),
                            onAccounts: () => _show(_View.accounts),
                            onCancel: s.current == null ? null : _back,
                          ),
                          // Only while its session is the one that is open.
                          _ when _view == _View.agent && _more?.into == s.current?.id => StartPage(
                            _more!,
                            key: ObjectKey(_more),
                            onStarted: (_) => setState(() {
                              _agent = _more!.last;
                              _view = _View.work;
                            }),
                            onAccounts: () => _show(_View.accounts),
                            onCancel: _back,
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
                            changes: _changesOf(m),
                            panel: roomy && _panel,
                            tab: _tab,
                            onTab: (i) => setState(() => _tab = i),
                            onAgain: () =>
                                _do('The session could not be opened', () => s.open(m.id)),
                            onStop: (run) =>
                                _do('It could not be stopped', () => s.cancel(m.id, run)),
                            onAgent: (id) => setState(() {
                              _agent = id;
                              _tab = 0;
                            }),
                          ),
                          _ => _Word(
                            s.sessions.isEmpty ? 'No sessions yet' : 'Choose a session',
                            action: s.sessions.isEmpty
                                ? ('New session', () => _show(_View.start))
                                : null,
                          ),
                        },
                      ),
                    ],
                  ),
                ),
              ),
            ],
          );
          return Stack(
            children: [
              Positioned.fill(
                child: ExcludeFocus(excluding: _finding, child: window),
              ),
              if (_finding)
                Positioned.fill(
                  child: SearchBox(
                    all: _everything(s),
                    onClose: () => setState(() => _finding = false),
                  ),
                ),
            ],
          );
        },
      ),
    );
  }
}

/// What a folder is called: the last part of its path.
String folderName(String path) => oneLine(
  literal(path.split(RegExp(r'[/\\]')).lastWhere((x) => x.isNotEmpty, orElse: () => path)),
  80,
);

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

String _upper(String s) => s.isEmpty ? s : s[0].toUpperCase() + s.substring(1);

/// What a call is doing: never by the tool's own name alone, if it is one that is known.
String toolWords(ItemModel i) {
  final tool = i.name.toLowerCase();
  final verbs = ToolRow.verbs(tool);
  final verb = verbs == null
      ? _upper(oneLine(literal(i.name), 40))
      : i.open
      ? verbs.$1
      : verbs.$2;
  if (const {'delegate', 'agent', 'task'}.contains(tool)) return verb;
  String about;
  if (ToolRow.isPatch(tool)) {
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
    ItemKind.prompt => 'Starting',
    _ => last.open ? 'Writing' : 'Replied',
  }, failed: last.outcome == Outcome.failed);
}

/// A run that failed is seen as that, whatever its agent could do next.
AgentState stateOf(AgentModel a) =>
    a.ended == Outcome.failed ? AgentState.failed : phaseOf(a.phase);

/// What its model is called, where the host has said so.
String? _modelName(Execution x, HostStore s) {
  final id = x.model, on = x.connection;
  if (id == null || on == null) return id;
  for (final m in s.modelsOf(on) ?? const <ModelInfo>[]) {
    if (m.id == id) return m.name ?? id;
  }
  return id;
}

AgentView viewOf(AgentModel a, HostStore s) {
  return AgentView(
    id: a.id,
    name: a.name,
    state: stateOf(a),
    // What chooses its model itself is named instead.
    model: oneLine(
      _modelName(a.execution, s) ??
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

/// What stands over the display: where one is, or what has failed, and, where the window is too narrow
/// for the list beside it, the way to the list.
class _Strip extends StatelessWidget {
  const _Strip({
    required this.problem,
    required this.page,
    required this.session,
    required this.inset,
    required this.needs,
    required this.list,
    required this.onList,
    required this.panel,
    required this.onPanel,
    required this.onAnswer,
    required this.onStart,
    required this.onAccounts,
    required this.onFind,
  });
  final String? problem;

  /// The page that is shown, if it is not a session's.
  final String? page;
  final SessionModel? session;
  final double inset;
  final List<Waits> needs;
  final Widget Function(VoidCallback close)? list;

  /// Brings back the list, where it was put away.
  final VoidCallback? onList;

  /// Whether the panel is shown; null where there is none.
  final bool? panel;
  final VoidCallback onPanel;
  final ValueChanged<Waits> onAnswer;
  final VoidCallback onStart, onAccounts, onFind;

  Widget _where(AxTheme t) {
    final c = t.colors, m = session;
    final strong = t.type.uiStrong;
    if (page case final p? when p != 'New agent') {
      return Semantics(header: true, child: Text(p, style: strong));
    }
    if (m == null) return const SizedBox.shrink();
    return ListenableBuilder(
      listenable: m,
      builder: (_, _) {
        final at = m.workspace;
        Widget between() => Padding(
          padding: const EdgeInsets.symmetric(horizontal: 8),
          child: ExcludeSemantics(child: Text('/', style: t.type.ui.ink(c.ink4))),
        );
        Widget part(String words, TextStyle style) => Flexible(
          child: Text(words, maxLines: 1, overflow: TextOverflow.ellipsis, style: style),
        );
        return Row(
          children: [
            if (at != null) ...[
              Flexible(
                child: Semantics(
                  label: 'Folder $at',
                  excludeSemantics: true,
                  child: Text(
                    folderName(at),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: t.type.ui.ink(c.ink3),
                  ),
                ),
              ),
              between(),
            ],
            part(m.title.isEmpty ? 'Session' : oneLine(m.title, 120), strong),
            if (page case final p?) ...[between(), part(p, strong)],
          ],
        );
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final said = problem == null
        ? null
        : Semantics(
            liveRegion: true,
            child: Text(
              problem!,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: t.type.label.ink(c.faultText),
            ),
          );
    final failed = Row(
      children: [
        if (said != null) ...[
          const AxLamp(AgentState.failed),
          const SizedBox(width: 10),
          Expanded(child: said),
        ],
      ],
    );
    final open = list;
    if (open == null) {
      return Padding(
        padding: EdgeInsets.only(left: onList == null ? 20 : inset + 12, right: 12),
        child: Row(
          children: [
            if (onList case final f?) ...[
              AxButton.icon(
                AxIcon.sidebar,
                label: 'Show the sidebar',
                tone: AxTone.quiet,
                onPressed: f,
              ),
              const SizedBox(width: 10),
            ],
            Expanded(child: said == null ? _where(t) : failed),
            if (panel case final p?) ...[
              const SizedBox(width: 8),
              AxButton.icon(
                AxIcon.panel,
                label: p ? 'Hide the panel' : 'Show the panel',
                tone: AxTone.quiet,
                onPressed: onPanel,
              ),
            ],
          ],
        ),
      );
    }
    final asks = [
      for (final w in needs)
        if (w.need == Need.asks) w,
    ];
    final stopped = [
      for (final w in needs)
        if (w.need == Need.failed || w.need == Need.limit) w,
    ];
    final (state, count, first, ink) = asks.isNotEmpty
        ? (AgentState.asking, '${asks.length} asking', asks.first, c.attentionText)
        : (AgentState.failed, '${stopped.length} stopped', stopped.firstOrNull, c.faultText);
    return Padding(
      padding: EdgeInsets.only(left: inset + 12, right: 8),
      child: Row(
        children: [
          AxPopover(
            width: 264,
            maxHeight: MediaQuery.sizeOf(context).height - 60,
            anchor: (context, isOpen, toggle) => AxButton.icon(
              AxIcon.list,
              label: 'Sessions',
              tone: AxTone.quiet,
              engaged: isOpen,
              onPressed: toggle,
            ),
            panel: (context, close) => open(close),
          ),
          const SizedBox(width: 8),
          if (first != null)
            Flexible(
              child: Pressable(
                label: '$count, open the first',
                onPressed: () => onAnswer(first),
                builder: (context, p) => Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    AxLamp(state),
                    const SizedBox(width: 7),
                    Flexible(
                      child: Text(
                        count,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: t.type.label.ink(ink),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          const SizedBox(width: 12),
          Expanded(flex: 3, child: said == null && first == null ? _where(t) : failed),
          const SizedBox(width: 8),
          AxButton.icon(AxIcon.search, label: 'Search', tone: AxTone.quiet, onPressed: onFind),
          AxButton.icon(
            AxIcon.settings,
            label: 'Accounts',
            tone: AxTone.quiet,
            onPressed: onAccounts,
          ),
          AxButton.icon(AxIcon.plus, label: 'New session', tone: AxTone.quiet, onPressed: onStart),
        ],
      ),
    );
  }
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
    required this.onFind,
    required this.onHide,
    required this.view,
    this.close,
  });
  final _View view;
  final VoidCallback onFind, onHide;

  /// Closes the panel it is in, where there is no room beside the display: then it is the list alone.
  final VoidCallback? close;
  final ValueChanged<Waits> onAnswer;
  final HostStore store;
  final double inset;
  final String? agent;
  final ValueChanged<String> onAgent, onOpen;

  /// In this folder, if one is named.
  final ValueChanged<String?> onStart;
  final VoidCallback onAddAgent, onAccounts;

  void _then(VoidCallback f) {
    close?.call();
    f();
  }

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
    final open = m == null
        ? null
        : ListenableBuilder(
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
                selected: view != _View.work
                    ? null
                    : m.agents.containsKey(agent)
                    ? agent
                    : m.agents.keys.firstOrNull,
                onSelect: (id) => _then(() => onAgent(id)),
                // A replay's session has nobody to work beside.
                onAddAgent: m.workspace == null ? null : () => _then(onAddAgent),
              ),
            ),
          );
    final groups = _groups();
    final list = <Widget>[
      for (final (label, all) in [
        (
          'Needs you',
          [
            for (final w in store.needs)
              if (w.need != Need.finished) w,
          ],
        ),
        (
          'Finished',
          [
            for (final w in store.needs)
              if (w.need == Need.finished) w,
          ],
        ),
      ])
        if (all.isNotEmpty) ...[
          Semantics(
            liveRegion: true,
            child: SideHeading(
              label,
              trailing: Padding(
                padding: const EdgeInsets.only(right: 6),
                child: Text('${all.length}', style: t.type.small.ink(c.ink3)),
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
              onOpen: () => _then(() => onAnswer(w)),
            ),
        ],
      for (final g in groups) ...[
        SideHeading(
          g.name,
          icon: g.folder == null ? null : AxIcon.folder,
          spoken: g.folder == null ? null : 'Folder ${g.folder}',
          trailing: g.folder == null
              ? null
              : AxButton.icon(
                  AxIcon.plus,
                  label: 'New session in ${g.name}',
                  tone: AxTone.quiet,
                  size: 24,
                  onPressed: () => _then(() => onStart(g.folder)),
                ),
        ),
        for (final x in g.sessions)
          if (x == null)
            open!
          else
            SessionRow(
              SessionView(
                id: x.id,
                title: oneLine(x.title),
                age: ageOf(x.updated),
                working: x.running,
              ),
              onOpen: () => _then(() => onOpen(x.id)),
            ),
      ],
      if (store.older)
        Padding(
          padding: const EdgeInsets.fromLTRB(8, 8, 8, 0),
          child: Text(
            'The last ${HostStore.listed}. Older ones are in the journal.',
            style: t.type.small.ink(c.ink3),
          ),
        ),
    ];
    if (close != null) {
      return Padding(
        // With what the panel has, 8 all round.
        padding: const EdgeInsets.all(3),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: list),
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SizedBox(
          height: 44,
          child: Padding(
            padding: EdgeInsets.only(left: inset == 0 ? 16 : inset + 20, right: 8),
            child: Row(
              children: [
                const AxMark(size: 16),
                const SizedBox(width: 8),
                Expanded(child: Text('Axiom', style: t.type.uiStrong)),
                AxButton.icon(
                  AxIcon.sidebar,
                  label: 'Hide the sidebar',
                  shortcut: _ShellState.keysOf('B'),
                  tone: AxTone.quiet,
                  onPressed: onHide,
                ),
              ],
            ),
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(8, 2, 8, 0),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              NavRow(
                AxIcon.plus,
                'New session',
                keys: _ShellState.keysOf('N'),
                selected: view == _View.start,
                act: true,
                onPressed: () => onStart(null),
              ),
              const SizedBox(height: 3),
              NavRow(AxIcon.search, 'Search', keys: _ShellState.keysOf('K'), onPressed: onFind),
            ],
          ),
        ),
        Expanded(
          child: ListView(padding: const EdgeInsets.fromLTRB(8, 0, 8, 12), children: list),
        ),
        const Hairline(),
        Padding(
          padding: const EdgeInsets.all(8),
          child: NavRow(
            AxIcon.settings,
            'Accounts',
            keys: _ShellState.keysOf(','),
            selected: view == _View.accounts,
            onPressed: onAccounts,
          ),
        ),
      ],
    );
  }

  /// The sessions, by folder. A session stays where it is when it is opened, and when it works: they
  /// stand in the order they were made in. The open one is null.
  List<({String name, String? folder, List<SessionInfo?> sessions})> _groups() {
    final m = store.current;
    const unlisted = 1 << 62;
    final all = <(String?, int, SessionInfo?)>[
      for (final x in store.sessions) (x.workspace, x.created, x.id == m?.id ? null : x),
    ];
    if (m != null && !store.sessions.any((x) => x.id == m.id)) {
      all.add((m.workspace, unlisted, null));
    }
    mergeSort(all, compare: (a, b) => b.$2.compareTo(a.$2));
    final by = <String?, List<SessionInfo?>>{};
    for (final (folder, _, x) in all) {
      (by[folder == null || folder.isEmpty ? null : folder] ??= []).add(x);
    }
    final names = <String, int>{};
    for (final f in by.keys.nonNulls) {
      names.update(folderName(f), (n) => n + 1, ifAbsent: () => 1);
    }
    // Two folders of one name are told apart by where they are.
    String called(String f) {
      final name = folderName(f);
      if (names[name] == 1) return name;
      final parts = f.split(RegExp(r'[/\\]')).where((x) => x.isNotEmpty).toList();
      return parts.length < 2
          ? name
          : oneLine(literal(parts.sublist(parts.length - 2).join('/')), 80);
    }

    return [
      for (final MapEntry(key: f, value: v) in by.entries)
        (
          name: f == null ? (by.length == 1 ? 'Sessions' : 'No folder') : called(f),
          folder: f,
          sessions: v,
        ),
    ];
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
    required this.onAgent,
    required this.changes,
    required this.panel,
  });
  final FolderChanges changes;

  /// There is room for the panel, and it is wanted.
  final bool panel;
  final ValueChanged<String> onAgent;
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
        ListenableBuilder(
          listenable: Listenable.merge([a, changes]),
          builder: (context, _) => _bar(context, a),
        ),
        const Hairline(),
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
                    changes,
                    agent: a.name,
                    draft: drafts.of(session.id, a.id),
                    key: ValueKey(session.id),
                  ),
          )
        else
          Expanded(
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Expanded(
                  child: LayoutBuilder(
                    builder: (context, box) => Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        Expanded(
                          child: ListenableBuilder(
                            listenable: a,
                            builder: (context, full) => a.last != null || a.ended != null
                                ? full!
                                : Padding(
                                    padding: EdgeInsets.fromLTRB(
                                      AxSpace.beside(box.maxWidth),
                                      20,
                                      AxSpace.gutter,
                                      0,
                                    ),
                                    child: Text(
                                      'No messages yet.',
                                      style: Ax.of(context).type.ui.ink(c.ink3),
                                    ),
                                  ),
                            child: TranscriptView(a),
                          ),
                        ),
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
                                      onAgent: onAgent,
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
                if (panel && acts && !a.apart) _Panel(changes, onOpen: () => onTab(1)),
              ],
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
      tabs: [
        ('Conversation', null),
        if (acts)
          ('Changes', !a.apart && changes.files.isNotEmpty ? '${changes.files.length}' : null),
      ],
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
            AxButton(label: 'Stop', icon: AxIcon.stop, onPressed: () => onStop(run)),
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

/// What has changed in the folder, beside the conversation. There is none where nothing has.
class _Panel extends StatefulWidget {
  const _Panel(this.changes, {required this.onOpen});
  final FolderChanges changes;
  final VoidCallback onOpen;
  @override
  State<_Panel> createState() => _PanelState();
}

class _PanelState extends State<_Panel> {
  @override
  void initState() {
    super.initState();
    unawaited(widget.changes.lookOnce());
  }

  @override
  void didUpdateWidget(_Panel old) {
    super.didUpdateWidget(old);
    if (!identical(old.changes, widget.changes)) unawaited(widget.changes.lookOnce());
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: widget.changes,
    builder: (context, _) {
      final files = widget.changes.files;
      if (files.isEmpty) return const SizedBox.shrink();
      int sum(int Function(FileDiff) of) => files.fold(0, (n, f) => n + of(f.$2));
      return SidePanel(
        groups: [
          (
            'Changed files',
            ChangedBy(sum((f) => f.added), sum((f) => f.removed)),
            [
              for (final (f, d) in files)
                ChangedRow(
                  oneLine(literal(f.path), 300),
                  added: d.added,
                  removed: d.removed,
                  onOpen: widget.onOpen,
                ),
            ],
          ),
        ],
      );
    },
  );
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
      child: LayoutBuilder(
        builder: (context, box) => SingleChildScrollView(
          padding: EdgeInsets.fromLTRB(
            AxSpace.beside(box.maxWidth),
            10,
            AxSpace.beside(box.maxWidth),
            12,
          ),
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
