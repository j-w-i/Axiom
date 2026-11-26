import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../host/host_store.dart';
import '../host/proto.dart';
import '../patterns/composer.dart';
import '../patterns/execution.dart';
import '../transcript/safe_text.dart';
import '../ui/controls.dart';
import '../ui/editable.dart';
import '../ui/icons.dart';
import '../ui/marks.dart';
import '../ui/overlay.dart';
import '../ui/prefs.dart';
import '../ui/press.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';
import 'model_picker.dart';
import 'offers.dart';
import 'outside.dart';
import 'start.dart';

/// A conversation that has not begun: where it is written, what it runs on and where it works. What
/// is sent begins it. The same for another agent of a session.
class StartPage extends StatefulWidget {
  const StartPage(
    this.draft, {
    required this.onStarted,
    required this.onAccounts,
    this.onCancel,
    super.key,
  });

  /// Leaves it as it is; null where there is nothing to go back to.
  final VoidCallback? onCancel;
  final StartDraft draft;
  final ValueChanged<String> onStarted;
  final VoidCallback onAccounts;
  @override
  State<StartPage> createState() => _StartPageState();
}

class _StartPageState extends State<StartPage> {
  StartDraft get d => widget.draft;

  @override
  void initState() {
    super.initState();
    d.recall();
    if (d.into != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) => mounted ? _name.requestFocus() : null);
    }
  }

  final _name = FocusNode(debugLabel: 'name');
  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  Future<bool> _start(Connection? c) async {
    final key = offerKey(d.harness, d.on, d.model.text.trim());
    final id = await d.start(c, () => mounted);
    if (id != null && mounted) {
      final all = PrefsScope.maybeOf(context);
      all?.value = all.value.using(key);
      widget.onStarted(id);
    }
    return id != null;
  }

  @override
  Widget build(BuildContext context) => CallbackShortcuts(
    bindings: {const SingleActivator(LogicalKeyboardKey.escape): ?widget.onCancel},
    child: ListenableBuilder(
      listenable: Listenable.merge([d.store, d]),
      builder: (context, _) => _page(context),
    ),
  );

  Widget _page(BuildContext context) {
    final s = d.store;
    final t = Ax.of(context), c = t.colors;
    final own = d.harness == 'axiom', adds = d.into != null;
    final all = s.own;
    final chosen = all.where((x) => x.id == d.connection).firstOrNull ?? all.firstOrNull;
    if (chosen != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) => d.ask(chosen.id));
    }
    final other = d.delegated;
    bool filled(TextEditingController x) => x.text.trim().isNotEmpty;
    // What has to be set before it can begin, in the order it is looked for.
    final missing = s.phase != HostPhase.connected
        ? 'Waiting for the host.'
        : own && chosen == null
        ? 'Add an account to begin.'
        : own && !filled(d.model)
        ? 'Choose a model to begin.'
        : !own && other == null
        ? 'That harness is not on this computer any more.'
        : !adds && !filled(d.folder)
        ? 'Choose a folder to begin.'
        : !own && other!.needsTrust && !d.trust
        ? '${other.name} cannot ignore the folder\'s own settings: allow them to begin.'
        : adds && !filled(d.name)
        ? 'Name it to begin.'
        : null;
    final said = d.busy
        ? 'Starting'
        : missing ??
              (own
                  ? 'Sandboxed: no network, writes only in the folder.'
                  : 'Runs outside Axiom\'s sandbox. You approve each start.');
    final ready = missing == null && !d.busy && (adds || filled(d.say));
    final page = Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Semantics(
          header: true,
          child: Text(
            adds ? 'Add an agent' : 'What should we work on?',
            textAlign: TextAlign.center,
            style: t.type.heading,
          ),
        ),
        const SizedBox(height: 22),
        Composer(
          key: ObjectKey(d),
          label: 'Task',
          placeholder: adds ? 'What it should do. You can also say it later' : 'Describe a task',
          action: adds ? 'Add agent' : 'Start session',
          controller: d.say,
          ready: ready,
          autofocus: !adds,
          high: 64,
          header: !adds
              ? null
              : Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Padding(
                      padding: const EdgeInsets.fromLTRB(AxSpace.edge, 13, AxSpace.edge, 10),
                      child: AxEditable(
                        label: 'Name',
                        placeholder: 'Name, such as Reviewer',
                        controller: d.name,
                        focusNode: _name,
                        style: t.type.uiStrong.copyWith(fontSize: 15),
                        maxLines: 1,
                        onSubmitted: (_) => FocusScope.of(context).nextFocus(),
                      ),
                    ),
                    const Padding(
                      padding: EdgeInsets.symmetric(horizontal: AxSpace.edge),
                      child: Hairline(),
                    ),
                  ],
                ),
          onSubmit: (_) => _start(chosen),
          leading: [
            if (own && all.isEmpty)
              AxButton(
                label: 'Add an account or server',
                icon: AxIcon.plus,
                tone: AxTone.quiet,
                onPressed: widget.onAccounts,
              ),
            if (sourcesOf(s).isNotEmpty)
              ModelKey(
                s,
                Running(
                  harness: d.harness,
                  connection: d.on,
                  model: d.model.text.trim(),
                  effort: d.effort,
                  speed: d.speed,
                ),
                of: d,
                onModel: d.choose,
                onEffort: (v) => d.change(() => d.effort = v),
                onSpeed: (v) => d.change(() => d.speed = v),
                advanced: [if (PrefsScope.of(context).harnesses) _harness()],
                onModels: widget.onAccounts,
              ),
            if (!own || all.isNotEmpty)
              AccessKey(
                d.access,
                can: d.can,
                of: other?.name ?? 'Axiom',
                plan: d.plan,
                plans: d.plans,
                onChanged: (v, plan) => d.change(() {
                  d
                    ..access = v
                    ..plan = plan;
                }),
              ),
          ],
        ),
        const SizedBox(height: 6),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: AxSpace.chip),
          child: Wrap(
            spacing: 2,
            runSpacing: 2,
            children: [
              if (!adds) _FolderKey(d),
              if (d.access != Access.read || d.plan)
                AxSelect<bool>(
                  label: 'Works in',
                  quiet: true,
                  icon: AxIcon.branch,
                  items: [
                    const AxMenuItem(false, 'In the folder', detail: 'changes it as it goes'),
                    AxMenuItem(
                      true,
                      'In a separate copy',
                      detail: own
                          ? 'changes wait until you apply them'
                          : 'it starts there, but is not held to it',
                    ),
                  ],
                  value: d.apart,
                  onChanged: (v) => d.change(() => d.apart = v),
                ),
              if (!own || d.words.isNotEmpty) _WordsKey(d),
            ],
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(AxSpace.edge, 8, AxSpace.edge, 0),
          child: Semantics(
            liveRegion: missing != null,
            child: Text(said, style: t.type.label.ink(c.ink3)),
          ),
        ),
        if (d.problem case final p?)
          Semantics(
            liveRegion: true,
            child: Padding(
              padding: const EdgeInsets.fromLTRB(AxSpace.edge, 10, AxSpace.edge, 0),
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
    );
    return LayoutBuilder(
      builder: (context, box) => SingleChildScrollView(
        child: ConstrainedBox(
          constraints: BoxConstraints(minHeight: box.maxHeight),
          child: Align(
            alignment: const Alignment(0, -.2),
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 700 + 2 * AxSpace.gutter),
              child: Padding(
                padding: const EdgeInsets.fromLTRB(AxSpace.gutter, 16, AxSpace.gutter, 24),
                child: page,
              ),
            ),
          ),
        ),
      ),
    );
  }

  /// Which program it is, for those who choose that themselves.
  ExecutionChoice _harness() {
    final s = d.store;
    final here = [
      for (final h in s.harnesses)
        if (h.program != null) h,
    ];
    // Those that most people would look for: that they are not there is said.
    final missing = [
      for (final h in s.harnesses)
        if (h.program == null && const {'claude', 'codex'}.contains(h.id)) h,
    ];
    return ExecutionChoice(
      'Harness',
      [
        const AxMenuItem('axiom', 'Axiom', detail: 'your accounts', mark: ServiceMark.axiom),
        for (final h in here)
          AxMenuItem(
            h.id,
            oneLine(h.name, 40),
            detail: h.own ? 'your own' : 'its own sign-in',
            mark: markOf(null, null, h.id),
          ),
        for (final h in missing)
          AxMenuItem(
            h.id,
            oneLine(h.name, 40),
            mark: markOf(null, null, h.id),
            enabled: false,
            why: h.problem != null && !h.problem!.contains('is not in any of')
                ? oneLine(h.problem!, 80)
                : 'not on this computer',
          ),
      ],
      d.harness,
      d.runWith,
    );
  }
}

