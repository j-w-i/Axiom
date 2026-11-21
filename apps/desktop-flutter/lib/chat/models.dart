// Model providers (FL-CHAT-001; ADR-0017 §5, ADR-0020 §6; a port of the Tauri shell's
// Models.tsx): the first-run setup Chat shows when nothing can answer, and the Models section of
// Settings. Keys go to the OS credential store through the daemon; the field clears either way
// and the UI only ever shows whether a key is stored. The built-in agents can be installed here.

import 'dart:async';

import 'package:flutter/widgets.dart';

import '../app/app_state.dart';
import '../daemon/protocol.g.dart';
import '../state/agent.dart';
import '../ui/controls.dart';
import '../ui/icon_data.dart';
import '../ui/icons.dart';
import '../ui/overlays.dart';
import '../ui/tabs.dart';
import '../ui/text_input.dart';
import '../ui/theme.dart';
import '../ui/tokens.dart';

String kindLabel(ProviderKind k) => switch (k) {
  ProviderKind.anthropic => 'Anthropic',
  ProviderKind.openaiChat => 'OpenAI-compatible',
  ProviderKind.acp => 'Agent',
  ProviderKind.scripted => 'Test',
};

/// The add-a-provider form: kind, server (OpenAI-compatible), key, name.
class AddProvider extends StatefulWidget {
  const AddProvider({super.key, this.compact = false});

  /// The first-run panel: no name field.
  final bool compact;

  @override
  State<AddProvider> createState() => _AddProviderState();
}

class _AddProviderState extends State<AddProvider> {
  final _url = TextEditingController();
  final _key = TextEditingController();
  final _name = TextEditingController();
  AddKind _kind = AddKind.anthropic;
  bool _busy = false;
  String _error = '';

  @override
  void dispose() {
    _url.dispose();
    _key.dispose();
    _name.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final a = AppScope.of(context).agent;
    final c = formConfig(_kind, _url.text, _name.text, [for (final p in a.providers) p.providerId]);
    if (c is String) {
      setState(() => _error = c);
      return;
    }
    if (_kind == AddKind.anthropic && _key.text.trim().isEmpty) {
      setState(() => _error = 'Anthropic needs an API key.');
      return;
    }
    setState(() {
      _busy = true;
      _error = '';
    });
    final key = _key.text.trim();
    // The key leaves the UI either way: it is in the credential store or it was refused.
    _key.clear();
    final err = await a.configure(c as ProviderConfig, apiKey: key.isEmpty ? null : key);
    if (!mounted) return;
    setState(() {
      _busy = false;
      _error = err;
    });
    if (err.isEmpty) {
      _url.clear();
      _name.clear();
    }
  }

  Widget _row(BuildContext context, String label, Widget field) => Padding(
    padding: const EdgeInsets.only(bottom: Metrics.s3),
    child: Row(
      children: [
        SizedBox(width: 90, child: Text(label, style: context.axiom.type.small)),
        Expanded(child: field),
      ],
    ),
  );

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: 'Add a model provider',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          AxTabs(
            label: 'Provider kind',
            tabs: const [
              TabItem(id: 'anthropic', label: 'Anthropic'),
              TabItem(id: 'openai', label: 'OpenAI-compatible'),
            ],
            selected: _kind.name,
            onSelect: (id) => setState(() {
              _kind = AddKind.values.byName(id);
              _error = '';
            }),
          ),
          const SizedBox(height: Metrics.s4),
          if (_kind == AddKind.openai)
            _row(
              context,
              'Server',
              AxTextInput(
                label: 'Server address',
                controller: _url,
                mono: true,
                placeholder: 'https://api.openai.com/v1',
              ),
            ),
          _row(
            context,
            'API key',
            AxTextInput(
              label: 'API key',
              controller: _key,
              obscure: true,
              placeholder: _kind == AddKind.openai
                  ? 'Not needed for a server on this computer'
                  : 'sk-ant-… (kept in the system keychain)',
              onSubmitted: (_) => unawaited(_save()),
            ),
          ),
          if (!widget.compact)
            _row(
              context,
              'Name',
              AxTextInput(label: 'Provider name', controller: _name, placeholder: 'Optional'),
            ),
          if (_error.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(bottom: Metrics.s3),
              child: Semantics(
                liveRegion: true,
                child: Text(_error, style: t.type.small.copyWith(color: t.colors.dangerText)),
              ),
            ),
          Row(
            children: [
              AxButton(label: 'Add provider', onPressed: _busy ? null : () => unawaited(_save())),
              if (_busy) ...[const SizedBox(width: 8), const Spinner(label: 'Checking')],
            ],
          ),
        ],
      ),
    );
  }
}

