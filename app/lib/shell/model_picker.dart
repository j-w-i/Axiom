import 'dart:async';

import 'package:flutter/semantics.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../host/host_store.dart';
import '../host/proto.dart';
import '../patterns/execution.dart';
import '../patterns/model_name.dart';
import '../transcript/safe_text.dart';
import '../ui/controls.dart';
import '../ui/editable.dart';
import '../ui/icons.dart';
import '../ui/marks.dart';
import '../ui/prefs.dart';
import '../ui/press.dart';
import '../ui/status.dart' show AgentState, AxLamp;
import '../ui/surface.dart';
import '../ui/tokens.dart';
import 'offers.dart';
import 'start.dart';

/// What an agent runs on, as it is now and as it can be told to be.
class Running {
  const Running({
    required this.harness,
    required this.connection,
    required this.model,
    this.effort = '',
    this.thinking,
    this.speed = '',
  });
  final String harness, model, effort, speed;
  final String? connection;
  final bool? thinking;
  String get key => offerKey(harness, connection, model);
}

/// The key that names a model and opens what can be chosen in its place.
class ModelKey extends StatelessWidget {
  const ModelKey(
    this.store,
    this.now, {
    required this.of,
    required this.onModel,
    required this.onEffort,
    required this.onThinking,
    required this.onSpeed,
    super.key,
    this.onOpen,
    this.keeps = false,
    this.costs,
    this.advanced = const [],
    this.onModels,
  });
  final HostStore store;
  final Running now;

  /// Whose it is. What is open closes when the key becomes another's.
  final Object of;

  /// It keeps its harness: an agent that is at work already.
  final bool keeps;
  final ValueChanged<Offer> onModel;
  final ValueChanged<String> onEffort, onSpeed;
  final ValueChanged<bool?> onThinking;
  final VoidCallback? onOpen;

  /// What another model costs, in words, if it does: it is asked about before it is changed.
  final String? Function(Offer)? costs;

  /// What else can be chosen, for those who want to.
  final List<ExecutionChoice> advanced;

  /// Goes to where it is said which models are offered.
  final VoidCallback? onModels;

  @override
  Widget build(BuildContext context) {
    final sources = sourcesOf(store, harness: keeps ? now.harness : null);
    final source = sources
        .where((s) => s.harness == now.harness && s.connection == now.connection)
        .firstOrNull;
    final known = source?.models.where((o) => o.key == now.key).firstOrNull;
    final program = now.harness != 'axiom';
    final name = now.model.isEmpty && !program
        ? 'Choose a model'
        : known?.name ?? (now.model.isEmpty ? 'Default model' : modelName(now.model));
    return ExecutionKey(
      const [],
      of: of,
      onOpen: onOpen,
      width: 440,
      mark: now.model.isEmpty && !program
          ? null
          : markOf(source?.provider, now.model.isEmpty ? null : now.model, now.harness),
      words: [
        oneLine(name, 60),
        // Whose it is, where that is not plain.
        if (program)
          oneLine(store.harnessName(now.harness), 40)
        else if (sources.length > 1 && source != null && now.model.isNotEmpty)
          oneLine(source.name, 40),
        if (now.effort.isNotEmpty) effortName(now.effort),
        if (now.speed.isNotEmpty) now.speed == 'fast' ? 'Fast' : oneLine(now.speed, 20),
      ],
      panel: (context, close) => ModelPicker(this, close: close),
    );
  }
}

/// A question about what a change costs.
class _Asked {
  const _Asked(this.words, this.keep, this.change, this.apply);
  final String words, keep, change;
  final VoidCallback apply;
}

/// Every model of every account and program, found by what it is called; under them what the chosen
/// one can be told.
class ModelPicker extends StatefulWidget {
  const ModelPicker(this.of, {required this.close, super.key});
  final ModelKey of;
  final VoidCallback close;

  /// Of each account or program, until something is looked for.
  static const most = 8;
  @override
  State<ModelPicker> createState() => _ModelPickerState();
}