/// Where it works: one that was worked in before, one that is written, or one that is looked for in
/// the system's own window.
class _FolderKey extends StatelessWidget {
  const _FolderKey(this.d);
  final StartDraft d;

  Future<void> _look(BuildContext context) async {
    final f = await chooseFolder(wholePath(d.folder.text.trim()));
    if (f != null && context.mounted) d.change(() => d.folder.text = f);
  }

  @override
  Widget build(BuildContext context) {
    final at = d.folder.text.trim();
    final name = oneLine(literal(_tail(at)), 60);
    return AxPopover(
      width: 360,
      anchor: (context, open, toggle) => AxChip(
        label: at.isEmpty ? 'Folder: none' : 'Folder: ${oneLine(literal(at), 200)}',
        words: at.isEmpty ? 'Choose a folder' : name,
        icon: AxIcon.folder,
        calls: at.isEmpty,
        open: open,
        onPressed: toggle,
      ),
      panel: (_, close) => _Folders(
        at: at,
        recent: recentFolders(d.store.sessions),
        onChosen: (f) {
          close();
          d.change(() => d.folder.text = f);
        },
        onLook: () {
          close();
          unawaited(_look(context));
        },
      ),
    );
  }
}

String _tail(String f) => f.split(RegExp(r'[/\\]')).lastWhere((x) => x.isNotEmpty, orElse: () => f);
String _head(String f) {
  final t = _tail(f);
  return oneLine(literal(f.endsWith(t) ? f.substring(0, f.length - t.length) : f), 60);
}

