import 'dart:async';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../host/host_store.dart';
import '../host/link.dart';
import '../host/proto.dart';
import '../transcript/safe_text.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/marks.dart';
import '../ui/overlay.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';
import 'drafts.dart';

/// What the host said, in a sentence.
String wordsOf(Object e) => switch (e) {
  Failed(:final message) => message,
  LinkClosed(:final why) => why,
  _ => '$e',
};

ServiceMark? markOf(String? provider, String? model, [String? harness]) =>
    ServiceMark.of(model) ??
    switch (harness) {
      'claude' => ServiceMark.claudecode,
      'codex' => ServiceMark.codex,
      'agy' => ServiceMark.antigravity,
      'agent' => ServiceMark.cursor,
      final h? when h != 'axiom' => ServiceMark.of(h),
      _ => null,
    } ??
    switch (provider) {
      'anthropic' => ServiceMark.claude,
      'claude-code' => ServiceMark.claudecode,
      'openai' => ServiceMark.openai,
      'gemini' => ServiceMark.gemini,
      'openrouter' => ServiceMark.openrouter,
      _ => null,
    };

const accessItems = [
  AxMenuItem(Access.read, 'Only look', detail: 'changes nothing'),
  AxMenuItem(Access.ask, 'Ask first', detail: 'before each change and command'),
  AxMenuItem(Access.work, 'Work in the folder', detail: 'without asking; contained'),
];

/// One labelled part of a form. Labels stand over what they name, on the surface itself.
class FormPart extends StatelessWidget {
  const FormPart(this.label, {required this.child, super.key, this.note});

  /// Null for what names itself, as a select does.
  final String? label;
  final String? note;
  final Widget child;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Padding(
      padding: const EdgeInsets.only(bottom: 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (label case final l?) ...[
            ExcludeSemantics(child: Text(l, style: t.type.label.ink(c.ink2))),
            const SizedBox(height: 5),
          ],
          child,
          if (note case final n?) ...[
            const SizedBox(height: 5),
            Text(n, style: t.type.small.ink(c.ink3)),
          ],
        ],
      ),
    );
  }
}

