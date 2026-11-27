import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../host/host_store.dart';
import '../host/proto.dart';
import '../host/session_model.dart';
import '../patterns/agent_bar.dart';
import '../patterns/beside.dart';
import '../patterns/context_gauge.dart';
import '../patterns/model_name.dart';
import '../patterns/models.dart';
import '../patterns/session_panel.dart';
import '../transcript/find_here.dart';
import '../transcript/safe_text.dart';
import '../transcript/inline.dart';
import '../transcript/transcript_view.dart';
import '../ui/commands.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/overlay.dart';
import '../ui/prefs.dart';
import '../ui/press.dart';
import '../ui/status.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';
import '../ui/zoom.dart';
import 'changes.dart';
import 'drafts.dart';
import 'drop.dart';
import 'find.dart';
import 'folder.dart';
import 'new_session.dart';
import 'settings.dart';
import 'start.dart';
import 'talk.dart';
import 'window.dart';

/// The application's window, on what the host says. Nothing here is specimen data.
class Shell extends StatefulWidget {
  const Shell(this.store, {super.key, this.titleInset = 0, this.drags});
  final HostStore store;

  /// Folders that are dropped on the window.
  final Drags? drags;

  /// Room for the window's own buttons.
  final double titleInset;
  @override
  State<Shell> createState() => _ShellState();
}

/// What the display shows.
enum _View { work, start, agent, accounts, settings }

class _ShellState extends State<Shell> with WidgetsBindingObserver {
  String? _agent;
  String? _session;

  /// An agent of a session that is being opened.
  (String, String)? _wanted;