class _Folders extends StatefulWidget {
  const _Folders({
    required this.at,
    required this.recent,
    required this.onChosen,
    required this.onLook,
  });
  final String at;
  final List<String> recent;
  final ValueChanged<String> onChosen;
  final VoidCallback onLook;
  @override
  State<_Folders> createState() => _FoldersState();
}

class _FoldersState extends State<_Folders> {
  final _text = TextEditingController();
  @override
  void initState() {
    super.initState();
    _text.addListener(() => setState(() {}));
  }

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  String get _words => _text.text.trim();

  /// A path, and not a part of a name.
  bool get _path => RegExp(r'^(/|~|\\|[A-Za-z]:)').hasMatch(_words);
  List<String> get _fit => [
    for (final f in {if (widget.at.isNotEmpty) widget.at, ...widget.recent})
      if (f.toLowerCase().contains(_words.toLowerCase())) f,
  ];

  void _enter() {
    if (_path) {
      widget.onChosen(_words);
    } else if (_fit.firstOrNull case final f?) {
      widget.onChosen(f);
    }
  }

  @override
  Widget build(BuildContext context) {
    final words = _words, path = _path, fit = _fit;
    void down() => FocusScope.of(context).nextFocus();
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.arrowDown): down,
        const SingleActivator(LogicalKeyboardKey.arrowUp): () =>
            FocusScope.of(context).previousFocus(),
      },
      child: Settle(
        // What comes under a pointer by itself is not what is pressed.
        stamp: Object.hash(path, Object.hashAll(fit)),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            AxField(
              label: 'Folder',
              controller: _text,
              placeholder: 'Find a folder, or write its path',
              autofocus: true,
              onSubmitted: (_) => _enter(),
            ),
            const SizedBox(height: 5),
            if (path && !fit.contains(words))
              _Row(
                'Use ${oneLine(literal(words), 80)}',
                icon: AxIcon.folder,
                onPressed: () => widget.onChosen(words),
              ),
            for (final f in fit)
              _Row(
                oneLine(literal(_tail(f)), 60),
                detail: _head(f),
                chosen: f == widget.at,
                icon: AxIcon.folder,
                onPressed: () => widget.onChosen(f),
              ),
            if (fit.isEmpty && !path && words.isNotEmpty)
              Padding(
                padding: const EdgeInsets.fromLTRB(AxSpace.row, 6, AxSpace.row, 6),
                child: Text(
                  'None fits',
                  style: Ax.of(context).type.small.ink(Ax.of(context).colors.ink3),
                ),
              ),
            const Padding(
              padding: EdgeInsets.symmetric(horizontal: AxSpace.row, vertical: 4),
              child: Hairline(),
            ),
            _Row('Choose…', icon: AxIcon.search, onPressed: widget.onLook),
          ],
        ),
      ),
    );
  }
}

