import 'dart:async';

import 'package:flutter/widgets.dart';

import '../host/host_store.dart';
import '../host/link.dart';
import '../host/proto.dart';
import '../transcript/safe_text.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/marks.dart';
import '../ui/overlay.dart';
import '../ui/tokens.dart';

/// What the host said, in a sentence.
String wordsOf(Object e) => switch (e) {
  Failed(:final message) => message,
  LinkClosed(:final why) => why,
  _ => '$e',
};

ServiceMark? markOf(String? provider, String? model) =>
    ServiceMark.of(model) ??
    switch (provider) {
      'anthropic' => ServiceMark.claude,
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

/// A new piece of work: what it is, where, and who does it.
class StartPage extends StatefulWidget {
  const StartPage(this.store, {required this.onStarted, required this.onAccounts, super.key});
  final HostStore store;
  final ValueChanged<String> onStarted;
  final VoidCallback onAccounts;
  @override
  State<StartPage> createState() => _StartPageState();
}

class _StartPageState extends State<StartPage> {
  final _say = TextEditingController(), _folder = TextEditingController();
  final _model = TextEditingController();
  String? _connection, _problem;
  Access _access = Access.ask;
  bool _busy = false, _typed = false;
  String? _asked;

  /// In the list of models: none of these.
  static const _other = '\n';

  /// Made already, if a later step failed: it is used, not made again.
  String? _session;
  String? _sessionFolder;

  @override
  void initState() {
    super.initState();
    for (final c in [_say, _folder, _model]) {
      c.addListener(() => setState(() {}));
    }
  }

  @override
  void dispose() {
    _say.dispose();
    _folder.dispose();
    _model.dispose();
    super.dispose();
  }

  /// Asks for the models of the chosen connection, once.
  void _look(String? id) {
    if (id == null || id == _asked) return;
    _asked = id;
    unawaited(
      widget.store
          .models(id)
          .then(
            (_) {},
            onError: (Object e) {
              if (mounted && _asked == id) setState(() => _problem = wordsOf(e));
            },
          ),
    );
  }

  Future<void> _start(Connection c) async {
    setState(() {
      _busy = true;
      _problem = null;
    });
    final s = widget.store;
    try {
      final text = _say.text.trim(), folder = _folder.text.trim();
      if (_session == null || _sessionFolder != folder) {
        _session = await s.create(oneLine(text, 80), workspace: folder);
        _sessionFolder = folder;
      }
      final id = _session!;
      final x = Execution(
        harness: 'axiom',
        provider: c.provider,
        model: _model.text.trim(),
        connection: c.id,
      );
      final agent = await s.addAgent(id, 'Lead', x, _access);
      await s.open(id);
      await s.prompt(id, agent, text);
      if (mounted) widget.onStarted(id);
    } on Object catch (e) {
      if (mounted) setState(() => _problem = wordsOf(e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: widget.store,
    builder: (context, _) {
      final s = widget.store;
      final t = Ax.of(context), c = t.colors;
      final all = s.connections;
      final chosen = all.where((x) => x.id == _connection).firstOrNull ?? all.firstOrNull;
      if (chosen != null) {
        WidgetsBinding.instance.addPostFrameCallback((_) => _look(chosen.id));
      }
      final models = (chosen == null ? null : s.modelsOf(chosen.id)) ?? const <ModelInfo>[];
      final listed = models.isNotEmpty;
      final ready =
          !_busy &&
          chosen != null &&
          [_say, _folder, _model].every((x) => x.text.trim().isNotEmpty) &&
          s.phase == HostPhase.connected;
      return _Page(
        title: 'New session',
        problem: _problem,
        children: [
          FormPart(
            'What should be done',
            child: AxField(label: 'What should be done', controller: _say, lines: 4),
          ),
          FormPart(
            'Folder',
            note: 'The whole path. The agent changes nothing outside it.',
            child: AxField(
              label: 'Folder',
              controller: _folder,
              mono: true,
              placeholder: '/Users/you/project',
            ),
          ),
          if (all.isEmpty)
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
                    onChanged: (v) => setState(() {
                      _connection = v;
                      _problem = null;
                      _typed = false;
                      _model.clear();
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
                      value: _typed ? _other : _model.text,
                      onChanged: (v) => setState(() {
                        _typed = v == _other;
                        _model.text = _typed ? '' : v;
                      }),
                    ),
                  AxSelect<Access>(
                    label: 'Access',
                    items: accessItems,
                    value: _access,
                    onChanged: (v) => setState(() => _access = v),
                  ),
                  AxButton(label: 'Accounts', tone: AxTone.quiet, onPressed: widget.onAccounts),
                ],
              ),
            ),
            if (!listed || _typed)
              FormPart(
                'Model',
                child: AxField(
                  label: 'Model name',
                  controller: _model,
                  mono: true,
                  placeholder: 'As the provider calls it',
                ),
              ),
          ],
          Row(
            children: [
              AxButton(
                label: _busy ? 'Starting' : 'Start',
                icon: AxIcon.play,
                tone: AxTone.primary,
                onPressed: ready ? () => unawaited(_start(chosen)) : null,
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Text(
                  'Commands run contained: no network, writing only in the folder.',
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
      final ready =
          !_busy &&
          _name.text.trim().isNotEmpty &&
          (!own || _url.text.trim().isNotEmpty) &&
          (from == null || _key.text.trim().isNotEmpty);
      return _Page(
        title: 'Accounts and servers',
        problem: _problem,
        children: [
          for (final x in widget.store.connections)
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
                  ])
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
          if (_provider != 'lm-studio')
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
