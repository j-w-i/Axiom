import 'dart:async';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../host/host_store.dart';
import '../host/link.dart';
import '../host/proto.dart';
import '../patterns/model_name.dart';
import '../transcript/safe_text.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/marks.dart';
import '../ui/overlay.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';
import 'drafts.dart';
import 'outside.dart';

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
      'openai' || 'chatgpt' => ServiceMark.openai,
      'gemini' => ServiceMark.gemini,
      'openrouter' => ServiceMark.openrouter,
      _ => null,
    };

const accessItems = [
  AxMenuItem(Access.read, 'Read only', detail: 'changes nothing'),
  AxMenuItem(Access.ask, 'Ask first', detail: 'before each change and command'),
  AxMenuItem(Access.work, 'Work without asking', detail: 'contained in the folder'),
];

/// How much an agent may do. It is said where one writes to it.
class AccessKey extends StatelessWidget {
  const AccessKey(
    this.access, {
    required this.can,
    required this.of,
    required this.onChanged,
    super.key,
  });
  final Access access;

  /// What there is a mode for, and of which harness.
  final List<Access> can;
  final String of;
  final ValueChanged<Access> onChanged;
  @override
  Widget build(BuildContext context) => AxSelect<Access>(
    label: 'Access',
    quiet: true,
    icon: AxIcon.shield,
    items: [
      for (final i in accessItems)
        if (can.contains(i.value))
          i
        else
          AxMenuItem(i.value, i.label, enabled: false, why: '$of has no mode for it'),
    ],
    value: access,
    onChanged: onChanged,
  );
}

/// How hard a model is to try, where that can be said.
const efforts = [
  AxMenuItem('', 'Default effort'),
  AxMenuItem('low', 'Low effort'),
  AxMenuItem('medium', 'Medium effort'),
  AxMenuItem('high', 'High effort'),
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
            const SizedBox(height: 6),
          ],
          child,
          if (note case final n?) ...[
            const SizedBox(height: 6),
            Text(n, style: t.type.label.ink(c.ink3)),
          ],
        ],
      ),
    );
  }
}

class _Page extends StatelessWidget {
  const _Page({required this.children, this.problem});
  final List<Widget> children;
  final String? problem;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return SingleChildScrollView(
      padding: const EdgeInsets.fromLTRB(24, 34, 24, 32),
      child: Align(
        alignment: Alignment.topCenter,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 640),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
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
                          child: AxGlyph(AxIcon.warn, color: c.faultText),
                        ),
                        const SizedBox(width: 8),
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

/// Lines of one kind on one raised surface, a groove between them.
class Card extends StatelessWidget {
  const Card(this.lines, {super.key});
  final List<Widget> lines;
  @override
  Widget build(BuildContext context) {
    final c = Ax.of(context).colors;
    return Surface(
      c.plate,
      radius: AxRadius.card,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final (i, l) in lines.indexed) ...[
            if (i > 0) const Hairline(),
            Padding(padding: const EdgeInsets.fromLTRB(14, 11, 12, 11), child: l),
          ],
        ],
      ),
    );
  }
}