class _ModelPickerState extends State<ModelPicker> {
  final _words = TextEditingController(), _scroll = ScrollController();
  final _field = FocusNode(debugLabel: 'models');
  final _all = <String>{};
  _Asked? _asked;
  ModelKey get of => widget.of;

  @override
  void initState() {
    super.initState();
    _words.addListener(() => setState(() {}));
    WidgetsBinding.instance.addPostFrameCallback((_) => mounted ? _field.requestFocus() : null);
  }

  @override
  void dispose() {
    _field.dispose();
    _words.dispose();
    _scroll.dispose();
    super.dispose();
  }

  List<String> get _looked =>
      _words.text.toLowerCase().split(' ').where((w) => w.isNotEmpty).toList();

  void _choose(Offer o) {
    if (o.key == of.now.key) return widget.close();
    final cost = of.costs?.call(o);
    if (cost == null) return of.onModel(o);
    setState(() => _asked = _Asked(cost, 'Keep it', 'Change to ${o.name}', () => of.onModel(o)));
  }

  void _favourite(BuildContext context, Offer o) {
    final all = PrefsScope.maybeOf(context);
    if (all == null) return;
    all.value = all.value.offering(o.key, favourite: !all.value.favourites.contains(o.key));
  }

  @override
  Widget build(BuildContext context) {
    final p = PrefsScope.of(context);
    return ListenableBuilder(listenable: of.store, builder: (context, _) => _build(context, p));
  }