class _Page extends StatelessWidget {
  const _Page({required this.title, required this.children, this.problem});
  final String title;
  final List<Widget> children;
  final String? problem;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return SingleChildScrollView(
      padding: const EdgeInsets.fromLTRB(24, 28, 24, 24),
      child: Align(
        alignment: Alignment.topLeft,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 460),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text(title, style: t.type.title),
              const SizedBox(height: 20),
              ...children,
              if (problem case final p?)
                Semantics(
                  liveRegion: true,
                  child: Padding(
                    padding: const EdgeInsets.only(top: 12),
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Padding(
                          padding: const EdgeInsets.only(top: 2),
                          child: AxGlyph(AxIcon.warn, size: 13, color: c.faultText),
                        ),
                        const SizedBox(width: 7),
                        Expanded(child: Text(visible(p), style: t.type.ui.ink(c.faultText))),
                      ],
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

/// `~` as a shell has it. Everything else is the host's to judge.
String wholePath(String p) {
  final home = Platform.environment[Platform.isWindows ? 'USERPROFILE' : 'HOME'];
  if (home == null || !(p == '~' || p.startsWith('~/') || p.startsWith(r'~\'))) return p;
  return home + p.substring(1);
}

/// Folders that sessions are about, the last used first.
List<String> recentFolders(List<SessionInfo> sessions, {int most = 8}) {
  final out = <String>{};
  for (final s in sessions) {
    if (s.workspace case final w? when w.isNotEmpty) out.add(w);
    if (out.length >= most) break;
  }
  return out.toList();
}

/// What a person is setting up, and how far it has come. It outlives the page: adding an account on
/// the way, or looking at another session, loses nothing.
class StartDraft extends ChangeNotifier {
  StartDraft(this.store, {this.into}) {
    for (final c in [say, folder, model, name]) {
      c.addListener(notifyListeners);
    }
    folder.addListener(_folderNamed);
  }
  final HostStore store;

  /// The session it is to be another agent of; null: a session of its own.
  final String? into;
  final say = TextEditingController(), folder = TextEditingController();
  final model = TextEditingController(), name = TextEditingController();

  /// What it runs with.
  String harness = 'axiom';

  /// The folder's own settings for that may take effect.
  bool trust = false;

  /// Of a program: another account than its first.
  String? account;

  HarnessInfo? get delegated =>
      harness == 'axiom' ? null : store.harnesses.where((h) => h.id == harness).firstOrNull;

  void runWith(String id) => change(() {
    harness = id;
    problem = null;
    trust = false;
    account = null;
    typed = false;
    model.clear();
    final can = delegated?.access ?? Access.values;
    // The least it has a mode for, if not the one that is chosen.
    if (!can.contains(access)) {
      access = [Access.ask, Access.read, Access.work].where(can.contains).firstOrNull ?? access;
    }
  });
  String? connection, problem;
  Access access = Access.ask;
  bool busy = false;

  /// The model is written, not chosen.
  bool typed = false;

  /// The folder's words for agents, and those of them that are to be followed.
  List<Instructions> words = const [];
  final follow = <String>{};
  String? open;
  String? _looked, _asked;
  Timer? _soon;
  bool _gone = false;

  /// Made already, if a later step failed: used, not made again.
  _Made? _made;
  final _prompt = Draft();

  /// The agent that was made last.
  String? last;

  void change(VoidCallback f) {
    f();
    tell();
  }

  void tell() {
    if (!_gone) notifyListeners();
  }

  /// Looks into the folder a moment after it has been named.
  void _folderNamed() {
    final path = wholePath(folder.text.trim());
    if (path == _looked) return;
    _looked = path;
    _soon?.cancel();
    if (words.isNotEmpty || follow.isNotEmpty) {
      // What was said of another folder's does not hold for this one's.
      words = const [];
      follow.clear();
      open = null;
    }
    if (path.isEmpty) return;
    _soon = Timer(const Duration(milliseconds: 400), () async {
      try {
        final w = await store.look(path);
        if (_looked == path) change(() => words = w);
      } on Object {
        // Not a folder yet; starting says so.
      }
    });
  }

  /// Asks for the models of the chosen connection, once.
  void ask(String? id) {
    if (id == null || id == _asked) return;
    _asked = id;
    unawaited(
      store
          .models(id)
          .then(
            (_) {},
            onError: (Object e) {
              if (_asked == id) change(() => problem = wordsOf(e));
            },
          ),
    );
  }

  /// Where it works, if it does now. [shown]: whether its person still waits for it.
  Future<String?> start(Connection? c, bool Function() shown) async {
    change(() {
      busy = true;
      problem = null;
    });
    try {
      final text = say.text.trim(), where = wholePath(folder.text.trim());
      final other = harness != 'axiom', written = model.text.trim();
      final x = Execution(
        harness: harness,
        provider: other ? null : c!.provider,
        model: written.isEmpty ? null : written,
        connection: other ? account : c!.id,
      );
      final followed = [
        for (final w in words)
          if (!other && follow.contains(w.path)) w,
      ];
      final as = [harness, '$trust', for (final w in followed) '${w.path} ${w.digest}'].join('\n');
      var m = _made;
      if (m == null || m.folder != where || (m.agent != null && m.follows != as)) {
        final session = into ?? await store.create(oneLine(text, 80), workspace: where);
        m = _Made(session, where);
        _made = m;
      }
      final how = (x.provider, x.model, x.connection, access);
      if (m.agent == null) {
        m.agent = await store.addAgent(
          m.session,
          into == null ? 'Lead' : name.text.trim(),
          x,
          access,
          instructions: followed,
          trust: other && trust,
        );
        m
          ..how = how
          ..follows = as;
      } else if (m.how != how) {
        await store.setAgent(m.session, m.agent!, execution: x, access: access);
        m.how = how;
      }
      if (shown() && into == null) await store.open(m.session);
      if (text.isNotEmpty) {
        await store.prompt(m.session, m.agent!, text, key: _prompt.keyFor('${m.session} $text'));
      }
      last = m.agent;
      _made = null;
      _prompt.heard();
      say.clear();
      name.clear();
      follow.clear();
      open = null;
      return m.session;
    } on Object catch (e) {
      problem = wordsOf(e);
      return null;
    } finally {
      busy = false;
      tell();
    }
  }

  @override
  void dispose() {
    _gone = true;
    _soon?.cancel();
    say.dispose();
    folder.dispose();
    model.dispose();
    name.dispose();
    _prompt.dispose();
    super.dispose();
  }
}

class _Made {
  _Made(this.session, this.folder);
  final String session, folder;
  String? agent, follows;
  Object? how;
}

/// A new piece of work: what it is, where, and who does it.
class StartPage extends StatefulWidget {
  const StartPage(this.draft, {required this.onStarted, required this.onAccounts, super.key});
  final StartDraft draft;
  final ValueChanged<String> onStarted;
  final VoidCallback onAccounts;
  @override
  State<StartPage> createState() => _StartPageState();
}

class _StartPageState extends State<StartPage> {
  StartDraft get d => widget.draft;

  /// In the list of models: none of these.
  static const _other = '\n';

  static String _tail(String f) =>
      f.split(RegExp(r'[/\\]')).lastWhere((x) => x.isNotEmpty, orElse: () => f);
  static String _head(String f) {
    final t = _tail(f);
    return oneLine(literal(f.endsWith(t) ? f.substring(0, f.length - t.length) : f), 60);
  }

  Future<void> _start(Connection? c) async {
    final id = await d.start(c, () => mounted);
    if (id != null && mounted) widget.onStarted(id);
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: Listenable.merge([d.store, d]),
    builder: (context, _) {
      final s = d.store;
      final t = Ax.of(context), c = t.colors;
      final all = s.own;
      final chosen = all.where((x) => x.id == d.connection).firstOrNull ?? all.firstOrNull;
      if (chosen != null) {
        WidgetsBinding.instance.addPostFrameCallback((_) => d.ask(chosen.id));
      }
      final models = (chosen == null ? null : s.modelsOf(chosen.id)) ?? const <ModelInfo>[];
      final listed = models.isNotEmpty;
      // What is being written is not taken away when the list comes.
      final typed =
          d.typed || (d.model.text.isNotEmpty && !models.any((m) => m.id == d.model.text));
      final recent = recentFolders(s.sessions);
      final here = [
        for (final h in s.harnesses)
          if (h.program != null) h,
      ];
      final other = d.delegated;
      final adds = d.into != null;
      bool filled(TextEditingController x) => x.text.trim().isNotEmpty;
      final ready =
          !d.busy &&
          (d.harness == 'axiom'
              ? chosen != null && filled(d.model)
              : other != null && (!other.needsTrust || d.trust)) &&
          (adds ? filled(d.name) : filled(d.say) && filled(d.folder)) &&
          s.phase == HostPhase.connected;
      final access = [
        for (final i in accessItems)
          if (other == null || other.access.contains(i.value))
            i
          else
            AxMenuItem(i.value, i.label, enabled: false, why: '${other.name} has no mode for it'),
      ];
      final accessSelect = AxSelect<Access>(
        label: 'Access',
        items: access,
        value: d.access,
        onChanged: (v) => d.change(() => d.access = v),
      );
      final runsWith = AxSelect<String>(
        label: 'Runs with',
        items: [
          const AxMenuItem(
            'axiom',
            'Axiom',
            detail: 'your keys and servers',
            mark: ServiceMark.axiom,
          ),
          for (final h in here)
            AxMenuItem(
              h.id,
              oneLine(h.name, 40),
              detail: 'its own sign-in',
              mark: markOf(null, null, h.id),
            ),
        ],
        value: d.harness,
        onChanged: d.runWith,
      );
      return _Page(
        title: adds ? 'New agent' : 'New session',
        problem: d.problem,
        children: [
          if (adds)
            FormPart(
              'Name',
              child: AxField(label: 'Name', controller: d.name, placeholder: 'Reviewer'),
            ),
          FormPart(
            'What should be done',
            note: adds ? 'It can also be told later.' : null,
            child: AxField(label: 'What should be done', controller: d.say, lines: 4),
          ),
          if (!adds)
            FormPart(
              'Folder',
              note: 'The whole path. The agent changes nothing outside it.',
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  AxField(
                    label: 'Folder',
                    controller: d.folder,
                    mono: true,
                    placeholder: '~/project',
                  ),
                  if (recent.isNotEmpty) ...[
                    const SizedBox(height: 6),
                    AxSelect<String>(
                      label: 'Recent folders',
                      items: [
                        for (final f in recent)
                          AxMenuItem(f, oneLine(literal(_tail(f)), 60), detail: _head(f)),
                      ],
                      value: d.folder.text,
                      onChanged: (v) => d.change(() => d.folder.text = v),
                    ),
                  ],
                ],
              ),
            ),
          if (other == null)
            for (final w in d.words)
              FormPart(
                null,
                note: d.open == w.path ? null : 'Its words for agents. Followed only as they are now, and only if you say so.',
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Wrap(
                      spacing: 12,
                      runSpacing: 4,
                      crossAxisAlignment: WrapCrossAlignment.center,
                      children: [
                        AxCheck(
                          label: 'Follow ${w.path}',
                          value: d.follow.contains(w.path),
                          onChanged: (v) => d.change(() {
                            v ? d.follow.add(w.path) : d.follow.remove(w.path);
                            // Read first: what is followed has been shown.
                            if (v) d.open = w.path;
                          }),
                        ),
                        AxButton(
                          label: d.open == w.path ? 'Hide it' : 'Read it',
                          tone: AxTone.quiet,
                          onPressed: () =>
                              d.change(() => d.open = d.open == w.path ? null : w.path),
                        ),
                      ],
                    ),
                    if (d.open == w.path)
                      Padding(
                        padding: const EdgeInsets.only(top: 6),
                        child: ConstrainedBox(
                          constraints: const BoxConstraints(maxHeight: 220),
                          child: Surface(
                            c.block,
                            radius: AxRadius.key,
                            child: SingleChildScrollView(
                              padding: const EdgeInsets.all(12),
                              child: SizedBox(
                                width: double.infinity,
                                child: Text(literal(w.text), style: t.type.codeSmall.ink(c.ink2)),
                              ),
                            ),
                          ),
                        ),
                      ),
                  ],
                ),
              ),
          if (here.isNotEmpty || d.harness != 'axiom')
            FormPart(
              null,
              child: Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  runsWith,
                  if (s.accountsOf(d.harness).isNotEmpty)
                    AxSelect<String>(
                      label: 'Account',
                      items: [
                        const AxMenuItem('', 'Its own sign-in'),
                        for (final x in s.accountsOf(d.harness))
                          AxMenuItem(x.id, oneLine(x.name, 60), mark: markOf(x.provider, null)),
                      ],
                      value: d.account ?? '',
                      onChanged: (v) => d.change(() => d.account = v.isEmpty ? null : v),
                    ),
                  if (d.harness != 'axiom') accessSelect,
                ],
              ),
            ),
          if (d.harness != 'axiom') ...[
            FormPart(
              'Model',
              child: AxField(
                label: 'Model name',
                controller: d.model,
                mono: true,
                placeholder: 'Its own choice',
              ),
            ),
            FormPart(
              null,
              note: other == null
                  ? 'It is not on this computer any more.'
                  : other.needsTrust
                  ? '${other.name} cannot be told to ignore what the folder says. Those settings can '
                        'name programs that it starts.'
                  : 'Those settings can name programs that it starts.',
              child: Align(
                alignment: Alignment.centerLeft,
                child: AxCheck(
                  label: 'Let the folder\'s own settings for it take effect',
                  value: d.trust,
                  onChanged: (v) => d.change(() => d.trust = v),
                ),
              ),
            ),
          ] else if (all.isEmpty)
            FormPart(
              'Account',
              child: Align(
                alignment: Alignment.centerLeft,
                child: AxButton(
                  label: 'Add an account or a server',
                  icon: AxIcon.plus,
                  onPressed: widget.onAccounts,
                ),
              ),
            )
          else ...[
            FormPart(
              null,
              child: Wrap(
                spacing: 8,
                runSpacing: 8,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  AxSelect<String>(
                    label: 'Account',
                    items: [
                      for (final x in all)
                        AxMenuItem(
                          x.id,
                          oneLine(x.name, 60),
                          detail: providerName(x.provider),
                          mark: markOf(x.provider, null),
                        ),
                    ],
                    value: chosen!.id,
                    onChanged: (v) => d.change(() {
                      d.connection = v;
                      d.problem = null;
                      d.typed = false;
                      d.model.clear();
                    }),
                  ),
                  if (listed)
                    AxSelect<String>(
                      label: 'Model',
                      items: [
                        for (final m in models)
                          AxMenuItem(m.id, oneLine(m.name ?? m.id, 80), mark: ServiceMark.of(m.id)),
                        const AxMenuItem(_other, 'Another, by name'),
                      ],
                      value: typed ? _other : d.model.text,
                      onChanged: (v) => d.change(() {
                        d.typed = v == _other;
                        d.model.text = d.typed ? '' : v;
                      }),
                    ),
                  accessSelect,
                  AxButton(label: 'Accounts', tone: AxTone.quiet, onPressed: widget.onAccounts),
                ],
              ),
            ),
            if (!listed || typed)
              FormPart(
                'Model',
                child: AxField(
                  label: 'Model name',
                  controller: d.model,
                  mono: true,
                  placeholder: 'As the provider calls it',
                  onChanged: (_) => d.typed = true,
                ),
              ),
          ],
          Row(
            children: [
              AxButton(
                label: d.busy
                    ? 'Starting'
                    : adds
                    ? 'Add'
                    : 'Start',
                icon: adds ? AxIcon.plus : AxIcon.play,
                tone: AxTone.primary,
                onPressed: ready ? () => unawaited(_start(chosen)) : null,
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Text(
                  d.harness == 'axiom'
                      ? 'Commands run contained: no network, writing only in the folder.'
                      : 'It runs outside Axiom\'s sandbox. You are asked before it is started.',
                  style: t.type.small.ink(c.ink3),
                ),
              ),
            ],
          ),
        ],
      );
    },
  );
}

