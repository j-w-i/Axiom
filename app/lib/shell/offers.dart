import '../host/host_store.dart';
import '../host/proto.dart';
import '../patterns/execution.dart';
import '../patterns/model_name.dart';
import '../transcript/safe_text.dart';
import '../ui/overlay.dart';
import '../ui/prefs.dart';
import 'start.dart';

/// A model that can be chosen: of which account or program.
class Offer {
  const Offer(this.harness, this.connection, this.provider, this.model, this.of, this.at);
  final String harness;

  /// Of Axiom's own, the account; of a program, another than its first.
  final String? connection, provider;
  final ModelInfo model;

  /// Whose it is, for people.
  final String of;

  /// When it was said to be there; 0 if it is from a program's documentation, null if just now.
  final int? at;

  /// What it is kept by, in favourites and the like.
  String get key => offerKey(harness, connection, model.id);
}

String offerKey(String harness, String? connection, String model) =>
    '$harness\n${connection ?? ''}\n$model';

/// What is known of all accounts and programs that are there, or of one [harness] only. Nobody is
/// asked.
List<Offer> offersOf(HostStore s, {String? harness}) {
  ModelList? kept(String h, String? c) =>
      s.lists.where((l) => l.harness == h && l.connection == c).firstOrNull;
  return [
    if (harness == null || harness == 'axiom')
      for (final c in s.own)
        for (final m in s.modelsOf(c.id) ?? kept('axiom', c.id)?.models ?? const <ModelInfo>[])
          Offer(
            'axiom',
            c.id,
            c.provider,
            m,
            c.name,
            s.modelsOf(c.id) == null ? kept('axiom', c.id)?.at : null,
          ),
    for (final h in s.harnesses)
      if (h.program != null && (harness == null || harness == h.id)) ...[
        if (kept(h.id, null) case final l?)
          for (final m in l.models) Offer(h.id, null, null, m, h.name, l.at),
        for (final c in s.accountsOf(h.id))
          if (kept(h.id, c.id) case final l?)
            for (final m in l.models) Offer(h.id, c.id, null, m, '${h.name}, ${c.name}', l.at),
      ],
  ];
}

/// As they are offered: favourites, then what was used last, then the rest, without what is hidden.
/// [chosen] is there in any case.
List<Offer> arranged(List<Offer> all, Prefs p, {String? chosen}) {
  final fit = [
    for (final o in all)
      if (o.key == chosen || !p.hidden.contains(o.key)) o,
  ];
  int rank(Offer o) {
    final used = p.used.indexOf(o.key);
    return p.favourites.contains(o.key) ? -1 : (used < 0 ? p.used.length : used);
  }

  final at = {for (final (i, o) in fit.indexed) o.key: i};
  return fit..sort((a, b) {
    final by = rank(a).compareTo(rank(b));
    return by != 0 ? by : at[a.key]!.compareTo(at[b.key]!);
  });
}

/// How large a window is, as people say it.
String contextName(int tokens) => tokens >= 1000000 && tokens % 100000 < 50000
    ? '${(tokens / 1000000).toStringAsFixed(tokens % 1000000 < 50000 ? 0 : 1)}M context'
    : '${(tokens / 1000).round()}K context';

/// What each is called in the list. Where there are several accounts or programs, whose it is.
List<AxMenuItem<String>> offerItems(List<Offer> all, Prefs p) {
  final whose = {for (final o in all) (o.harness, o.connection)}.length > 1;
  final names = <(String, String?), Map<String, (String, String?)>>{};
  for (final o in all) {
    names[(o.harness, o.connection)] ??= modelNames([
      for (final x in all)
        if (x.harness == o.harness && x.connection == o.connection) (x.model.id, x.model.name),
    ]);
  }
  return [
    for (final o in all)
      () {
        final (name, id) = names[(o.harness, o.connection)]![o.model.id]!;
        return AxMenuItem(
          o.key,
          oneLine(name, 80),
          detail: [
            if (p.favourites.contains(o.key)) 'favourite',
            if (whose) oneLine(o.of, 40),
            if (id != null) oneLine(id, 80),
            if (o.model.context case final n?) contextName(n),
          ].join(' · ').nonEmpty,
          mark: markOf(o.provider, o.model.id, o.harness),
        );
      }(),
  ];
}

extension on String {
  String? get nonEmpty => isEmpty ? null : this;
}

/// What is known of the model that is chosen, if it is in a list or has been asked about.
ModelInfo? infoOf(HostStore s, String harness, String? connection, String model) =>
    offersOf(
      s,
      harness: harness,
    ).where((o) => o.connection == connection && o.model.id == model).firstOrNull?.model ??
    (connection == null ? null : s.aboutOf(connection, model));

/// How hard a model is to try, as its provider calls it.
String effortName(String e) => switch (e) {
  '' => 'Default effort',
  'none' => 'No reasoning',
  'minimal' => 'Minimal effort',
  'low' => 'Low effort',
  'medium' => 'Medium effort',
  'high' => 'High effort',
  'xhigh' => 'Extra high effort',
  'max' => 'Maximum effort',
  _ => oneLine(e, 30),
};

/// What a model can be told: how hard to try, whether to think, how fast to be. Only what is offered.
List<ExecutionChoice> tellable(
  ModelInfo? m, {
  required List<String> efforts,
  required String effort,
  required bool? thinking,
  required String speed,
  required bool program,
  required void Function(String effort) onEffort,
  required void Function(bool? thinking) onThinking,
  required void Function(String speed) onSpeed,
}) => [
  if (efforts.isNotEmpty || effort.isNotEmpty)
    ExecutionChoice(
      'Effort',
      [
        for (final e in {'', ...efforts, effort}) AxMenuItem(e, effortName(e)),
      ],
      effort,
      onEffort,
    ),
  if ((m?.thinking ?? false) || thinking != null)
    ExecutionChoice(
      'Thinking',
      const [
        AxMenuItem('', 'As the model does'),
        AxMenuItem('on', 'Thinks'),
        AxMenuItem('off', 'Does not think'),
      ],
      switch (thinking) {
        null => '',
        true => 'on',
        false => 'off',
      },
      (v) => onThinking(v.isEmpty ? null : v == 'on'),
    ),
  if ((m?.speeds.isNotEmpty ?? false) || speed.isNotEmpty)
    ExecutionChoice(
      'Speed',
      [
        const AxMenuItem('', 'Standard speed'),
        for (final v in {...?m?.speeds, if (speed.isNotEmpty) speed})
          AxMenuItem(v, v == 'fast' ? 'Fast' : oneLine(v, 30), detail: 'costs more'),
      ],
      speed,
      onSpeed,
      cost: (v) => v.isEmpty
          ? null
          : program
          ? 'Fast answers cost more for each token. On a plan they are paid for by usage credits, and '
                'not by the plan.'
          : 'Fast answers cost more for each token. What was cached at one speed is not at the other.',
    ),
];