/// Finds Ollama, llama.cpp and LM Studio on their usual ports.
class LocalServers extends StatefulWidget {
  const LocalServers({super.key});

  @override
  State<LocalServers> createState() => _LocalServersState();
}

class _LocalServersState extends State<LocalServers> {
  bool _busy = false;
  List<({String name, bool found, String detail})>? _found;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final a = AppScope.of(context).agent;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            AxButton(
              label: 'Find local servers',
              hint: 'Looks for Ollama, llama.cpp and LM Studio on their usual ports',
              onPressed: _busy
                  ? null
                  : () async {
                      setState(() => _busy = true);
                      final r = await a.detectLocal();
                      if (mounted) {
                        setState(() {
                          _found = r;
                          _busy = false;
                        });
                      }
                    },
            ),
            if (_busy) ...[const SizedBox(width: 8), const Spinner(label: 'Looking')],
          ],
        ),
        if (_found != null)
          Semantics(
            label: 'Local servers',
            container: true,
            child: Padding(
              padding: const EdgeInsets.only(top: Metrics.s3),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  for (final f in _found!)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 4),
                      child: Row(
                        children: [
                          AxiomIcon(
                            f.found ? AxiomIcons.check : AxiomIcons.close,
                            size: 14,
                            color: f.found ? t.colors.okText : t.colors.ink3,
                          ),
                          const SizedBox(width: 6),
                          Text('${f.name}: ${f.detail}', style: t.type.small),
                        ],
                      ),
                    ),
                ],
              ),
            ),
          ),
      ],
    );
  }
}

/// Installs a built-in agent, with the outcome as a toast.
Future<void> installAgent(BuildContext context, ProviderOption p) async {
  final app = AppScope.of(context);
  final toasts = ToastHost.of(context);
  toasts.show('Installing ${p.name}…');
  final err = await app.agent.installAgent(p.providerId);
  if (err.isEmpty) {
    toasts.show(
      '${p.name} is installed. It uses your own sign-in: run it once in a terminal if it asks.',
      kind: ToastKind.ok,
      duration: const Duration(seconds: 8),
    );
  } else {
    toasts.show(
      '${p.name} was not installed: $err',
      kind: ToastKind.error,
      duration: const Duration(seconds: 12),
    );
  }
}

