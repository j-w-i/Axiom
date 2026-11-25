import '../host/host_store.dart';
import '../host/proto.dart';
import '../patterns/model_name.dart';
import '../transcript/safe_text.dart';
import '../ui/prefs.dart';
import 'start.dart';

/// A model that can be chosen: of which account or program.
class Offer {
  const Offer(this.harness, this.connection, this.provider, this.model);
  final String harness;

  /// Of Axiom's own, the account; of a program, another than its first.
  final String? connection, provider;

  /// Without a name: whatever a program takes by itself.
  final ModelInfo model;
  bool get byDefault => model.id.isEmpty;

  /// What it is kept by, in favourites and the like.
  String get key => offerKey(harness, connection, model.id);

  String get name => byDefault ? 'Default model' : modelName(model.id, model.name);
}

String offerKey(String harness, String? connection, String model) =>
    '$harness\n${connection ?? ''}\n$model';

/// An account or a program, and the models it has.
class Source {
  const Source(
    this.harness,
    this.connection,
    this.provider,
    this.name,
    this.note,
    this.models,
    this.at,
  );
  final String harness, name, note;
  final String? connection, provider;
  final List<Offer> models;

  /// When they were said to be there; 0 if that is from a program's documentation, null if it was just
  /// now or never.
  final int? at;
}

/// What is known of all accounts and programs that are there, or of one [harness] only. Nobody is
/// asked.
List<Source> sourcesOf(HostStore s, {String? harness}) {
  ModelList? kept(String h, String? c) =>
      s.lists.where((l) => l.harness == h && l.connection == c).firstOrNull;
  Source program(HarnessInfo h, Connection? c) {
    // Another account has the same, until it has said otherwise.
    final l = (c == null ? null : kept(h.id, c.id)) ?? kept(h.id, null);
    return Source(
      h.id,
      c?.id,
      null,
      h.name,
      c != null
          ? c.name
          : h.own
          ? 'your own program'
          : '',
      [
        Offer(h.id, c?.id, null, const ModelInfo('')),
        for (final m in l?.models ?? const <ModelInfo>[]) Offer(h.id, c?.id, null, m),
      ],
      l?.at,
    );
  }

  return [
    if (harness == null || harness == 'axiom')
      for (final c in s.own)
        Source('axiom', c.id, c.provider, c.name, providerName(c.provider), [
          for (final m in s.modelsOf(c.id) ?? kept('axiom', c.id)?.models ?? const <ModelInfo>[])
            Offer('axiom', c.id, c.provider, m),
        ], s.modelsOf(c.id) == null ? kept('axiom', c.id)?.at : null),
    for (final h in s.harnesses)
      if (h.program != null && (harness == null || harness == h.id)) ...[
        program(h, null),
        for (final c in s.accountsOf(h.id)) program(h, c),
      ],
  ];
}

/// As they are offered: what was used last first, without what is hidden. [chosen] is there in any
/// case.
List<Offer> arranged(List<Offer> all, Prefs p, {String? chosen}) {
  final fit = [
    for (final o in all)
      if (o.key == chosen || !p.hidden.contains(o.key)) o,
  ];
  int rank(Offer o) {
    final used = p.used.indexOf(o.key);
    return used < 0 ? p.used.length : used;
  }

  final at = {for (final (i, o) in fit.indexed) o.key: i};
  return fit..sort((a, b) {
    final by = rank(a).compareTo(rank(b));
    return by != 0 ? by : at[a.key]!.compareTo(at[b.key]!);
  });
}

/// How large a window is, as people say it.
String contextName(int tokens) {
  final m = tokens / 1000000;
  return tokens >= 950000
      ? '${m.toStringAsFixed((m - m.round()).abs() < .05 ? 0 : 1)}M'
      : '${(tokens / 1000).round()}K';
}

/// What is known of the model that is chosen, if it is in a list or has been asked about.
ModelInfo? infoOf(HostStore s, String harness, String? connection, String model) => model.isEmpty
    ? null
    : [
            for (final x in sourcesOf(s, harness: harness))
              if (x.connection == connection) ...x.models,
          ].where((o) => o.model.id == model).firstOrNull?.model ??
          (connection == null ? null : s.aboutOf(connection, model));

/// How hard a model is to try, as its provider calls it.
String effortName(String e) => switch (e) {
  '' => 'Default',
  'none' => 'None',
  'minimal' => 'Minimal',
  'low' => 'Low',
  'medium' => 'Medium',
  'high' => 'High',
  'xhigh' => 'Extra high',
  'max' => 'Max',
  _ => oneLine(e, 20),
};