/// Something that can be chosen or not, and what that means.
class OptionLine extends StatelessWidget {
  const OptionLine({
    required this.label,
    required this.value,
    required this.onChanged,
    super.key,
    this.note,
    this.trailing,
    this.below,
  });
  final String label;
  final String? note;
  final bool value;
  final ValueChanged<bool> onChanged;
  final Widget? trailing, below;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  AxCheck(label: label, value: value, onChanged: onChanged),
                  if (note case final n?)
                    Padding(
                      padding: const EdgeInsets.only(left: 25, top: 1),
                      child: Text(n, style: t.type.label.ink(c.ink3)),
                    ),
                ],
              ),
            ),
            ?trailing,
          ],
        ),
        ?below,
      ],
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

  /// Of a program: Axiom's tools in place of its own, where it can be told so.
  bool hands = true;
  bool get withHands => hands && (delegated?.hands ?? false);

  /// What it may be, as it is set up now.
  List<Access> get can => withHands ? Access.values : delegated?.access ?? Access.values;

  void fit() {
    // The least it has a mode for, if not the one that is chosen.
    if (!can.contains(access)) {
      access = [Access.ask, Access.read, Access.work].where(can.contains).firstOrNull ?? access;
    }
  }

  /// How hard it is to try; empty: as is usual.
  String effort = '';

  /// Whether that can be said of what is chosen.
  bool get tries =>
      delegated?.effort ??
      (store.modelsOf(connection ?? store.own.firstOrNull?.id ?? '') ?? const []).any(
        (m) => m.id == model.text.trim() && m.effort,
      );

  /// In a copy of the folder of its own. Beside others it is, if nobody says otherwise.
  late bool apart = into != null;
  bool get worksApart => apart && access != Access.read;

  /// Of a program: another account than its first.
  String? account;

  HarnessInfo? get delegated =>
      harness == 'axiom' ? null : store.harnesses.where((h) => h.id == harness).firstOrNull;

  void runWith(String id) => change(() {
    harness = id;
    problem = null;
    trust = false;
    account = null;
    model.clear();
    fit();
  });
  String? connection, problem;
  Access access = Access.ask;
  bool busy = false;

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

  bool _recalled = false;

  /// Begins as its person last worked: where, and with what. Once, and never over what they have set.
  /// How much it may do is not taken over: that is said each time.
  void recall() {
    final m = store.current;
    if (_recalled || m == null && store.sessions.isEmpty) return;
    _recalled = true;
    if (into == null && folder.text.isEmpty) {
      folder.text = m?.workspace ?? recentFolders(store.sessions, most: 1).firstOrNull ?? '';
    }
    final x = m?.agents.values.where((a) => a.access != null).firstOrNull?.execution;
    if (x == null || harness != 'axiom' || connection != null || model.text.isNotEmpty) return;
    if (x.harness == 'axiom') {
      if (!store.own.any((c) => c.id == x.connection)) return;
      connection = x.connection;
    } else {
      if (!store.harnesses.any((h) => h.id == x.harness && h.program != null)) return;
      harness = x.harness;
      hands = x.environment == axiomTools;
      if (store.accountsOf(harness).any((c) => c.id == x.connection)) account = x.connection;
    }
    model.text = x.model ?? '';
    effort = x.effort ?? '';
    fit();
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
        effort: tries && effort.isNotEmpty ? effort : null,
        environment: withHands ? axiomTools : null,
      );
      final followed = [
        for (final w in words)
          if (!other && follow.contains(w.path)) w,
      ];
      final as = [
        harness,
        '$trust',
        '$worksApart',
        for (final w in followed) '${w.path} ${w.digest}',
      ].join('\n');
      var m = _made;
      if (m == null || m.folder != where || (m.agent != null && m.follows != as)) {
        final session = into ?? await store.create(oneLine(text, 80), workspace: where);
        m = _Made(session, where);
        _made = m;
      }
      final how = (x.provider, x.model, x.connection, x.effort, x.environment, access);
      if (m.agent == null) {
        m.agent = await store.addAgent(
          m.session,
          into == null ? 'Lead' : name.text.trim(),
          x,
          access,
          instructions: followed,
          trust: other && trust,
          apart: worksApart,
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

String providerName(String p) => switch (p) {
  'anthropic' => 'Anthropic API',
  'openai' => 'OpenAI API',
  'chatgpt' => 'ChatGPT plan',
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

  /// The browser was opened for the sign-in that is on.
  bool _opened = false;

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

  /// Only for a person's press: it opens their browser.
  Future<void> _signIn({String? again}) => _do(() async {
    final s = await widget.store.beginSignin(again: again);
    _opened = await browse(s.url);
  });

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: widget.store,
    builder: (context, _) {
      final t = Ax.of(context), c = t.colors;
      final own = _provider == 'openai-compatible', plan = _provider == 'chatgpt';
      final signin = widget.store.signin;
      final from = _keyFrom(_provider);
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
        problem: _problem,
        children: [
          if (widget.store.connections.isNotEmpty) ...[
            Card([
              for (final x in widget.store.connections)
                Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Row(
                      children: [
                        if (markOf(x.provider, null) case final m?)
                          AxServiceMark(m, size: 16)
                        else
                          AxGlyph(AxIcon.web, color: c.ink3),
                        const SizedBox(width: 12),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                oneLine(x.name, 80),
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: t.type.uiMedium,
                              ),
                              Text(
                                [
                                  providerName(x.provider),
                                  if (x.url case final u?) oneLine(u, 60),
                                  switch (x) {
                                    Connection(signIn: _?) => 'its own sign-in',
                                    Connection(plan: true, lasts: true) => 'signed in',
                                    Connection(plan: true) => 'signed in until the host stops',
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
                            autofocus: true,
                            onPressed: () => setState(() => _leaving = null),
                          ),
                          const SizedBox(width: 8),
                          AxButton(
                            label: 'Remove ${oneLine(x.name, 30)}',
                            onPressed: _busy
                                ? null
                                : () => unawaited(
                                    _do(() async {
                                      final ended = await widget.store.disconnect(x.id);
                                      _leaving = null;
                                      if (ended == false) {
                                        _problem =
                                            'It is removed here. OpenAI did not confirm that it is signed '
                                            'out there: Axiom can be disconnected in ChatGPT\'s settings.';
                                      }
                                    }),
                                  ),
                          ),
                        ] else
                          AxButton(
                            label: 'Remove',
                            onPressed: _busy ? null : () => setState(() => _leaving = x.id),
                          ),
                      ],
                    ),
                    if (x.problem case final p?)
                      Padding(
                        padding: const EdgeInsets.only(left: 28, top: 8),
                        child: Row(
                          children: [
                            Expanded(child: Text(visible(p), style: t.type.small.ink(c.faultText))),
                            const SizedBox(width: 10),
                            AxButton(
                              label: 'Sign in again',
                              onPressed: _busy ? null : () => unawaited(_signIn(again: x.id)),
                            ),
                          ],
                        ),
                      ),
                    if (x.signIn case final command?)
                      Padding(
                        padding: const EdgeInsets.only(left: 28, top: 8),
                        child: _SignIn(command, leaving: _leaving == x.id),
                      ),
                  ],
                ),
            ]),
            const SizedBox(height: 24),
          ],
          Semantics(header: true, child: Text('Add an account', style: t.type.title)),
          const SizedBox(height: 12),
          FormPart(
            null,
            child: Align(
              alignment: Alignment.centerLeft,
              child: AxSelect<String>(
                label: 'Add',
                named: false,
                items: [
                  for (final p in const [
                    'lm-studio',
                    'anthropic',
                    'openai',
                    'gemini',
                    'openrouter',
                    'openai-compatible',
                    'chatgpt',
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
          if (signin != null && (plan || signin.again != null || !signin.waits))
            _Signing(
              signin,
              opened: _opened,
              onOpen: () => unawaited(_do(() async => _opened = await browse(signin.url))),
              onClose: () => unawaited(_do(widget.store.cancelSignin)),
            )
          else if (plan)
            Padding(
              padding: const EdgeInsets.only(bottom: 16),
              child: Text(
                'You sign in with OpenAI, in your browser. Axiom\'s own agents then work on your '
                'plan: there is no key, and nothing is billed to an API account. Axiom keeps what '
                'OpenAI gives it in the system\'s store.',
                style: t.type.label.ink(c.ink3),
              ),
            ),
          if (!plan)
            FormPart(
              'Name',
              child: AxField(label: 'Name', controller: _name, placeholder: 'To tell it apart'),
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
                'Claude Code keeps this account in its own folder. You sign in yourself, in a terminal, '
                'with the command shown once it is added. Agents use it only where you choose it.',
                style: t.type.label.ink(c.ink3),
              ),
            ),
          if (_provider != 'lm-studio' && !program && !plan)
            FormPart(
              'Key',
              note: from != null
                  ? 'An API key from $from. It goes to the system\'s store.'
                  : 'If the server needs one. It goes to the system\'s store.',
              child: AxField(label: 'Key', controller: _key, secret: true, mono: true),
            ),
          Row(
            mainAxisAlignment: MainAxisAlignment.end,
            children: [
              AxButton(label: 'Done', onPressed: widget.onDone),
              const SizedBox(width: 8),
              if (plan)
                AxButton(
                  label: 'Continue with ChatGPT',
                  tone: AxTone.primary,
                  onPressed: _busy || (signin?.waits ?? false) ? null : () => unawaited(_signIn()),
                )
              else
                AxButton(
                  label: 'Add',
                  tone: AxTone.primary,
                  onPressed: ready ? () => unawaited(_add()) : null,
                ),
            ],
          ),
        ],
      );
    },
  );
}

/// A sign-in that waits for its person, or how it ended.
class _Signing extends StatefulWidget {
  const _Signing(this.signin, {required this.opened, required this.onOpen, required this.onClose});
  final Signin signin;
  final bool opened;
  final VoidCallback onOpen, onClose;
  @override
  State<_Signing> createState() => _SigningState();
}

class _SigningState extends State<_Signing> {
  bool _copied = false;

  @override
  void didUpdateWidget(_Signing old) {
    super.didUpdateWidget(old);
    if (old.signin.id != widget.signin.id) _copied = false;
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors, s = widget.signin;
    final (words, more) = switch (s) {
      Signin(connection: _?) when s.again == null => (
        'You\'re using your ChatGPT plan',
        'What Axiom\'s agents do with this account counts towards your plan\'s limits.',
      ),
      Signin(connection: _?) => ('You are signed in again', null),
      Signin(:final problem?) => ('You are not signed in', visible(problem)),
      Signin(finishing: true) => ('Signing in…', null),
      _ when widget.opened => (
        'Continue in your browser',
        'Axiom continues when you are done there.',
      ),
      _ => (
        'Open the link in your browser',
        'It could not be opened from here. It is for a browser on this computer.',
      ),
    };
    return Semantics(
      liveRegion: true,
      child: Padding(
        padding: const EdgeInsets.only(bottom: 16),
        child: Surface(
          c.plate,
          radius: AxRadius.card,
          child: Padding(
            padding: const EdgeInsets.all(14),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(words, style: t.type.uiStrong),
                if (more case final m?) ...[
                  const SizedBox(height: 3),
                  Text(m, style: t.type.small.ink(s.problem == null ? c.ink3 : c.faultText)),
                ],
                if (!s.finishing) ...[
                  const SizedBox(height: 10),
                  Wrap(
                    spacing: 6,
                    runSpacing: 6,
                    children: [
                      if (s.waits) ...[
                        AxButton(
                          label: widget.opened ? 'Open again' : 'Open',
                          onPressed: widget.onOpen,
                        ),
                        AxButton(
                          label: _copied ? 'Copied' : 'Copy link',
                          onPressed: () async {
                            await Clipboard.setData(ClipboardData(text: s.url));
                            if (mounted) setState(() => _copied = true);
                          },
                        ),
                      ],
                      AxButton(
                        label: s.waits ? 'Cancel' : (s.problem == null ? 'Got it' : 'Close'),
                        tone: AxTone.quiet,
                        onPressed: widget.onClose,
                      ),
                    ],
                  ),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
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
            const SizedBox(width: 8),
            AxButton(
              label: _copied ? 'Copied' : 'Copy',
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

Map<String, (String, String?)> namesOf(List<ModelInfo> all) =>
    modelNames([for (final m in all) (m.id, m.name)]);

/// A model that is offered.
AxMenuItem<String> modelItem(ModelInfo m, Map<String, (String, String?)> names) {
  final (name, id) = names[m.id]!;
  return AxMenuItem(
    m.id,
    oneLine(name, 80),
    detail: id == null ? null : oneLine(id, 80),
    mark: ServiceMark.of(m.id),
  );
}