/// The external agents: ready, or why not (with Install for the built-in ones).
class AgentsList extends StatelessWidget {
  const AgentsList({super.key});

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: AppScope.of(context).agent,
    builder: (context, _) => _list(context),
  );

  Widget _list(BuildContext context) {
    final app = AppScope.of(context);
    final a = app.agent;
    final t = context.axiom;
    final agents = [
      for (final p in a.providers)
        if (p.isAgent) p,
    ];
    if (agents.isEmpty) return Text('None.', style: t.type.small);
    return Semantics(
      label: 'Agents',
      container: true,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final p in agents)
            Padding(
              padding: const EdgeInsets.only(bottom: Metrics.s3),
              child: Row(
                children: [
                  SizedBox(width: 130, child: Text(p.name, style: t.type.label)),
                  Expanded(
                    child: Text(
                      p.usable
                          ? 'Ready: with a folder or without one'
                          : (p.error ?? 'Not available'),
                      style: t.type.small,
                      overflow: TextOverflow.ellipsis,
                      maxLines: 2,
                    ),
                  ),
                  if (p.usable && a.activeId == null)
                    AxButton(
                      label: 'Use ${p.name}',
                      kind: ButtonKind.quiet,
                      onPressed: () =>
                          a.setModel((providerId: p.providerId, model: p.models.first.id)),
                    )
                  else if (!p.usable && p.builtin && a.installing.contains(p.providerId)) ...[
                    const Spinner(label: 'Installing with npm'),
                    const SizedBox(width: 8),
                    AxButton(
                      label: 'Cancel installing ${p.name}',
                      kind: ButtonKind.quiet,
                      onPressed: () => a.cancelInstall(p.providerId),
                    ),
                  ] else if (!p.usable && p.builtin)
                    AxButton(
                      label: a.installing.contains(p.providerId)
                          ? 'Installing ${p.name}…'
                          : 'Install ${p.name}',
                      icon: AxiomIcons.download,
                      hint: 'Installs it with npm into Axiom\'s data directory',
                      onPressed: a.installing.contains(p.providerId)
                          ? null
                          : () => unawaited(installAgent(context, p)),
                    ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}

/// Chat's first-run setup: shown when nothing can answer.
class ModelSetup extends StatelessWidget {
  const ModelSetup({super.key});

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    Widget card(String title, String? hint, Widget body) => Padding(
      padding: const EdgeInsets.only(bottom: Metrics.s6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Semantics(header: true, child: Text(title, style: t.type.label)),
          if (hint != null)
            Padding(
              padding: const EdgeInsets.only(top: 2),
              child: Text(hint, style: t.type.small),
            ),
          const SizedBox(height: Metrics.s4),
          body,
        ],
      ),
    );
    return ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 640),
      child: Semantics(
        container: true,
        explicitChildNodes: true,
        label: 'Model setup',
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Axiom talks to the model or agent you choose. Pick one way; you can add more in '
              'Settings → Models.',
              style: t.type.small,
            ),
            const SizedBox(height: Metrics.s6),
            card(
              'An agent you already use',
              'Claude Code, Codex or Gemini CLI, with your own sign-in.',
              const AgentsList(),
            ),
            card('With an API key or a server', null, const AddProvider(compact: true)),
            card(
              'A model on this computer',
              'Ollama, llama.cpp or LM Studio, running on their usual ports.',
              const LocalServers(),
            ),
          ],
        ),
      ),
    );
  }
}

class _ProviderRow extends StatefulWidget {
  const _ProviderRow(this.p, {super.key});
  final ProviderOption p;

  @override
  State<_ProviderRow> createState() => _ProviderRowState();
}

class _ProviderRowState extends State<_ProviderRow> {
  final _key = TextEditingController();
  bool _editing = false;
  String _error = '';

  @override
  void dispose() {
    _key.dispose();
    super.dispose();
  }