  Widget _build(BuildContext context, Prefs p) {
    final t = Ax.of(context), c = t.colors, s = of.store, now = of.now;
    final sources = sourcesOf(s, harness: of.keeps ? now.harness : null);
    final words = _looked;
    bool fits(Source x, Offer o) =>
        words.every('${o.name} ${o.model.id} ${x.name} ${x.note}'.toLowerCase().contains);
    final rows = <Widget>[];
    // What Enter takes, while something is looked for.
    Offer? first;
    final favourites = [
      for (final x in sources)
        for (final o in x.models)
          if (p.favourites.contains(o.key) && fits(x, o)) (x, o),
    ];
    if (favourites.isNotEmpty) {
      rows.add(const _Heading('Favourites', ''));
      for (final (x, o) in favourites) {
        if (words.isNotEmpty) first ??= o;
        rows.add(_row(context, p, o, whose: sources.length > 1 ? x.name : null));
      }
    }
    var found = favourites.length;
    for (final x in sources) {
      final all = [
        for (final o in arranged(x.models, p, chosen: now.key))
          if (!p.favourites.contains(o.key) && fits(x, o)) o,
      ];
      found += all.length;
      if (all.isEmpty && (words.isNotEmpty || x.models.isNotEmpty)) continue;
      final whole = words.isNotEmpty || _all.contains('${x.harness} ${x.connection}');
      final shown = whole ? all.take(60).toList() : all.take(ModelPicker.most).toList();
      final chosen = all.where((o) => o.key == now.key).firstOrNull;
      if (chosen != null && !shown.contains(chosen)) shown.add(chosen);
      rows.add(_Heading(x.name, x.note));
      if (x.models.isEmpty) {
        rows.add(_None(s, x.connection!, key: ValueKey('none ${x.connection}')));
      }
      for (final o in shown) {
        if (words.isNotEmpty) first ??= o;
        rows.add(_row(context, p, o));
      }
      if (all.length > shown.length) {
        rows.add(
          _Line(
            label: 'Show ${all.length - shown.length} more of ${oneLine(x.name, 40)}',
            onPressed: () => setState(() => _all.add('${x.harness} ${x.connection}')),
            child: Padding(
              padding: const EdgeInsets.only(left: 26),
              child: Text('${all.length - shown.length} more', style: t.type.ui.ink(c.ink3)),
            ),
          ),
        );
      }
    }
    // What is in no list can still be there.
    final written = _words.text.trim();
    final account =
        sources
            .where((x) => x.connection == now.connection && x.harness == now.harness)
            .firstOrNull ??
        sources.firstOrNull;
    if (found == 0 && written.isNotEmpty && account != null) {
      final o = first = Offer(
        account.harness,
        account.connection,
        account.provider,
        ModelInfo(written),
      );
      rows.add(
        _Line(
          label: 'Use $written',
          onPressed: () => _choose(o),
          child: Text.rich(
            TextSpan(
              text: 'No model fits. Use ',
              style: t.type.ui.ink(c.ink3),
              children: [
                TextSpan(text: oneLine(written, 60), style: t.type.uiMedium.ink(c.ink1)),
                TextSpan(text: ' of ${oneLine(account.name, 40)}'),
              ],
            ),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
          ),
        ),
      );
    }
    void down() => FocusScope.of(context).nextFocus();
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.arrowDown): down,
        const SingleActivator(LogicalKeyboardKey.arrowUp): () =>
            FocusScope.of(context).previousFocus(),
      },
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(AxSpace.row, 5, AxSpace.row, 7),
            child: Row(
              children: [
                AxGlyph(AxIcon.search, color: c.ink3),
                const SizedBox(width: 10),
                Expanded(
                  child: AxEditable(
                    label: 'Search models',
                    placeholder: 'Search models',
                    controller: _words,
                    focusNode: _field,
                    style: t.type.ui,
                    maxLines: 1,
                    onSubmitted: (_) => first == null ? null : _choose(first),
                  ),
                ),
              ],
            ),
          ),
          const Padding(
            padding: EdgeInsets.symmetric(horizontal: AxSpace.row),
            child: Hairline(),
          ),
          ConstrainedBox(
            constraints: const BoxConstraints(maxHeight: 360),
            child: Settle(
              // What comes under a pointer by itself is not what is pressed.
              stamp: Object.hash(_words.text, found, _all.length),
              child: Stack(
                children: [
                  ListView.builder(
                    controller: _scroll,
                    shrinkWrap: true,
                    padding: const EdgeInsets.only(bottom: 10),
                    itemCount: rows.length,
                    itemBuilder: (_, i) => rows[i],
                  ),
                  Positioned(
                    left: 0,
                    right: 0,
                    bottom: 0,
                    child: Fade(up: false, height: 12, ground: c.overlay.fill),
                  ),
                ],
              ),
            ),
          ),
          if (_asked case final q?)
            _Ask(
              q,
              onKeep: () => setState(() => _asked = null),
              onChange: () {
                setState(() => _asked = null);
                q.apply();
              },
            )
          else
            ..._told(context),
          if (of.onModels case final f?) ...[
            _rule,
            _Line(
              label: 'Manage models',
              onPressed: () {
                widget.close();
                f();
              },
              child: Text('Manage models', style: t.type.ui.ink(c.ink2)),
            ),
          ],
        ],
      ),
    );
  }

  static const _rule = Padding(
    padding: EdgeInsets.symmetric(horizontal: AxSpace.row, vertical: 4),
    child: Hairline(),
  );

  /// What the chosen one can be told: only what is offered.
  List<Widget> _told(BuildContext context) {
    final now = of.now, s = of.store;
    final info = infoOf(s, now.harness, now.connection, now.model);
    final program = s.harnesses.where((h) => h.id == now.harness).firstOrNull;
    final efforts =
        info?.efforts ?? ((program?.effort ?? false) ? const ['low', 'medium', 'high'] : const []);
    final speeds = {...?info?.speeds, if (now.speed.isNotEmpty) now.speed};
    final all = <Widget>[
      if (efforts.isNotEmpty || now.effort.isNotEmpty)
        _Part(
          'Effort',
          AxSegments<String>(
            label: 'Effort',
            items: [
              for (final e in {'', ...efforts, now.effort}) (e, effortName(e), null),
            ],
            value: now.effort,
            onChanged: of.onEffort,
          ),
        ),
      if ((info?.thinking ?? false) || now.thinking != null)
        _Part(
          'Thinking',
          AxSegments<String>(
            label: 'Thinking',
            items: const [('', 'Default', null), ('on', 'On', null), ('off', 'Off', null)],
            value: switch (now.thinking) {
              null => '',
              true => 'on',
              false => 'off',
            },
            onChanged: (v) => of.onThinking(v.isEmpty ? null : v == 'on'),
          ),
        ),
      if (speeds.isNotEmpty)
        _Part(
          'Speed',
          AxSegments<String>(
            label: 'Speed',
            items: [
              ('', 'Standard', null),
              for (final v in speeds) (v, v == 'fast' ? 'Fast' : oneLine(v, 20), null),
            ],
            value: now.speed,
            onChanged: (v) {
              if (v.isEmpty || v == now.speed) return of.onSpeed(v);
              setState(
                () => _asked = _Asked(
                  now.harness == 'axiom'
                      ? 'Fast answers cost more for each token. What was cached at one speed is not at '
                            'the other.'
                      : 'Fast answers cost more for each token. On a plan they are paid for by usage '
                            'credits, and not by the plan.',
                  'Keep it',
                  'Change to Fast',
                  () => of.onSpeed(v),
                ),
              );
            },
          ),
        ),
      if (of.advanced.isNotEmpty) ExecutionPanel(of.advanced),
    ];
    return [if (all.isNotEmpty) _rule, ...all];
  }

  Widget _row(BuildContext context, Prefs p, Offer o, {String? whose}) {
    final t = Ax.of(context), c = t.colors;
    final on = o.key == of.now.key, star = p.favourites.contains(o.key);
    final mark = markOf(o.provider, o.byDefault ? null : o.model.id, o.harness);
    void toggle() => _favourite(context, o);
    return CallbackShortcuts(
      key: ValueKey('$whose ${o.key}'),
      bindings: {const SingleActivator(LogicalKeyboardKey.keyF): toggle},
      child: Pressable(
        label: o.name,
        hint: [?whose, if (o.model.context case final n?) '${contextName(n)} context'].join(', '),
        selected: on,
        actions: {
          CustomSemanticsAction(label: star ? 'Remove from favourites' : 'Add to favourites'):
              toggle,
        },
        onPressed: () => _choose(o),
        builder: (context, s) => Ground(
          on
              ? c.selected
              : s.hovered || s.focused
              ? c.hover
              : null,
          child: ConstrainedBox(
            constraints: BoxConstraints(minHeight: t.density.row + 4),
            child: Padding(
              padding: const EdgeInsets.only(left: AxSpace.row, right: 4),
              child: Row(
                children: [
                  SizedBox(width: 16, child: mark == null ? null : AxServiceMark(mark, size: 15)),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Text.rich(
                      TextSpan(
                        text: oneLine(o.name, 80),
                        style: on ? t.type.uiMedium : t.type.ui,
                        children: [
                          if (whose != null)
                            TextSpan(
                              text: '   ${oneLine(whose, 24)}',
                              style: t.type.small.ink(c.ink3),
                            ),
                        ],
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                  const SizedBox(width: 10),
                  if (o.model.context case final n?) ...[
                    Text(contextName(n), style: t.type.small.ink(c.ink3)),
                    const SizedBox(width: 8),
                  ],
                  if (on) ...[AxGlyph(AxIcon.check, color: c.ink1), const SizedBox(width: 2)],
                  // The keyboard has F, and a screen reader an act of the row's.
                  ExcludeFocus(
                    child: ExcludeSemantics(
                      child: Opacity(
                        opacity: star || s.hovered || s.focused ? 1 : 0,
                        child: AxButton.icon(
                          AxIcon.star,
                          label: star ? 'Remove from favourites' : 'Add to favourites',
                          tone: AxTone.quiet,
                          size: 24,
                          iconFilled: star,
                          onPressed: toggle,
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _Heading extends StatelessWidget {
  const _Heading(this.name, this.note);
  final String name, note;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Semantics(
      header: true,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(AxSpace.row, 10, AxSpace.row, 4),
        child: Text.rich(
          TextSpan(
            text: oneLine(name, 60),
            style: t.type.smallStrong.ink(c.ink3),
            children: [
              if (note.isNotEmpty)
                TextSpan(text: '   ${oneLine(note, 60)}', style: t.type.small.ink(c.ink3)),
            ],
          ),
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
        ),
      ),
    );
  }
}

/// An account whose models nobody has listed yet.
class _None extends StatefulWidget {
  const _None(this.store, this.connection, {super.key});
  final HostStore store;
  final String connection;
  @override
  State<_None> createState() => _NoneState();
}

class _NoneState extends State<_None> {
  bool _busy = false;
  String? _problem;
  Future<void> _ask() async {
    setState(() {
      _busy = true;
      _problem = null;
    });
    try {
      final all = await widget.store.models(widget.connection);
      if (all.isEmpty) _problem = 'It lists none. Search for a model by its name.';
    } on Object catch (e) {
      _problem = wordsOf(e);
    }
    if (mounted) setState(() => _busy = false);
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return _Line(
      label: 'List its models',
      onPressed: _busy ? null : () => unawaited(_ask()),
      child: Padding(
        padding: const EdgeInsets.only(left: 26),
        child: Text(
          _busy ? 'Listing' : visible(_problem ?? 'List its models'),
          maxLines: 2,
          overflow: TextOverflow.ellipsis,
          style: t.type.ui.ink(_problem == null ? c.ink2 : c.ink3),
        ),
      ),
    );
  }
}

class _Line extends StatelessWidget {
  const _Line({required this.label, required this.onPressed, required this.child});
  final String label;
  final VoidCallback? onPressed;
  final Widget child;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Pressable(
      label: label,
      onPressed: onPressed,
      builder: (context, s) => Ground(
        s.hovered || s.focused ? c.hover : null,
        child: ConstrainedBox(
          constraints: BoxConstraints(minHeight: t.density.row + 2),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: AxSpace.row),
            child: Align(alignment: Alignment.centerLeft, heightFactor: 1, child: child),
          ),
        ),
      ),
    );
  }
}

/// Something the model can be told.
class _Part extends StatelessWidget {
  const _Part(this.name, this.control);
  final String name;
  final Widget control;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: AxSpace.row, vertical: 3),
      child: Row(
        children: [
          SizedBox(
            width: 66,
            child: ExcludeSemantics(child: Text(name, style: t.type.label.ink(c.ink3))),
          ),
          Expanded(
            child: SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              reverse: true,
              child: control,
            ),
          ),
        ],
      ),
    );
  }
}

/// What a change costs, and the two answers.
class _Ask extends StatefulWidget {
  const _Ask(this.asked, {required this.onKeep, required this.onChange});
  final _Asked asked;
  final VoidCallback onKeep, onChange;
  @override
  State<_Ask> createState() => _AskState();
}

class _AskState extends State<_Ask> {
  final _keep = FocusNode();
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => mounted ? _keep.requestFocus() : null);
  }

  @override
  void dispose() {
    _keep.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors, q = widget.asked;
    return Semantics(
      container: true,
      liveRegion: true,
      explicitChildNodes: true,
      label: q.words,
      child: Settle(
        stamp: q,
        fresh: true,
        child: Padding(
          padding: const EdgeInsets.only(top: 6),
          child: Ground(
            c.hover,
            child: Padding(
              padding: const EdgeInsets.all(AxSpace.row),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      const Padding(
                        padding: EdgeInsets.only(top: 5),
                        child: ExcludeSemantics(child: AxLamp(AgentState.asking)),
                      ),
                      const SizedBox(width: 9),
                      Expanded(
                        child: ExcludeSemantics(
                          child: Text(q.words, style: t.type.label.ink(c.ink1)),
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 8),
                  Wrap(
                    spacing: 6,
                    runSpacing: 6,
                    alignment: WrapAlignment.end,
                    children: [
                      AxButton(
                        label: q.keep,
                        tone: AxTone.quiet,
                        focusNode: _keep,
                        onPressed: widget.onKeep,
                      ),
                      AxButton(label: q.change, tone: AxTone.primary, onPressed: widget.onChange),
                    ],
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