class _Row extends StatelessWidget {
  const _Row(
    this.words, {
    required this.icon,
    required this.onPressed,
    this.detail,
    this.chosen = false,
  });
  final String words;
  final String? detail;
  final AxIcon icon;
  final bool chosen;
  final VoidCallback onPressed;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Pressable(
      label: words,
      hint: detail,
      selected: chosen,
      onPressed: onPressed,
      builder: (_, s) => Ground(
        s.hovered || s.focused ? c.hover : null,
        child: ConstrainedBox(
          constraints: BoxConstraints(minHeight: t.density.row),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: AxSpace.row),
            child: Row(
              children: [
                AxGlyph(icon, color: c.ink3),
                const SizedBox(width: 9),
                Expanded(
                  child: Text.rich(
                    TextSpan(
                      text: words,
                      children: [
                        if (detail case final w?)
                          TextSpan(text: '   $w', style: t.type.small.ink(c.ink3)),
                      ],
                    ),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: t.type.ui,
                  ),
                ),
                if (chosen) ...[const SizedBox(width: 8), AxGlyph(AxIcon.check, color: c.liveText)],
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// What the folder itself says to agents, and whether that is followed. Nothing of it is, unless a
/// person has said so, and what is followed has been shown.
class _WordsKey extends StatelessWidget {
  const _WordsKey(this.d);
  final StartDraft d;

  @override
  Widget build(BuildContext context) {
    final own = d.harness == 'axiom', other = d.delegated;
    final followed = [
      for (final w in d.words)
        if (d.follow.contains(w.path)) w.path,
    ];
    final must = !own && (other?.needsTrust ?? false) && !d.trust;
    final words = own
        ? switch (followed.length) {
            0 => 'No instructions',
            1 => oneLine(literal(followed.first), 40),
            final n => '$n instructions',
          }
        : d.trust
        ? 'Folder settings'
        : 'No folder settings';
    return AxPopover(
      width: 440,
      anchor: (context, open, toggle) => AxChip(
        label: '${own ? 'Instructions' : 'Folder settings'}: $words',
        words: words,
        icon: must ? AxIcon.warn : AxIcon.file,
        calls: must,
        open: open,
        onPressed: toggle,
      ),
      // It is set while it is open.
      panel: (context, _) => ListenableBuilder(
        listenable: d,
        builder: (context, _) => Padding(
          padding: const EdgeInsets.fromLTRB(AxSpace.row, 4, AxSpace.row, AxSpace.row),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: own
                ? [
                    for (final (i, w) in d.words.indexed) ...[
                      if (i > 0) const SizedBox(height: 12),
                      _file(context, w),
                    ],
                  ]
                : [
                    OptionLine(
                      label: 'Use this folder\'s agent settings',
                      note: other == null
                          ? 'It is not on this computer any more.'
                          : other.needsTrust
                          ? '${other.name} cannot be told to ignore them. They can name programs that '
                                'it starts.'
                          : 'They can name programs that it starts.',
                      value: d.trust,
                      onChanged: (v) => d.change(() => d.trust = v),
                    ),
                  ],
          ),
        ),
      ),
    );
  }

  Widget _file(BuildContext context, Instructions w) {
    final t = Ax.of(context), c = t.colors;
    return OptionLine(
      label: 'Follow ${w.path}',
      note: d.open == w.path ? null : 'Instructions for agents, as they are now.',
      value: d.follow.contains(w.path),
      onChanged: (v) => d.change(() {
        v ? d.follow.add(w.path) : d.follow.remove(w.path);
        // Read first: what is followed has been shown.
        if (v) d.open = w.path;
      }),
      trailing: AxButton(
        label: d.open == w.path ? 'Hide' : 'View',
        tone: AxTone.quiet,
        onPressed: () => d.change(() => d.open = d.open == w.path ? null : w.path),
      ),
      below: d.open != w.path
          ? null
          : Padding(
              padding: const EdgeInsets.only(top: 8),
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
    );
  }
}