String providerName(String p) => switch (p) {
  'anthropic' => 'Anthropic API',
  'openai' => 'OpenAI API',
  'gemini' => 'Gemini API',
  'openrouter' => 'OpenRouter',
  'claude-code' => 'Claude Code, another account',
  'lm-studio' => 'LM Studio on this machine',
  'openai-compatible' => 'OpenAI-compatible server',
  _ => oneLine(p, 40),
};

/// Where a key for a provider comes from, if it answers no one without.
String? _keyFrom(String p) => switch (p) {
  'anthropic' => 'the Anthropic console',
  'openai' => 'the OpenAI platform',
  'gemini' => 'Google AI Studio',
  'openrouter' => 'OpenRouter',
  _ => null,
};

/// Accounts, keys and servers. Several for one provider is normal (PR-4).
class AccountsPage extends StatefulWidget {
  const AccountsPage(this.store, {required this.onDone, super.key});
  final HostStore store;
  final VoidCallback onDone;
  @override
  State<AccountsPage> createState() => _AccountsPageState();
}

class _AccountsPageState extends State<AccountsPage> {
  final _name = TextEditingController(), _url = TextEditingController();
  final _key = TextEditingController();
  String _provider = 'lm-studio';
  String? _problem, _leaving;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    for (final c in [_name, _url, _key]) {
      c.addListener(() => setState(() {}));
    }
  }

  @override
  void dispose() {
    _name.dispose();
    _url.dispose();
    _key.dispose();
    super.dispose();
  }

  Future<void> _do(Future<void> Function() f) async {
    setState(() {
      _busy = true;
      _problem = null;
    });
    try {
      await f();
    } on Object catch (e) {
      if (mounted) setState(() => _problem = wordsOf(e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _add() => _do(() async {
    final secret = _key.text.trim();
    await widget.store.connect(
      _name.text.trim(),
      _provider,
      url: _provider == 'openai-compatible' ? _url.text.trim() : null,
      secret: secret.isEmpty ? null : secret,
    );
    _key.clear();
    _name.clear();
    _url.clear();
  });

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: widget.store,
    builder: (context, _) {
      final t = Ax.of(context), c = t.colors;
      final own = _provider == 'openai-compatible', from = _keyFrom(_provider);
      final programs = [
        for (final h in widget.store.harnesses)
          if (h.program != null && h.accounts != null) h.accounts!,
      ];
      final program = programs.contains(_provider);
      final ready =
          !_busy &&
          _name.text.trim().isNotEmpty &&
          (!own || _url.text.trim().isNotEmpty) &&
          (from == null || _key.text.trim().isNotEmpty);
      return _Page(
        title: 'Accounts and servers',
        problem: _problem,
        children: [
          for (final x in widget.store.connections) ...[
            Padding(
              padding: const EdgeInsets.only(bottom: 6),
              child: Row(
                children: [
                  if (markOf(x.provider, null) case final m?)
                    AxServiceMark(m, size: 14)
                  else
                    AxGlyph(AxIcon.web, size: 14, color: c.ink3),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          oneLine(x.name, 80),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: t.type.uiStrong,
                        ),
                        Text(
                          [
                            providerName(x.provider),
                            if (x.url case final u?) oneLine(u, 60),
                            switch (x) {
                              Connection(signIn: _?) => 'its own sign-in',
                              Connection(lasts: true) => 'key in the system\'s store',
                              Connection(secret: true) => 'key until the host stops',
                              _ => 'no key',
                            },
                          ].join(' · '),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: t.type.small.ink(c.ink3),
                        ),
                      ],
                    ),
                  ),
                  const SizedBox(width: 10),
                  if (_leaving == x.id) ...[
                    AxButton(
                      label: 'Keep',
                      tone: AxTone.quiet,
                      autofocus: true,
                      onPressed: () => setState(() => _leaving = null),
                    ),
                    const SizedBox(width: 6),
                    AxButton(
                      label: 'Remove ${oneLine(x.name, 30)}',
                      onPressed: _busy
                          ? null
                          : () => unawaited(
                              _do(() async {
                                await widget.store.disconnect(x.id);
                                _leaving = null;
                              }),
                            ),
                    ),
                  ] else
                    AxButton(
                      label: 'Remove',
                      tone: AxTone.quiet,
                      onPressed: _busy ? null : () => setState(() => _leaving = x.id),
                    ),
                ],
              ),
            ),
            if (x.signIn case final command?)
              Padding(
                padding: const EdgeInsets.only(left: 24, bottom: 12),
                child: _SignIn(command, leaving: _leaving == x.id),
              ),
          ],
          if (widget.store.connections.isNotEmpty) const SizedBox(height: 18),
          FormPart(
            null,
            child: Align(
              alignment: Alignment.centerLeft,
              child: AxSelect<String>(
                label: 'Add',
                items: [
                  for (final p in const [
                    'lm-studio',
                    'anthropic',
                    'openai',
                    'gemini',
                    'openrouter',
                    'openai-compatible',
                  ].followedBy(programs))
                    AxMenuItem(p, providerName(p), mark: markOf(p, null)),
                ],
                value: _provider,
                onChanged: (v) => setState(() {
                  _provider = v;
                  _problem = null;
                  _key.clear();
                }),
              ),
            ),
          ),
          FormPart(
            'Name',
            child: AxField(
              label: 'Name',
              controller: _name,
              placeholder: 'So you can tell it apart',
            ),
          ),
          if (own)
            FormPart(
              'Address',
              note: 'Up to and including /v1. Without https only on this machine.',
              child: AxField(
                label: 'Address',
                controller: _url,
                mono: true,
                placeholder: 'https://example.com/v1',
              ),
            ),
          if (program)
            Padding(
              padding: const EdgeInsets.only(bottom: 16),
              child: Text(
                'Your own Claude Code keeps it in a folder of its own. You sign in to it yourself, in '
                'a terminal, with the command that is shown once it is added. Agents use an account '
                'only where you choose it.',
                style: t.type.label.ink(c.ink3),
              ),
            ),
          if (_provider != 'lm-studio' && !program)
            FormPart(
              'Key',
              note: from != null
                  ? 'An API key from $from. It goes to the system\'s store.'
                  : 'If the server needs one. It goes to the system\'s store.',
              child: AxField(label: 'Key', controller: _key, secret: true, mono: true),
            ),
          Row(
            children: [
              AxButton(
                label: 'Add',
                icon: AxIcon.plus,
                tone: AxTone.primary,
                onPressed: ready ? () => unawaited(_add()) : null,
              ),
              const SizedBox(width: 8),
              AxButton(label: 'Done', tone: AxTone.quiet, onPressed: widget.onDone),
            ],
          ),
        ],
      );
    },
  );
}

/// How an account of a program is signed in to: by its person, in their own terminal.
class _SignIn extends StatefulWidget {
  const _SignIn(this.command, {required this.leaving});
  final String command;

  /// It is about to be removed here.
  final bool leaving;
  @override
  State<_SignIn> createState() => _SignInState();
}

class _SignInState extends State<_SignIn> {
  bool _copied = false;

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(
              child: Surface(
                c.block,
                radius: AxRadius.key,
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 6),
                  child: Text(
                    // Whole: it is what its person is about to run.
                    literal(widget.command),
                    style: t.type.codeSmall.ink(c.ink2),
                  ),
                ),
              ),
            ),
            const SizedBox(width: 6),
            AxButton(
              label: _copied ? 'Copied' : 'Copy to sign in',
              tone: AxTone.quiet,
              onPressed: () async {
                await Clipboard.setData(ClipboardData(text: widget.command));
                if (mounted) setState(() => _copied = true);
              },
            ),
          ],
        ),
        if (widget.leaving)
          Padding(
            padding: const EdgeInsets.only(top: 4),
            child: Text(
              'Removing it here does not sign it out, and leaves its folder.',
              style: t.type.small.ink(c.ink3),
            ),
          ),
      ],
    );
  }
}