  /// Where it is asked is where it is answered.
  void _answer(Waits w) {
    setState(() {
      _view = _View.work;
      // Where the panel would be in front of it.
      if (MediaQuery.sizeOf(context).width < 900) _changing = false;
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

  /// The panel beside the conversation, as its person wants it, and how wide; null: where there is
  /// room, if there is anything in it.
  bool? _changing;
  (double, double) _sizes = Beside.usual;

  /// Who asks for it sees what there is now.
  void _change(bool v) {
    if (v && _changes?.all != null) unawaited(_changes!.look());
    setState(() => _changing = v);
  }

  final _conversation = GlobalKey();

  /// Whether the panel is shown; null where there is none.
  bool? _panel(BuildContext context) {
    final m = widget.store.current;
    final a = m?.agents[_agent] ?? m?.agents.values.firstOrNull;
    if (m == null || a?.access == null || _view != _View.work || m.fault != null) return null;
    // Of itself where there is room, if there is anything in it.
    return _changing ??
        (MediaQuery.sizeOf(context).width >= _roomy &&
            !a!.apart &&
            (a.plans || _changesOf(m).files.isNotEmpty));
  }

  static const _roomy = 1180.0;
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

  /// What a key was pressed for and did not come about, or what is to be known of what did.
  String? _failed;
  bool _well = false;
  Timer? _forget;

  void _show(_View v) => setState(() => _view = v);

  /// The list beside the display, as its person wants it.
  bool _list = true;
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

  /// What is read, found or searched for in the open session's folder.
  FolderLook? _look;
  FolderLook _lookOf(SessionModel m) {
    if (_look?.session != m.id) {
      // What is on the screen still listens to the one before, until the next frame.
      final was = _look;
      if (was != null) WidgetsBinding.instance.addPostFrameCallback((_) => was.dispose());
      _look = FolderLook(widget.store, m.id);
    }
    return _look!;
  }

  /// The panel shows a file, a search or the box to find one in.
  void _inFolder(void Function(FolderLook) f) {
    final m = widget.store.current;
    if (m == null || m.workspace == null) return;
    final a = m.agents[_agent] ?? m.agents.values.firstOrNull;
    final l = _lookOf(m)..agent = a?.apart ?? false ? a!.id : null;
    f(l);
    _change(true);
  }

  /// Back from a page to what was shown before it.
  void _back() => _show(_View.work);

  void _startIn(String? folder) {
    // Of what is open, unless something is being written already.
    final at = folder ?? (_start.say.text.trim().isEmpty ? widget.store.current?.workspace : null);
    if (at != null) _start.folder.text = at;
    _show(_View.start);
  }

  final _commands = Commands();

  /// Another session: the one so far from the open one in the list, around its end.
  void _by(int d) {
    final all = sessionOrder(widget.store);
    if (all.isEmpty) return;
    final at = all.indexOf(widget.store.current?.id ?? '');
    _open(all[at < 0 ? (d > 0 ? 0 : all.length - 1) : (at + d) % all.length]);
  }

  /// What has waited longest after the one that is shown.
  void _nextRequest() {
    final all = widget.store.needs.where((w) => w.need == Need.asks).toList();
    if (all.isEmpty) return;
    final m = widget.store.current;
    final agent = _agent ?? m?.agents.keys.firstOrNull;
    final at = all.lastIndexWhere((w) => w.session == m?.id && w.agent == agent);
    _answer(all[(at + 1) % all.length]);
  }

  void _prefs(Prefs Function(Prefs) f) {
    final all = PrefsScope.maybeOf(context);
    if (all != null) all.value = f(all.value);
  }

  bool _run(Command c) {
    switch (c) {
      case Command.start:
        _startIn(null);
      case Command.find:
        setState(() => _finding = !_finding);
      case Command.accounts:
        _show(_View.accounts);
      case Command.settings:
        _show(_View.settings);
      case Command.zoomIn:
        _prefs((p) => p.zoomed(1));
      case Command.zoomOut:
        _prefs((p) => p.zoomed(-1));
      case Command.zoomReset:
        _prefs((p) => p.copyWith(zoom: 1));
      case Command.larger:
        _prefs((p) => p.sized(1));
      case Command.smaller:
        _prefs((p) => p.sized(-1));
      case Command.usual:
        _prefs((p) => p.copyWith(text: 1));
      case Command.panel:
        if (_panel(context) case final open?) _change(!open);
      case Command.list:
        setState(() => _list = !_list);
      case Command.goToFile:
        _inFolder((l) => l.find(!l.finding));
      case Command.searchFiles:
        _inFolder((l) => l.search(!l.searching));
      case Command.next:
        _by(1);
      case Command.previous:
        _by(-1);
      case Command.request:
        _nextRequest();
      case Command.area:
        _commands.area();
      case Command.areaBack:
        _commands.area(back: true);
      case Command.stop ||
          Command.findHere ||
          Command.model ||
          Command.access ||
          Command.allow ||
          Command.deny:
        // Of what is on the screen. Nothing there: it is nobody's.
        return _commands.run(c);
    }
    return true;
  }

  /// The window's own keys, wherever the keyboard is.
  bool _key(KeyEvent e) {
    if (Command.place(e) case final n? when !_finding) {
      final all = sessionOrder(widget.store);
      if (n > all.length) return false;
      _open(all[n - 1]);
      return true;
    }
    final c = Command.of(e);
    if (c == null) return false;
    // Nothing is decided on from under the search.
    if (_finding && c != Command.find) return false;
    return _run(c);
  }

  List<Found> _everything(HostStore s) => [
    for (final (c, icon) in [
      (Command.start, AxIcon.plus),
      (Command.accounts, AxIcon.account),
      (Command.settings, AxIcon.list),
    ])
      Found(c.label, () => _run(c), icon: icon, keys: c.keys),
    if (s.current case final m?) ...[
      if (m.workspace != null)
        Found(
          'Add an agent',
          () => _addAgent(m.id),
          detail: oneLine(m.title, 60),
          icon: AxIcon.plus,
        ),
      if (m.agents[_agent] ?? m.agents.values.firstOrNull case final a?
          when a.access != null &&
              a.execution.harness == 'axiom' &&
              a.run == null &&
              a.last != null)
        Found(
          'Summarise the conversation to make room',
          () => _do('The conversation was not summarised', () => widget.store.compact(m.id, a.id)),
          detail: oneLine(literal(a.name), 40),
          icon: AxIcon.list,
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
    // A yes is said where the question is, not from a list.
    for (final c in Command.values)
      if (switch (c) {
        Command.panel => _panel(context) != null,
        Command.goToFile || Command.searchFiles => s.current?.workspace != null,
        Command.list ||
        Command.zoomIn ||
        Command.zoomOut ||
        Command.zoomReset ||
        Command.larger ||
        Command.smaller ||
        Command.usual => true,
        Command.next || Command.previous => s.sessions.length > 1,
        Command.request => s.needs.any((w) => w.need == Need.asks),
        Command.stop || Command.findHere || Command.model || Command.access => _commands.can(c),
        _ => false,
      })
        Found(c.label, () => _run(c), icon: AxIcon.more, keys: c.keys),
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

  void _say(String? failed, {bool well = false}) {
    _forget?.cancel();
    if (!mounted) return;
    setState(() {
      _failed = failed;
      _well = well;
    });
    if (failed != null) _forget = Timer(const Duration(seconds: 15), () => _say(null));
  }

  /// A folder that was asked for from outside: a session could begin there, and nothing does.
  void _folder(String path) {
    if (!mounted) return;
    setState(() => _finding = false);
    _startIn(path);
  }

  /// The folder that is held over the window.
  String? _over;
  void _held(String? folder) => mounted ? setState(() => _over = folder) : null;

  int _drops = 0;
  Future<void> _dropped(List<String> paths) async {
    final n = ++_drops;
    _held(null);
    final d = await whatWasDropped(paths);
    // Another has been dropped meanwhile.
    if (!mounted || n != _drops) return;
    if (d.folder case final f?) _folder(f);
    _say(d.words, well: d.folder != null);
  }

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    HardwareKeyboard.instance.addHandler(_key);
    widget.store.onFolder = _folder;
    widget.drags?.listen(
      over: _held,
      out: () => _held(null),
      dropped: (paths) => unawaited(_dropped(paths)),
    );
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
    widget.store.onFolder = null;
    widget.drags?.stop();
    _changes?.dispose();
    _look?.dispose();
    _forget?.cancel();
    _drafts.dispose();
    _start.dispose();
    _more?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return CommandScope(
      _commands,
      child: ColoredBox(
        color: c.chassis.fill,
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
            // With nothing to go back to, one begins.
            final view =
                _view == _View.work &&
                    s.phase == HostPhase.connected &&
                    s.current == null &&
                    s.sessions.isEmpty
                ? _View.start
                : _view;
            final width = MediaQuery.sizeOf(context).width;
            // The window's own buttons stay as large as they are.
            final inset = widget.titleInset / Zoom.of(context);
            final narrow = width < 860, beside = !narrow && _list;
            Widget panelKey() => ListenableBuilder(
              listenable: Listenable.merge([
                if (s.current case final m?) ...[m, _changesOf(m), ...m.agents.values],
              ]),
              builder: (context, _) {
                final open = _panel(context);
                return PanelKey(
                  open: open ?? false,
                  shortcut: Command.panel.keys,
                  onPressed: open == null ? null : () => _change(!open),
                );
              },
            );
            Widget side([VoidCallback? close]) => _Side(
              store: s,
              inset: inset,
              agent: _agent,
              view: view,
              close: close,
              onAgent: (id) => setState(() {
                _agent = id;
                _view = _View.work;
              }),
              onAddAgent: () => _addAgent(s.current!.id),
              onStart: _startIn,
              onFind: () => setState(() => _finding = true),
              onSettings: () => _show(_View.settings),
              onAccounts: () => _show(_View.accounts),
              onHide: () => setState(() => _list = false),
              onAnswer: _answer,
              onOpen: _open,
            );
            final page = switch (view) {
              _View.start => 'New session',
              _View.agent => 'New agent',
              _View.accounts => 'Accounts',
              _View.settings => 'Settings',
              _View.work => null,
            };
            // What is said of the host goes before anything else.
            final (problem, well) = switch (s) {
              _ when stale && s.phase != HostPhase.starting => (
                s.binary == null && s.current == null
                    ? 'axiom-host was not found beside the application'
                    : 'The host is not answering. Trying again.',
                false,
              ),
              HostStore(:final damaged?) => (
                'The journal is damaged: ${oneLine(damaged, 200)}',
                false,
              ),
              _ => (_failed == null ? null : oneLine(_failed!, 200), _well),
            };
            final onList = beside || narrow ? null : () => setState(() => _list = true);
            final banner = problem == null
                ? null
                : _Banner(
                    problem,
                    well: well,
                    inset: beside ? AxSpace.gutter : inset + AxSpace.edge,
                  );
            final content = switch (s) {
              // Kept while the host is away: what was written is not lost.
              _ when view == _View.start => StartPage(
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
              _ when _view == _View.settings => SettingsPage(limits: widget.store.limits),
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
                look: _lookOf(m),
                onPath: (path, line, agent) =>
                    _inFolder((l) => unawaited(l.read(path, line: line, agent: agent))),
                onClose: () => _change(false),
                panelKey: panelKey,
                open: () => _panel(context) ?? false,
                sizes: _sizes,
                onSizes: (v) => setState(() => _sizes = v),
                conversation: _conversation,
                looks: width >= _roomy,
                banner: banner,
                onList: onList,
                inset: beside ? AxSpace.gutter : inset + AxSpace.edge,
                alone: !beside && !narrow,
                small: narrow,
                onAgain: () => _do('The session could not be opened', () => s.open(m.id)),
                onStop: (run) => _do('It could not be stopped', () => s.cancel(m.id, run)),
                onAgent: (id) => setState(() => _agent = id),
              ),
              _ => _Word('Choose a session', action: ('New session', () => _startIn(null))),
            };
            final window = Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                if (beside)
                  SizedBox(
                    width: width < 1080 ? 228 : 264,
                    child: CommandArea(0, child: FocusTraversalGroup(child: side())),
                  ),
                Expanded(
                  // Kept when the window is made wider or narrower.
                  key: const ValueKey('display'),
                  child: FocusTraversalGroup(
                    child: _DropPlace(
                      _over,
                      padding: EdgeInsets.fromLTRB(beside ? 0 : 8, 8, 8, 8),
                      child: page == null && s.current != null && !narrow
                          ? content
                          : AxCanvas(
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.stretch,
                                children: [
                                  if (!narrow) ?banner,
                                  MoveArea(
                                    child: Container(
                                      height: 44,
                                      padding: const EdgeInsets.only(bottom: 8),
                                      child: _Strip(
                                        problem: narrow ? problem : null,
                                        well: well,
                                        page: page,
                                        session: s.current,
                                        inset: beside ? 0 : inset,
                                        needs: s.needs,
                                        list: narrow ? side : null,
                                        onList: onList,
                                        onAnswer: _answer,
                                        onStart: () => _startIn(null),
                                        onLeave: switch (_view) {
                                          _View.start when s.current != null => _back,
                                          _View.agent ||
                                          _View.settings when s.current != null => _back,
                                          _ => null,
                                        },
                                        onAccounts: () => _show(_View.accounts),
                                        onFind: () => setState(() => _finding = true),
                                      ),
                                    ),
                                  ),
                                  Expanded(
                                    child: page == null ? content : CommandArea(1, child: content),
                                  ),
                                ],
                              ),
                            ),
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
                    child: GivesFocusBack(
                      child: SearchBox(
                        all: _everything(s),
                        onClose: () => setState(() => _finding = false),
                      ),
                    ),
                  ),
              ],
            );
          },
        ),
      ),
    );
  }
}

/// What the window shows; while a folder is held over the window, where it would go.
class _DropPlace extends StatelessWidget {
  const _DropPlace(this.folder, {required this.padding, required this.child});
  final String? folder;
  final EdgeInsets padding;
  final Widget child;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Stack(
      fit: StackFit.expand,
      children: [
        Padding(padding: padding, child: child),
        IgnorePointer(
          child: AnimatedSwitcher(
            duration: reducedMotion(context) ? Duration.zero : AxMotion.fade,
            child: folder == null
                ? const SizedBox.expand()
                : Padding(
                    padding: padding,
                    child: AxCanvas(
                      child: Center(
                        child: Semantics(
                          liveRegion: true,
                          child: Padding(
                            padding: const EdgeInsets.all(AxSpace.gutter),
                            child: Column(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                Text(
                                  'New session in ${folderName(folder!)}',
                                  maxLines: 2,
                                  overflow: TextOverflow.ellipsis,
                                  textAlign: TextAlign.center,
                                  style: t.type.heading,
                                ),
                                const SizedBox(height: 6),
                                Text(
                                  'Drop the folder here. Nothing starts by itself.',
                                  textAlign: TextAlign.center,
                                  style: t.type.label.ink(c.ink3),
                                ),
                              ],
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
          ),
        ),
      ],
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

/// What its model is called.
String? modelOf(Execution x, HostStore s) {
  final id = x.model, on = x.connection;
  if (id == null) return null;
  for (final m in s.modelsOf(on ?? '') ?? const <ModelInfo>[]) {
    if (m.id == id) return modelName(id, m.name);
  }
  return modelName(id);
}

/// What the account it uses is called.
String? _accountOf(Execution x, HostStore s) {
  final name = s.connections.where((c) => c.id == x.connection).firstOrNull?.name;
  return name == null ? null : oneLine(name, 60);
}

AgentView viewOf(AgentModel a, HostStore s) {
  return AgentView(
    id: a.id,
    name: a.name,
    state: stateOf(a),
    // What chooses its model itself is named instead.
    model: oneLine(
      modelOf(a.execution, s) ??
          (a.execution.harness == 'axiom' ? '' : s.harnessName(a.execution.harness)),
      60,
    ),
    harness: s.harnessName(a.execution.harness),
    service: markOf(a.execution.provider, a.execution.model, a.execution.harness),
    connection: _accountOf(a.execution, s) ?? '',
    elapsed: '',
    context: a.context,
    contextProvenance: provenanceOf(a.contextKnown),
    recent: [?_activity(a, s)],
  );
}

/// Where one is: the folder, the session, and what stands in front of it.
class Where extends StatelessWidget {
  const Where(this.session, {super.key, this.then, this.quiet = false});
  final SessionModel session;
  final String? then;

  /// Beside something that is more to the point.
  final bool quiet;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors, m = session;
    return ListenableBuilder(
      listenable: m,
      builder: (_, _) {
        final at = m.workspace;
        final last = quiet ? t.type.ui.ink(c.ink3) : t.type.uiStrong.copyWith(fontSize: 14);
        Widget between() => Padding(
          padding: const EdgeInsets.symmetric(horizontal: 7),
          child: ExcludeSemantics(child: Text('/', style: t.type.ui.ink(c.ink4))),
        );
        Widget part(String words, TextStyle style) => Flexible(
          child: Text(words, maxLines: 1, overflow: TextOverflow.ellipsis, style: style),
        );
        return Row(
          mainAxisSize: quiet ? MainAxisSize.min : MainAxisSize.max,
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
            part(m.title.isEmpty ? 'Session' : oneLine(m.title, 120), last),
            if (then case final p?) ...[between(), part(p, last)],
          ],
        );
      },
    );
  }
}

/// What has failed, over everything else.
class _Banner extends StatelessWidget {
  const _Banner(this.words, {required this.inset, required this.well});
  final String words;

  /// Nothing has failed: it is only to be known.
  final bool well;
  final double inset;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Padding(
      padding: EdgeInsets.fromLTRB(inset, 14, AxSpace.gutter, 0),
      child: Row(
        children: [
          if (well) AxGlyph(AxIcon.folder, color: c.ink3) else const AxLamp(AgentState.failed),
          const SizedBox(width: 10),
          Expanded(
            child: Semantics(
              liveRegion: true,
              child: Text(
                words,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: t.type.label.ink(well ? c.ink2 : c.faultText),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// What stands over the display: where one is, or what has failed, and, where the window is too narrow
/// for the list beside it, the way to the list.
class _Strip extends StatelessWidget {
  const _Strip({
    required this.problem,
    required this.well,
    required this.page,
    required this.session,
    required this.inset,
    required this.needs,
    required this.list,
    required this.onList,
    required this.onAnswer,
    required this.onStart,
    required this.onAccounts,
    required this.onFind,
    this.onLeave,
  });

  /// Leaves the page that is shown, if it can be left as it is.
  final VoidCallback? onLeave;
  final String? problem;
  final bool well;

  /// The page that is shown, if it is not a session's.
  final String? page;
  final SessionModel? session;
  final double inset;
  final List<Waits> needs;
  final Widget Function(VoidCallback close)? list;

  /// Brings back the list, where it was put away.
  final VoidCallback? onList;

  final ValueChanged<Waits> onAnswer;
  final VoidCallback onStart, onAccounts, onFind;

  Widget _where(AxTheme t) {
    final m = session;
    final strong = t.type.uiStrong.copyWith(fontSize: 14);
    // It says itself what it is.
    if (page == 'New session') return const SizedBox.shrink();
    if (page case final p? when p != 'New agent') {
      return Semantics(header: true, child: Text(p, style: strong));
    }
    if (m == null) return const SizedBox.shrink();
    return Where(m, then: page);
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
              style: t.type.label.ink(well ? c.ink2 : c.faultText),
            ),
          );
    final failed = Row(
      children: [
        if (said != null) ...[
          if (well) AxGlyph(AxIcon.folder, color: c.ink3) else const AxLamp(AgentState.failed),
          const SizedBox(width: 10),
          Expanded(child: said),
        ],
      ],
    );
    final open = list;
    if (open == null) {
      return Padding(
        padding: EdgeInsets.only(
          left: onList == null ? AxSpace.gutter : inset + AxSpace.edge,
          right: AxSpace.edge,
        ),
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
            if (onLeave case final f?)
              AxButton.icon(AxIcon.close, label: 'Cancel', tone: AxTone.quiet, onPressed: f),
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
      padding: EdgeInsets.only(left: inset + AxSpace.edge, right: AxSpace.edge),
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
            AxIcon.account,
            label: 'Accounts',
            tone: AxTone.quiet,
            onPressed: onAccounts,
          ),
          if (onLeave case final f?)
            AxButton.icon(AxIcon.close, label: 'Cancel', tone: AxTone.quiet, onPressed: f)
          else
            AxButton.icon(
              AxIcon.plus,
              label: 'New session',
              tone: AxTone.quiet,
              onPressed: onStart,
            ),
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
    required this.onSettings,
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
  final VoidCallback onSettings, onAddAgent, onAccounts;

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
                padding: const EdgeInsets.only(right: 8),
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
      for (final (i, g) in groups.indexed) ...[
        SideHeading(
          g.name,
          first: close != null && i == 0 && store.needs.isEmpty,
          spoken: g.folder == null ? null : 'Folder ${g.folder}',
          act: g.folder == null
              ? null
              : AxButton.icon(
                  AxIcon.plus,
                  label: 'New session in ${g.name}',
                  tone: AxTone.quiet,
                  size: 28,
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
          padding: const EdgeInsets.fromLTRB(AxSpace.row, 8, AxSpace.row, 0),
          child: Text(
            'The last ${HostStore.listed}. Older ones are in the journal.',
            style: t.type.small.ink(c.ink3),
          ),
        ),
    ];
    if (close != null) {
      return Padding(
        // With what the panel has, 8 all round.
        padding: const EdgeInsets.all(2),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: list),
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        MoveArea(
          child: SizedBox(
            height: 52,
            child: Padding(
              // On the line of what stands on the canvas.
              padding: EdgeInsets.only(left: inset == 0 ? 20 : inset + 20, right: 10),
              child: Row(
                children: [
                  const AxMark(size: 18),
                  const SizedBox(width: 9),
                  Expanded(
                    child: Text(
                      'Axiom',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: t.type.uiStrong.copyWith(fontSize: 14),
                    ),
                  ),
                  AxButton.icon(
                    AxIcon.sidebar,
                    label: 'Hide the sidebar',
                    shortcut: Command.list.keys,
                    tone: AxTone.quiet,
                    iconFilled: true,
                    onPressed: onHide,
                  ),
                ],
              ),
            ),
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(10, 0, 10, 0),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              NavRow(
                AxIcon.plus,
                'New session',
                keys: Command.start.keys,
                selected: view == _View.start,
                act: true,
                onPressed: () => onStart(null),
              ),
              const SizedBox(height: 3),
              NavRow(AxIcon.search, 'Search', keys: Command.find.keys, onPressed: onFind),
            ],
          ),
        ),
        Expanded(
          child: Stack(
            children: [
              ListView(padding: const EdgeInsets.fromLTRB(10, 0, 10, 20), children: list),
              Positioned(
                left: 0,
                right: 0,
                bottom: 0,
                child: Fade(up: false, height: 20, ground: c.chassis.fill),
              ),
            ],
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(10, 6, 10, 8),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              NavRow(
                AxIcon.list,
                'Settings',
                keys: Command.settings.keys,
                selected: view == _View.settings,
                onPressed: onSettings,
              ),
              const SizedBox(height: 3),
              NavRow(
                AxIcon.account,
                'Accounts',
                keys: Command.accounts.keys,
                selected: view == _View.accounts,
                onPressed: onAccounts,
              ),
            ],
          ),
        ),
      ],
    );
  }

  List<({String name, String? folder, List<SessionInfo?> sessions})> _groups() => _groupsOf(store);
}

/// The sessions, by folder. A session stays where it is when it is opened, and when it works: they
/// stand in the order they were made in. The open one is null.
List<({String name, String? folder, List<SessionInfo?> sessions})> _groupsOf(HostStore store) {
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

/// The sessions as the list has them, from the top.
List<String> sessionOrder(HostStore store) => [
  for (final g in _groupsOf(store))
    for (final x in g.sessions) ?(x?.id ?? store.current?.id),
];

class _Display extends StatelessWidget {
  const _Display(
    this.store,
    this.session, {
    required this.agent,
    required this.stale,
    required this.drafts,
    required this.onAgain,
    required this.onStop,
    required this.onAgent,
    required this.changes,
    required this.look,
    required this.onPath,
    required this.open,
    required this.onClose,
    required this.panelKey,
    required this.sizes,
    required this.onSizes,
    required this.conversation,
    required this.looks,
    required this.banner,
    required this.onList,
    required this.inset,
    required this.alone,
    required this.small,
  });

  /// Whether the panel is shown, now.
  final bool Function() open;
  final VoidCallback onClose;
  final Widget Function() panelKey;
  final (double, double) sizes;
  final ValueChanged<(double, double)> onSizes;

  /// Keeps what is being read and written when the window is made wider or narrower.
  final GlobalKey conversation;

  /// How much has changed is looked at without being asked.
  final bool looks;
  final Widget? banner;
  final VoidCallback? onList;
  final double inset;

  /// The list is put away, so where one is has to be said; the window is small, and it is said above.
  final bool alone, small;
  final FolderChanges changes;
  final FolderLook look;

  /// A path in an answer was pressed: of the folder, or of the copy of the agent named.
  final void Function(String path, int? line, String? agent) onPath;
  final ValueChanged<String> onAgent;
  final HostStore store;
  final Drafts drafts;
  final ValueChanged<String> onStop;
  final SessionModel session;
  final String? agent;
  final bool stale;
  final VoidCallback onAgain;

  /// What is written next to an agent goes to a branch of it.
  Future<void> _branchAt(AgentModel a, ItemModel i) async {
    final d = drafts.of(session.id, a.id)..problem.value = null;
    try {
      final info = await store.lookBranch(session.id, a.id, i.id);
      d.branchAt(
        Branching(
          i.id,
          apart: info.apart,
          files: info.files,
          words: i.kind == ItemKind.prompt ? i.source : null,
        ),
      );
    } on Object catch (e) {
      d.problem.value = wordsOf(e);
    }
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: session,
    builder: (context, _) {
      final a = session.agents[agent] ?? session.agents.values.firstOrNull;
      if (session.fault case final f?) {
        return _bare(
          _Word('This session cannot be shown', detail: f, action: ('Try again', onAgain)),
        );
      }
      if (!session.loaded) {
        return _bare(_Word(stale ? 'Waiting for the host' : 'Reading the session'));
      }
      if (a == null) return _bare(const _Word('No agent has worked here yet'));
      return _work(context, a);
    },
  );

  /// With nothing to show: the way back to the list stays.
  Widget _bare(Widget word) {
    final all = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        ?banner,
        if (!small)
          MoveArea(
            child: SizedBox(
              height: 44,
              child: Padding(
                padding: EdgeInsets.only(left: inset, right: AxSpace.edge, bottom: 8),
                child: Row(
                  children: [
                    if (onList case final f?) ...[
                      AxButton.icon(
                        AxIcon.sidebar,
                        label: 'Show the sidebar',
                        tone: AxTone.quiet,
                        onPressed: f,
                      ),
                      const SizedBox(width: 12),
                    ],
                    Expanded(child: Where(session)),
                  ],
                ),
              ),
            ),
          ),
        Expanded(child: word),
      ],
    );
    return small ? all : AxCanvas(child: all);
  }

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
    if (looks && acts && !a.apart) {
      WidgetsBinding.instance.addPostFrameCallback((_) => unawaited(changes.lookOnce()));
    }
    final talk = KeyedSubtree(
      key: conversation,
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
                        child: Text('No messages yet.', style: Ax.of(context).type.ui.ink(c.ink3)),
                      ),
                child: CommandArea(
                  1,
                  child: FindHere(
                    a,
                    builder: (view) => TranscriptView(
                      a,
                      key: view,
                      takes: true,
                      onLink: session.workspace == null
                          ? null
                          : (url) {
                              if (pathIn(url) case (final path, final line)) {
                                onPath(path, line, a.apart ? a.id : null);
                              }
                            },
                      onQuote: acts ? (q) => drafts.of(session.id, a.id).add(q) : null,
                      onBranch: acts && !stale ? (i) => unawaited(_branchAt(a, i)) : null,
                    ),
                  ),
                ),
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
                        builder: (context, _) => CommandArea(
                          2,
                          child: Talk(
                            store,
                            session,
                            a,
                            draft: drafts.of(session.id, a.id),
                            drafts: drafts,
                            live: !stale,
                            onAgent: onAgent,
                            onStop: switch (a.run) {
                              final run?
                                  when !stale &&
                                      (a.phase == AgentPhase.running ||
                                          a.phase == AgentPhase.asking) =>
                                () => onStop(run),
                              _ => null,
                            },
                            // What is asked stays in sight beside its keys.
                            room: box.maxHeight * .35,
                            key: ValueKey('${session.id} ${a.id}'),
                          ),
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
    final all = ListenableBuilder(
      listenable: Listenable.merge([changes, a, look]),
      child: talk,
      builder: (context, talk) {
        final names = {for (final x in session.agents.values) x.id: x.name};
        final lead = [
          if (a.plans) AgentPlan(a, key: ValueKey((a.id, 'plan'))),
          if (look.finding) FileFinder(look, key: ValueKey((session.id, 'find'))),
          JobsPart(store, session.id, names: names, key: ValueKey((session.id, 'jobs'))),
        ];
        final tail = [FileTree(look, key: ValueKey((session.id, a.apart ? a.id : '', 'tree')))];
        final reading = look.reads || look.searching;
        final panel = !open()
            ? null
            : CallbackShortcuts(
                bindings: {
                  const SingleActivator(LogicalKeyboardKey.escape): reading
                      ? () => look.reads ? look.close() : look.search(false)
                      : onClose,
                },
                child: CommandArea(
                  3,
                  child: FocusTraversalGroup(
                    child: look.reads
                        ? FileReader(
                            look,
                            draft: drafts.of(session.id, a.id),
                            key: ValueKey((session.id, 'read')),
                          )
                        : look.searching
                        ? FileSearch(look, key: ValueKey((session.id, 'search')))
                        : a.apart
                        ? CopyView(
                            store,
                            session,
                            a,
                            draft: drafts.of(session.id, a.id),
                            lead: lead,
                            tail: tail,
                            onRead: (p) => onPath(p, null, a.id),
                            key: ValueKey((session.id, a.id)),
                          )
                        : ChangesView(
                            changes,
                            agent: a.name,
                            draft: drafts.of(session.id, a.id),
                            lead: lead,
                            tail: tail,
                            onRead: (p) => onPath(p, null, null),
                            key: ValueKey(session.id),
                          ),
                  ),
                ),
              );
        return Beside(
          main: talk!,
          panel: panel,
          // What is in a copy is read before it is applied.
          wide: a.apart || changes.reads || reading,
          sizes: sizes,
          onSizes: onSizes,
        );
      },
    );
    final work = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (!small) ?banner,
        // The transcript between these two looks after itself.
        ListenableBuilder(
          listenable: Listenable.merge([a, changes]),
          builder: (context, _) => MoveArea(child: _bar(context, a)),
        ),
        Expanded(child: all),
      ],
    );
    return small ? work : AxCanvas(child: work);
  }

  Widget _bar(BuildContext context, AgentModel a) {
    final t = Ax.of(context), c = t.colors;
    final x = a.execution;
    final acts = a.access != null;
    return AgentBar(
      name: a.name,
      state: stateOf(a),
      mark: markOf(x.provider, x.model, x.harness),
      inset: small ? AxSpace.gutter : inset,
      height: small ? 40 : 44,
      said: !acts,
      lead: onList == null
          ? null
          : Padding(
              padding: const EdgeInsets.only(right: 12),
              child: AxButton.icon(
                AxIcon.sidebar,
                label: 'Show the sidebar',
                tone: AxTone.quiet,
                onPressed: onList,
              ),
            ),
      where: alone ? Where(session, quiet: true) : null,
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (!acts)
            Flexible(
              child: Text(
                [
                  for (final p in [
                    modelOf(x, store),
                    store.harnessName(x.harness),
                    _accountOf(x, store),
                  ])
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
              ),
            ),
          ],
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
          if (acts) ...[const SizedBox(width: 6), panelKey()],
        ],
      ),
    );
  }
}

/// What an agent says that it means to do.
class AgentPlan extends StatefulWidget {
  const AgentPlan(this.agent, {super.key});
  final AgentModel agent;
  @override
  State<AgentPlan> createState() => _AgentPlanState();
}

class _AgentPlanState extends State<AgentPlan> {
  bool _folded = false;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors, a = widget.agent;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (a.steps.isEmpty)
          PanelHeading(
            'Plan',
            null,
            folded: _folded,
            onFold: () => setState(() => _folded = !_folded),
          )
        else
          PlanView(
            [
              for (final s in a.steps)
                TaskView(oneLine(literal(s.text), 200), switch (s.state) {
                  TodoState.done => TaskState.done,
                  // A run that has ended does nothing any more, whatever it said last.
                  TodoState.doing when a.run != null => TaskState.active,
                  _ => TaskState.todo,
                }),
            ],
            folded: _folded,
            onFold: () => setState(() => _folded = !_folded),
          ),
        if (a.stepsText case final words? when !_folded)
          Padding(
            padding: const EdgeInsets.fromLTRB(AxSpace.card, 0, AxSpace.card, 10),
            child: Text(visible(cutTo(words, 4000)), style: t.type.ui.ink(c.ink2)),
          ),
      ],
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
