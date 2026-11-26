import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/semantics.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../host/host_store.dart';
import '../host/proto.dart';
import '../patterns/execution.dart';
import '../patterns/model_name.dart';
import '../transcript/safe_text.dart';
import '../ui/commands.dart';
import '../ui/controls.dart';
import '../ui/editable.dart';
import '../ui/icons.dart';
import '../ui/marks.dart';
import '../ui/overlay.dart' show AxHint;
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
    this.speed = '',
  });
  final String harness, model, effort, speed;
  final String? connection;
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
    // What it tries at: as it was told, else as it does when told none, where that is known.
    final effort = now.effort.isNotEmpty
        ? now.effort
        : infoOf(store, now.harness, now.connection, now.model)?.defaultEffort ?? '';
    final name = now.model.isEmpty && !program
        ? 'Choose a model'
        : known?.name ?? (now.model.isEmpty ? 'Default model' : modelName(now.model));
    return ExecutionKey(
      const [],
      of: of,
      onOpen: onOpen,
      width: ModelPicker.width,
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
        if (effort.isNotEmpty) effortName(effort),
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

/// Every model of every account and program: whose at the left, which in the middle, and at the right
/// and below what the chosen one can be told.
class ModelPicker extends StatefulWidget {
  const ModelPicker(this.of, {required this.close, super.key});
  final ModelKey of;
  final VoidCallback close;

  /// Of one account or program, until more are asked for.
  static const most = 8;
  static const width = 600.0, height = 316.0, _rail = 48.0, _efforts = 160.0;
  @override
  State<ModelPicker> createState() => _ModelPickerState();
}

class _ModelPickerState extends State<ModelPicker> {
  final _words = TextEditingController(), _scroll = ScrollController();
  final _field = FocusNode(debugLabel: 'models'), _top = FocusNode(debugLabel: 'first model');
  _Asked? _asked;

  /// As tall as it has been while open: what is looked for does not make it jump.
  double _tall = 0;

  /// Whose models are shown; those that are favourites; null: nobody has said.
  String? _whose;
  static const _favourites = '';
  bool _more = false;
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
    _top.dispose();
    _words.dispose();
    _scroll.dispose();
    super.dispose();
  }

  List<String> get _looked =>
      _words.text.toLowerCase().split(' ').where((w) => w.isNotEmpty).toList();

  static String _id(Source x) => '${x.harness} ${x.connection ?? ''}';

  void _choose(Offer o) {
    if (o.key == of.now.key) return widget.close();
    final cost = of.costs?.call(o);
    if (cost == null) return of.onModel(o);
    final keep = of.now.model.isEmpty ? 'Keep it' : 'Keep ${oneLine(modelName(of.now.model), 30)}';
    setState(() => _asked = _Asked(cost, keep, 'Change to ${o.name}', () => of.onModel(o)));
  }

  void _favourite(Offer o) {
    final all = PrefsScope.maybeOf(context);
    if (all == null) return;
    all.value = all.value.offering(o.key, favourite: !all.value.favourites.contains(o.key));
  }

  void _turn(String whose) => setState(() {
    _whose = whose;
    _more = false;
    _words.clear();
  });

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
    final starred = [
      for (final x in sources)
        for (final o in x.models)
          if (p.favourites.contains(o.key)) (x, o),
    ];
    final mine = sources
        .where((x) => x.harness == now.harness && x.connection == now.connection)
        .firstOrNull;
    final whose =
        _whose ??
        (mine != null && now.model.isNotEmpty || starred.isEmpty
            ? (mine ?? sources.firstOrNull)?.let(_id)
            : _favourites);
    final source = sources.where((x) => _id(x) == whose).firstOrNull;
    // What is looked for is looked for everywhere.
    final all = words.isNotEmpty
        ? [
            for (final x in sources)
              for (final o in arranged(x.models, p, chosen: now.key))
                if (fits(x, o)) (x, o),
          ]
        : source == null
        ? starred
        : [for (final o in arranged(source.models, p, chosen: now.key)) (source, o)];
    final shown = all.take(words.isNotEmpty || _more ? 100 : ModelPicker.most).toList();
    final chosen = all.where((e) => e.$2.key == now.key).firstOrNull;
    if (chosen != null && !shown.contains(chosen)) shown.add(chosen);
    // What is in no list can still be there.
    final written = _words.text.trim(), account = mine ?? sources.firstOrNull;
    final typed = all.isEmpty && written.isNotEmpty && account != null
        ? Offer(account.harness, account.connection, account.provider, ModelInfo(written))
        : null;
    final first = words.isEmpty ? null : shown.firstOrNull?.$2 ?? typed;
    void to(TraversalDirection d) => FocusScope.of(context).focusedChild?.focusInDirection(d);

    // One grid: the search's line, then in each group a heading of one height and rows of one.
    final scaler = MediaQuery.textScalerOf(context);
    double line(TextStyle s) => scaler.scale(s.fontSize ?? 14) * (s.height ?? 1.3);
    final rowH = math.max(t.density.row + 6, line(t.type.ui) + 12);
    final top = line(t.type.ui) + 18, headH = line(t.type.smallStrong) + 14;
    // Written into: raised a tone and lit, with a short shade that stays off what is under it, and
    // the ring of what has the keyboard.
    final search = ListenableBuilder(
      listenable: _field,
      builder: (context, child) =>
          Surface(c.slab, radius: _inner, ring: _field.hasFocus ? c.focus : null, child: child),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: AxSpace.row),
        child: Row(
          children: [
            AxGlyph(AxIcon.search, color: c.ink3),
            const SizedBox(width: 10),
            Expanded(
              child: CallbackShortcuts(
                bindings: {const SingleActivator(LogicalKeyboardKey.arrowDown): _top.requestFocus},
                child: AxEditable(
                  label: 'Search models',
                  placeholder: 'Search models',
                  controller: _words,
                  focusNode: _field,
                  style: t.type.ui,
                  maxLines: 1,
                  onSubmitted: (_) {
                    if (first == null) return;
                    _choose(first);
                    // What has a price is asked about first.
                    if (_asked == null) widget.close();
                  },
                ),
              ),
            ),
          ],
        ),
      ),
    );
    final heading = words.isNotEmpty
        ? _Heading('Every account', '', height: headH)
        : source != null
        ? _Heading(source.name, source.note, height: headH)
        : _Heading('Favourites', '', height: headH);
    final rows = <Widget>[
      if (words.isEmpty && source != null && source.models.isEmpty)
        _None(s, source.connection!, key: ValueKey('none ${source.connection}')),
      for (final (i, (x, o)) in shown.indexed)
        _row(
          p,
          o,
          key: i < 9 ? i + 1 : null,
          node: i == 0 ? _top : null,
          whose: (words.isNotEmpty || source == null) && sources.length > 1 ? x.name : null,
          rowH: rowH,
        ),
      if (all.length > shown.length)
        _Line(
          label: 'Show ${all.length - shown.length} more',
          onPressed: () => setState(() => _more = true),
          child: Padding(
            padding: const EdgeInsets.only(left: 26),
            child: Text('Show more', style: t.type.ui.ink(c.ink3)),
          ),
        ),
      if (typed != null)
        _Line(
          label: 'Use $written',
          onPressed: () => _choose(typed),
          child: Text.rich(
            TextSpan(
              text: 'No model fits. Use ',
              style: t.type.ui.ink(c.ink3),
              children: [
                TextSpan(text: oneLine(written, 60), style: t.type.uiMedium.ink(c.ink1)),
                TextSpan(text: ' of ${oneLine(account?.name ?? '', 40)}'),
              ],
            ),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
          ),
        ),
    ];
    final list = Settle(
      // What comes under a pointer by itself is not what is pressed.
      stamp: Object.hash(_words.text, whose, all.length, _more),
      child: ListView.builder(
        controller: _scroll,
        padding: EdgeInsets.zero,
        itemCount: rows.length,
        itemBuilder: (_, i) => rows[i],
      ),
    );
    // Accounts, programs, and last the favourites, so that nothing moves when there are some.
    final stars = starred.isNotEmpty || whose == _favourites;
    final rail = _Rail(
      [
        for (final (i, x) in sources.indexed)
          _Stop(
            _id(x),
            x.note.isEmpty ? x.name : '${x.name}, ${x.note}',
            mark: markOf(x.provider, null, x.harness),
            letters: _letters(x, sources),
            // Apart from each other by room.
            apart: i > 0 && x.harness != 'axiom' && sources[i - 1].harness == 'axiom',
          ),
        if (stars) _Stop(_favourites, 'Favourites', icon: AxIcon.star, apart: sources.isNotEmpty),
      ],
      // Where one looks everywhere, nobody's is shown.
      at: words.isEmpty ? whose : null,
      onTurn: _turn,
    );
    return CallbackShortcuts(
      bindings: {
        for (final (k, d) in const [
          (LogicalKeyboardKey.arrowDown, TraversalDirection.down),
          (LogicalKeyboardKey.arrowUp, TraversalDirection.up),
          (LogicalKeyboardKey.arrowLeft, TraversalDirection.left),
          (LogicalKeyboardKey.arrowRight, TraversalDirection.right),
        ])
          SingleActivator(k): () => to(d),
        for (final (i, (_, o)) in shown.take(9).indexed)
          SingleActivator(_digits[i], meta: Command.isMac, control: !Command.isMac): () =>
              _choose(o),
      },
      child: LayoutBuilder(
        builder: (context, box) {
          // Larger words want the room the effort column would take.
          // Every effort a model takes stays in sight in its column down to a small window; below
          // that, and with larger words, they go below.
          final room = box.maxWidth / scaler.scale(1) >= 400;
          final effortsW = (box.maxWidth * .27).clamp(124.0, ModelPicker._efforts);
          // As tall as what is in it, up to where it scrolls; all groups end on one line.
          final stops = sources.length + (stars ? 1 : 0);
          final aparts =
              (stars && sources.isNotEmpty ? 1 : 0) +
              (sources.any((x) => x.harness == 'axiom') && sources.any((x) => x.harness != 'axiom')
                  ? 1
                  : 0);
          final listNeed = top + _gap + 2 * _pad + headH + rows.length * rowH + _pad;
          final need = [
            listNeed,
            stops * (scaler.scale(_Rail.stop) + 2) + aparts * _Rail.apart,
            if (room) top + _gap + 2 * _pad + headH + math.max(_efforts.length, 2) * rowH,
          ].reduce(math.max);
          _tall = math.min(ModelPicker.height * scaler.scale(1), math.max(_tall, need));
          final cut = listNeed > _tall + 1;
          // The groups as tall as the taller of them, not as the rail: below them, what floats.
          final groupH = math.min(
            _tall - top - _gap,
            math.max(
              listNeed - top - _gap,
              room && _efforts.isNotEmpty
                  ? 2 * _pad + headH + _efforts.length * rowH
                  : 2 * _pad + headH + 2 * rowH,
            ),
          );
          final groups = Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Expanded(
                child: _Group(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      heading,
                      Expanded(
                        child: Stack(
                          children: [
                            Positioned.fill(child: list),
                            // What scrolls goes out of sight gradually, where it does.
                            Positioned(
                              left: 0,
                              right: 0,
                              top: 0,
                              child: ListenableBuilder(
                                listenable: _scroll,
                                builder: (context, _) => _scroll.hasClients && _scroll.offset > 0
                                    ? Fade(up: true, height: 12, ground: c.field.fill)
                                    : const SizedBox.shrink(),
                              ),
                            ),
                            if (cut)
                              Positioned(
                                left: 0,
                                right: 0,
                                bottom: 0,
                                child: Fade(up: false, height: 12, ground: c.field.fill),
                              ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ),
              if (room) ...[
                const SizedBox(width: _gap),
                SizedBox(
                  width: effortsW,
                  child: _effort(context, headH: headH, rowH: rowH),
                ),
              ],
            ],
          );
          return Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              SizedBox(
                height: _tall,
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    // On what floats: two groups beside it, not three.
                    SizedBox(width: scaler.scale(ModelPicker._rail), child: rail),
                    const SizedBox(width: _gap),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          SizedBox(height: top, child: search),
                          const SizedBox(height: _gap),
                          SizedBox(height: groupH, child: groups),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
              // What else can be chosen, then the acts, which end it.
              if (of.advanced.isNotEmpty) ...[
                const SizedBox(height: _gap),
                ExecutionPanel(of.advanced, labels: _labels),
              ],
              // What is asked about stays in sight above the question.
              ..._below(context, efforts: !room),
              if (_asked case final q?)
                _Ask(
                  q,
                  onKeep: () => setState(() => _asked = null),
                  onChange: () {
                    setState(() => _asked = null);
                    q.apply();
                  },
                ),
            ],
          );
        },
      ),
    );
  }

  /// Between columns as from the edge of what floats; corners concentric with its own.
  static const _gap = AxSpace.menu, _inner = AxRadius.card - AxSpace.menu, _pad = 2.0;

  /// The column of names below: what they name begins where the models do, or, with the harness,
  /// where its name has room.
  double get _labels => of.advanced.isEmpty ? ModelPicker._rail + _gap - AxSpace.row : 64;

  static const _digits = [
    LogicalKeyboardKey.digit1,
    LogicalKeyboardKey.digit2,
    LogicalKeyboardKey.digit3,
    LogicalKeyboardKey.digit4,
    LogicalKeyboardKey.digit5,
    LogicalKeyboardKey.digit6,
    LogicalKeyboardKey.digit7,
    LogicalKeyboardKey.digit8,
    LogicalKeyboardKey.digit9,
  ];

  /// What tells one from others with the same mark, or stands for one that has none.
  static String? _letters(Source x, List<Source> all) {
    Object? mark(Source s) => markOf(s.provider, null, s.harness);
    final alone = all.where((s) => mark(s) == mark(x)).length == 1;
    if (mark(x) != null && alone) return null;
    final name = x.harness != 'axiom' && x.connection != null ? x.note : x.name;
    final words = name.split(RegExp(r'[^\p{L}\p{N}]+', unicode: true)).where((w) => w.isNotEmpty);
    final letters = words.length > 1
        ? words.take(2).map((w) => w.characters.first).join()
        : words.firstOrNull?.characters.take(2).toString() ?? '?';
    return letters.toUpperCase();
  }

  String? _providerOf(Running r) =>
      sourcesOf(of.store)
          .where((x) => x.harness == r.harness && x.connection == r.connection)
          .firstOrNull
          ?.provider;

  ModelInfo? get _info => infoOf(of.store, of.now.harness, of.now.connection, of.now.model);

  List<String> get _efforts {
    final program = of.store.harnesses.where((h) => h.id == of.now.harness).firstOrNull;
    final all =
        _info?.efforts ?? ((program?.effort ?? false) ? const ['low', 'medium', 'high'] : const []);
    return {...all, if (of.now.effort.isNotEmpty) of.now.effort}.toList();
  }

  /// Which it tries at when told none, where that is said.
  String? get _byDefault => _info?.defaultEffort;

  /// The one it tries at now: as it was told, else its default; empty where neither is known.
  String get _chosenEffort => of.now.effort.isNotEmpty ? of.now.effort : _byDefault ?? '';

  /// Choosing its default is telling it none, so that it goes on as its provider has it.
  void _effortTo(String e) => of.onEffort(e == _byDefault ? '' : e);

  Widget _effort(BuildContext context, {required double headH, required double rowH}) {
    final t = Ax.of(context), c = t.colors, now = of.now, all = _efforts;
    final whose = now.model.isEmpty ? '' : modelName(now.model, _info?.name);
    // Its check takes the hue of the model's service, as the model's does.
    final hue = markOf(_providerOf(now), now.model.isEmpty ? null : now.model, now.harness);
    final heading = _Heading('Effort', oneLine(whose, 30), height: headH);
    // With nothing to choose from there is no group: words on what floats, on the heading's line.
    if (all.isEmpty) {
      return Semantics(
        container: true,
        explicitChildNodes: true,
        label: 'Effort',
        child: Padding(
          padding: const EdgeInsets.all(_pad),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              heading,
              Flexible(
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: AxSpace.row, vertical: 6),
                  child: Text(
                    now.model.isEmpty && now.harness == 'axiom'
                        ? 'Choose a model first.'
                        : 'This model takes none.',
                    maxLines: 3,
                    overflow: TextOverflow.ellipsis,
                    style: t.type.label.ink(c.ink3),
                  ),
                ),
              ),
            ],
          ),
        ),
      );
    }
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: 'Effort',
      child: _Group(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            // Of which model: the one chosen, whichever account is looked at.
            heading,
            Expanded(
              child: ListView(
                padding: EdgeInsets.zero,
                children: [
                  for (final e in all)
                    Pressable(
                      label: e == _byDefault ? '${effortName(e)}, default' : effortName(e),
                      selected: e == _chosenEffort,
                      onPressed: () => _effortTo(e),
                      builder: (context, s) => _Chosen(
                        on: e == _chosenEffort,
                        hover: s.hovered || s.focused,
                        hue: hue?.color(c),
                        child: ConstrainedBox(
                          constraints: BoxConstraints(minHeight: rowH),
                          child: Padding(
                            padding: const EdgeInsets.symmetric(horizontal: AxSpace.row),
                            child: Row(
                              children: [
                                Expanded(
                                  child: Text.rich(
                                    TextSpan(
                                      text: effortName(e),
                                      style: e == _chosenEffort ? t.type.uiMedium : t.type.ui,
                                      children: [
                                        // Beside the one it tries at when told none.
                                        if (e == _byDefault)
                                          TextSpan(
                                            text: '  Default',
                                            style: t.type.small.ink(c.ink3),
                                          ),
                                      ],
                                    ),
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                  ),
                                ),
                                if (e == _chosenEffort)
                                  AxGlyph(AxIcon.check, color: hue?.color(c) ?? c.ink1),
                              ],
                            ),
                          ),
                        ),
                      ),
                    ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  /// What else the chosen one can be told, and the way to all of them.
  List<Widget> _below(BuildContext context, {required bool efforts}) {
    final now = of.now, info = _info;
    final speeds = {...?info?.speeds, if (now.speed.isNotEmpty) now.speed};
    final t = Ax.of(context), c = t.colors;
    // Each says what it is: no menu, which would stand outside what is open and close it.
    // Their names in one column with the harness's, so that what can be chosen begins on one line.
    Widget named(String name, Widget segments) => Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        ConstrainedBox(
          constraints: BoxConstraints(minWidth: MediaQuery.textScalerOf(context).scale(_labels)),
          child: Padding(
            padding: const EdgeInsets.only(right: 8),
            child: Text(name, maxLines: 1, softWrap: false, style: t.type.label.ink(c.ink3)),
          ),
        ),
        // However many there are, they stay in the picker: what does not fit scrolls sideways.
        Flexible(child: _Sideways(child: segments)),
      ],
    );
    final keys = <Widget>[
      if (efforts && _efforts.isNotEmpty)
        named(
          'Effort',
          AxSegments<String>(
            label: 'Effort',
            items: [
              for (final e in _efforts)
                (e, e == _byDefault ? '${effortName(e)} (default)' : effortName(e), null),
            ],
            value: _chosenEffort,
            onChanged: _effortTo,
          ),
        ),
      if (speeds.isNotEmpty)
        named(
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
                      ? 'Fast costs more for each token, and what was cached is not read at the other '
                            'speed.'
                      : 'Fast costs more for each token, and on a plan it is paid from usage credits.',
                  'Keep Standard',
                  'Change to Fast',
                  () => of.onSpeed(v),
                ),
              );
            },
          ),
        ),
    ];
    if (keys.isEmpty && of.onModels == null) return const [];
    return [
      Padding(
        padding: const EdgeInsets.fromLTRB(AxSpace.row, _gap, 0, 0),
        child: Row(
          children: [
            Expanded(
              child: Wrap(
                spacing: 8,
                runSpacing: 6,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: keys,
              ),
            ),
            if (of.onModels case final f?)
              AxButton(
                label: 'Manage models',
                onPressed: () {
                  widget.close();
                  f();
                },
              ),
          ],
        ),
      ),
    ];
  }

  Widget _row(Prefs p, Offer o, {required double rowH, int? key, FocusNode? node, String? whose}) {
    final t = Ax.of(context), c = t.colors;
    final on = o.key == of.now.key, star = p.favourites.contains(o.key);
    final mark = markOf(o.provider, o.byDefault ? null : o.model.id, o.harness);
    void toggle() => _favourite(o);
    return CallbackShortcuts(
      key: ValueKey('$whose ${o.key}'),
      bindings: {const SingleActivator(LogicalKeyboardKey.keyF): toggle},
      child: Pressable(
        label: o.name,
        focusNode: node,
        hint: [?whose, if (o.model.context case final n?) '${contextName(n)} context'].join(', '),
        selected: on,
        actions: {
          CustomSemanticsAction(label: star ? 'Remove from favourites' : 'Add to favourites'):
              toggle,
        },
        onPressed: () => _choose(o),
        builder: (context, s) => _Chosen(
          on: on,
          hover: s.hovered || s.focused,
          hue: mark?.color(c),
          child: ConstrainedBox(
            constraints: BoxConstraints(minHeight: rowH),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: AxSpace.row),
              child: Row(
                children: [
                  SizedBox(
                    width: MediaQuery.textScalerOf(context).scale(16),
                    child: mark == null ? null : AxServiceMark(mark, size: 15),
                  ),
                  const SizedBox(width: 10),
                  // The name and, beside it, the check: the rest at the right edge, on one line.
                  Expanded(
                    child: Row(
                      children: [
                        Flexible(
                          flex: 3,
                          child: Text(
                            oneLine(o.name, 80),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: on ? t.type.uiMedium : t.type.ui,
                          ),
                        ),
                        // Right after its name, in the hue of whose it is; then what is said of it.
                        if (on) ...[
                          const SizedBox(width: 8),
                          AxGlyph(AxIcon.check, color: mark?.color(c) ?? c.ink1),
                        ],
                        if (whose != null || o.model.context != null) ...[
                          const SizedBox(width: 10),
                          Flexible(
                            child: Text(
                              [
                                ?whose?.let((w) => oneLine(w, 24)),
                                ?o.model.context?.let(contextName),
                              ].join('  '),
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: t.type.small.ink(c.ink3),
                            ),
                          ),
                        ],
                      ],
                    ),
                  ),
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
                  SizedBox(
                    width: MediaQuery.textScalerOf(context).scale(Command.isMac ? 26 : 38),
                    child: key == null
                        ? null
                        : ExcludeSemantics(
                            child: Text(
                              Command.isMac ? '⌘$key' : 'Ctrl $key',
                              textAlign: TextAlign.right,
                              maxLines: 1,
                              style: t.type.small.ink(c.ink3),
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

extension<T> on T {
  R let<R>(R Function(T) f) => f(this);
}

/// One whose models can be looked at.
class _Stop {
  const _Stop(this.id, this.label, {this.mark, this.icon, this.letters, this.apart = false});
  final String id, label;
  final bool apart;
  final ServiceMark? mark;
  final AxIcon? icon;
  final String? letters;
}

/// Accounts and programs, one under the other.
class _Rail extends StatelessWidget {
  const _Rail(this.all, {required this.at, required this.onTurn});
  static const stop = 34.0, apart = 12.0;
  final List<_Stop> all;
  final String? at;
  final ValueChanged<String> onTurn;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return ListView(
      padding: EdgeInsets.zero,
      children: [
        for (final s in all)
          Padding(
            padding: EdgeInsets.only(top: s.apart ? apart : 0, bottom: 2),
            child: AxHint(
              s.label,
              child: Pressable(
                label: s.label,
                selected: s.id == at,
                onPressed: () => onTurn(s.id),
                builder: (context, p) => _Chosen(
                  on: s.id == at,
                  hover: p.hovered || p.focused,
                  hue: s.mark?.color(c),
                  // On what floats, concentric with it, and raised as a key is: a slab is not seen
                  // on it.
                  radius: _ModelPickerState._inner,
                  raised: c.key,
                  child: ConstrainedBox(
                    constraints: BoxConstraints(
                      minHeight: MediaQuery.textScalerOf(context).scale(stop),
                    ),
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      mainAxisAlignment: MainAxisAlignment.center,
                      children: [
                        if (s.icon case final i?)
                          AxGlyph(i, filled: s.id == at, color: s.id == at ? c.ink1 : c.ink2)
                        else if (s.mark case final m?)
                          AxServiceMark(m, size: s.letters == null ? 18 : 16),
                        if (s.letters case final l?)
                          Text(
                            l,
                            textScaler: TextScaler.noScaling,
                            style: (s.mark == null ? t.type.labelStrong : t.type.small)
                                .copyWith(fontSize: s.mark == null ? 12 : 10, height: 1.2)
                                .ink(c.ink2),
                          ),
                      ],
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

/// What is wider than its room: it scrolls sideways, and goes out of sight gradually where it is cut.
class _Sideways extends StatefulWidget {
  const _Sideways({required this.child});
  final Widget child;
  @override
  State<_Sideways> createState() => _SidewaysState();
}

class _SidewaysState extends State<_Sideways> {
  final _at = ScrollController();
  (bool, bool) _cut = (false, false);

  void _look() {
    final p = _at.hasClients ? _at.position : null;
    final cut = (p != null && p.extentBefore > 0, p != null && p.extentAfter > 0);
    if (cut != _cut && mounted) setState(() => _cut = cut);
  }

  @override
  void initState() {
    super.initState();
    _at.addListener(_look);
    WidgetsBinding.instance.addPostFrameCallback((_) => _look());
  }

  @override
  void dispose() {
    _at.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final ground = Ax.of(context).colors.overlay.fill;
    Widget fade(bool left) => IgnorePointer(
      child: DecoratedBox(
        decoration: BoxDecoration(
          gradient: LinearGradient(
            begin: left ? Alignment.centerLeft : Alignment.centerRight,
            end: left ? Alignment.centerRight : Alignment.centerLeft,
            colors: [ground, ground.withValues(alpha: 0)],
          ),
        ),
        child: const SizedBox(width: 24),
      ),
    );
    return NotificationListener<ScrollMetricsNotification>(
      onNotification: (_) {
        _look();
        return false;
      },
      child: Stack(
        children: [
          SingleChildScrollView(
            controller: _at,
            scrollDirection: Axis.horizontal,
            child: widget.child,
          ),
          if (_cut.$1) Positioned(left: 0, top: 0, bottom: 0, child: fade(true)),
          if (_cut.$2) Positioned(right: 0, top: 0, bottom: 0, child: fade(false)),
        ],
      ),
    );
  }
}

/// A group of rows, recessed: a tone under what floats, and nothing else.
class _Group extends StatelessWidget {
  const _Group({required this.child});
  final Widget child;
  @override
  Widget build(BuildContext context) => Surface(
    Ax.of(context).colors.field,
    radius: _ModelPickerState._inner,
    child: Padding(padding: const EdgeInsets.all(_ModelPickerState._pad), child: child),
  );
}

/// What is chosen is raised, as a thumb is in its trough; what is under the hand has a ground.
class _Chosen extends StatelessWidget {
  const _Chosen({
    required this.on,
    required this.hover,
    required this.child,
    this.hue,
    this.raised,
    this.radius = _ModelPickerState._inner - _ModelPickerState._pad,
  });
  final bool on, hover;
  final Widget child;

  /// What is chosen is raised as: a slab in a group.
  final SurfaceSpec? raised;

  /// Of the service it belongs to: its hover takes it.
  final Color? hue;
  final double radius;
  @override
  Widget build(BuildContext context) {
    final c = Ax.of(context).colors;
    return on
        ? Surface(raised ?? c.slab, radius: radius, child: child)
        : Ground(hover ? c.tinted(hue) : null, radius: radius, child: child);
  }
}

/// What a group holds, on a line as tall in every group.
class _Heading extends StatelessWidget {
  const _Heading(this.name, this.note, {required this.height});
  final String name, note;
  final double height;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Semantics(
      header: true,
      child: Container(
        height: height,
        alignment: Alignment.centerLeft,
        padding: const EdgeInsets.symmetric(horizontal: AxSpace.row),
        // The note whole or not at all: never cut down to a letter.
        child: LayoutBuilder(
          builder: (context, box) {
            TextSpan said(bool noted) => TextSpan(
              text: oneLine(name, 60),
              style: t.type.smallStrong.ink(c.ink2),
              children: [
                if (noted && note.isNotEmpty)
                  TextSpan(text: '   ${oneLine(note, 60)}', style: t.type.small.ink(c.ink3)),
              ],
            );
            final whole = TextPainter(
              text: said(true),
              textDirection: Directionality.of(context),
              textScaler: MediaQuery.textScalerOf(context),
              maxLines: 1,
            )..layout();
            final fits = whole.width <= box.maxWidth;
            whole.dispose();
            return Text.rich(said(fits), maxLines: 1, overflow: TextOverflow.ellipsis);
          },
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
          padding: const EdgeInsets.only(top: _ModelPickerState._gap),
          // Asked: raised above what floats, its lamp on the line the names above begin on.
          child: Surface(
            c.slab,
            radius: _ModelPickerState._inner,
            child: Padding(
              padding: const EdgeInsets.fromLTRB(AxSpace.row, 12, 12, 12),
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
                      AxButton(label: q.keep, focusNode: _keep, onPressed: widget.onKeep),
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