  Future<void> _saveKey() async {
    final k = _key.text;
    _key.clear();
    if (k.trim().isEmpty) return;
    final err = await AppScope.of(context).agent.setKey(widget.p.providerId, k.trim());
    if (mounted) {
      setState(() {
        _error = err;
        if (err.isEmpty) _editing = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final a = AppScope.of(context).agent;
    final t = context.axiom;
    final p = widget.p;
    final keyState = p.apiKeyEnv != null
        ? 'Key from \$${p.apiKeyEnv}'
        : (p.hasKey ? 'Key stored' : 'No key');
    final models = p.error ?? '${p.models.length} model${p.models.length == 1 ? '' : 's'}';
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: '${p.name}, ${kindLabel(p.kind)}, $keyState, $models',
      child: Padding(
        padding: const EdgeInsets.only(bottom: Metrics.s4),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Text(p.name, style: t.type.label),
                const SizedBox(width: 8),
                Text(kindLabel(p.kind), style: t.type.tiny),
                if (p.baseUrl != null) ...[
                  const SizedBox(width: 8),
                  Flexible(
                    child: Text(
                      p.baseUrl!,
                      style: t.type.codeSmall,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                ],
              ],
            ),
            const SizedBox(height: 2),
            Text(
              '$keyState · $models',
              style: t.type.small.copyWith(color: p.error != null ? t.colors.dangerText : null),
            ),
            if (p.kind != ProviderKind.scripted)
              Padding(
                padding: const EdgeInsets.only(top: 4),
                child: Row(
                  children: [
                    if (p.apiKeyEnv == null) ...[
                      AxButton(
                        label: p.hasKey ? 'Replace key' : 'Set key',
                        kind: ButtonKind.quiet,
                        onPressed: () => setState(() => _editing = !_editing),
                      ),
                      if (p.hasKey)
                        AxButton(
                          label: 'Clear key',
                          kind: ButtonKind.quiet,
                          onPressed: () async {
                            final err = await a.setKey(p.providerId, null);
                            if (mounted) setState(() => _error = err);
                          },
                        ),
                    ],
                    AxButton(
                      label: 'Remove ${p.name}',
                      kind: ButtonKind.quiet,
                      onPressed: () async {
                        final err = await a.removeProvider(p.providerId);
                        if (mounted) setState(() => _error = err);
                      },
                    ),
                  ],
                ),
              ),
            if (_editing)
              Padding(
                padding: const EdgeInsets.only(top: 4),
                child: Row(
                  children: [
                    Expanded(
                      child: AxTextInput(
                        label: 'API key for ${p.name}',
                        controller: _key,
                        obscure: true,
                        autofocus: true,
                        onSubmitted: (_) => unawaited(_saveKey()),
                      ),
                    ),
                    const SizedBox(width: 8),
                    AxButton(label: 'Save key', onPressed: () => unawaited(_saveKey())),
                  ],
                ),
              ),
            if (_error.isNotEmpty)
              Padding(
                padding: const EdgeInsets.only(top: 4),
                child: Text(_error, style: t.type.small.copyWith(color: t.colors.dangerText)),
              ),
          ],
        ),
      ),
    );
  }
}

/// Settings → Models.
class ModelsPane extends StatefulWidget {
  const ModelsPane({super.key});

  @override
  State<ModelsPane> createState() => _ModelsPaneState();
}

class _ModelsPaneState extends State<ModelsPane> {
  @override
  void initState() {
    super.initState();
    final a = AppScope.of(context).agent;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (a.status == 'idle') unawaited(a.load());
    });
  }

  @override
  Widget build(BuildContext context) {
    final a = AppScope.of(context).agent;
    final t = context.axiom;
    Widget heading(String s) => Padding(
      padding: const EdgeInsets.only(top: Metrics.s5, bottom: Metrics.s3),
      child: Semantics(header: true, child: Text(s, style: t.type.label)),
    );
    return ListenableBuilder(
      listenable: a,
      builder: (context, _) {
        final configured = [
          for (final p in a.providers)
            if (!p.isAgent) p,
        ];
        final usable = [
          for (final p in configured)
            if (p.usable) p,
        ];
        final current = usable.where((p) => p.providerId == a.model?.providerId).firstOrNull;
        final currentName = current == null
            ? 'None'
            : '${current.name} · '
                  '${current.models.where((m) => m.id == a.model?.model).firstOrNull?.name ?? a.model?.model}';
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                SizedBox(width: 160, child: Text('Default model', style: t.type.label)),
                if (usable.isEmpty)
                  Text('Add a provider first.', style: t.type.small)
                else
                  AxMenu(
                    label: 'Default model: $currentName',
                    entries: () => [
                      for (final p in usable) ...[
                        MenuEntry.heading(p.name),
                        for (final m in p.models)
                          MenuEntry.item(
                            m.name,
                            () => a.setModel((providerId: p.providerId, model: m.id)),
                          ),
                      ],
                    ],
                    trigger: (context, s, open) => Text(
                      currentName,
                      style: t.type.body.copyWith(color: s.hovered || open ? t.colors.ink : null),
                    ),
                  ),
              ],
            ),
            heading('Providers'),
            if (a.status == 'loading') const Spinner(label: 'Checking providers'),
            if (configured.isEmpty)
              Text('None yet.', style: t.type.small)
            else
              for (final p in configured) _ProviderRow(p, key: ValueKey(p.providerId)),
            heading('Add a provider'),
            const AddProvider(),
            heading('On this computer'),
            const LocalServers(),
            heading('Agents'),
            const AgentsList(),
          ],
        );
      },
    );
  }
}
