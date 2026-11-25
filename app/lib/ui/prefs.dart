import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';

import 'tokens.dart';

enum Look { system, light, dark }

/// What a person chose about the window.
@immutable
class Prefs {
  const Prefs({
    this.look = Look.system,
    this.text = 1,
    this.zoom = 1,
    this.density = AxDensity.regular,
    this.code = '',
    this.enterSends = true,
    this.favourites = const [],
    this.hidden = const [],
    this.used = const [],
  });

  /// Whatever is not known is as it comes: an older or a newer Axiom may have written it.
  factory Prefs.decode(Object? j) {
    if (j is! Map) return const Prefs();
    T of<T extends Enum>(List<T> all, Object? v, T or) =>
        all.where((e) => e.name == v).firstOrNull ?? or;
    double at(List<double> all, Object? v) =>
        v is num && all.contains(v.toDouble()) ? v.toDouble() : 1;
    List<String> names(Object? v, int most) => v is! List
        ? const []
        : [
            for (final s in v.take(most))
              if (s is String && s.isNotEmpty && s.length <= 700) s,
          ];
    return Prefs(
      look: of(Look.values, j['look'], Look.system),
      text: at(sizes, j['text']),
      zoom: at(zooms, j['zoom']),
      density: of(AxDensity.values, j['density'], AxDensity.regular),
      code: j['code'] is String ? family(j['code'] as String) : '',
      enterSends: j['enterSends'] is bool ? j['enterSends'] as bool : true,
      favourites: names(j['favourites'], most),
      hidden: names(j['hidden'], most),
      used: names(j['used'], lately),
    );
  }

  final Look look;

  /// How large text is, beside what the system says.
  final double text;
  static const sizes = [.85, 1.0, 1.15, 1.3, 1.5, 1.75, 2.0];

  /// How large everything is.
  final double zoom;
  static const zooms = [.8, .9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];
  final AxDensity density;

  /// The family of code, diffs, paths and commands; empty: the one that is shipped.
  final String code;

  /// What can be a family's name.
  static String family(String s) {
    final name = s.trim();
    return name.length > 100 || name.contains(RegExp(r'[\x00-\x1f\x7f]')) ? '' : name;
  }

  /// Enter sends and Shift+Enter breaks the line; or Enter breaks it, and with Control or Command it
  /// sends.
  final bool enterSends;

  /// Models, by the names they are kept by: those that come first, those that are not offered, and
  /// those that were chosen last.
  final List<String> favourites, hidden, used;
  static const most = 200, lately = 12;

  /// One has been chosen.
  Prefs using(String model) =>
      copyWith(used: [model, ...used.where((m) => m != model)].take(lately).toList());

  /// How one is offered from now on.
  Prefs offering(String model, {bool favourite = false, bool hide = false}) {
    List<String> with_(List<String> all, bool has) =>
        [if (has) model, ...all.where((m) => m != model)].take(most).toList();
    return copyWith(favourites: with_(favourites, favourite), hidden: with_(hidden, hide));
  }

  Prefs copyWith({
    Look? look,
    double? text,
    double? zoom,
    AxDensity? density,
    String? code,
    bool? enterSends,
    List<String>? favourites,
    List<String>? hidden,
    List<String>? used,
  }) => Prefs(
    look: look ?? this.look,
    text: text ?? this.text,
    zoom: zoom ?? this.zoom,
    density: density ?? this.density,
    code: code ?? this.code,
    enterSends: enterSends ?? this.enterSends,
    favourites: favourites ?? this.favourites,
    hidden: hidden ?? this.hidden,
    used: used ?? this.used,
  );

  /// The next one up or down, as far as there is one.
  Prefs sized(int by) => copyWith(text: _by(sizes, text, by));
  Prefs zoomed(int by) => copyWith(zoom: _by(zooms, zoom, by));
  static double _by(List<double> all, double v, int by) =>
      all[(all.indexOf(v) + by).clamp(0, all.length - 1)];

  Map<String, Object> encode() => {
    'look': look.name,
    'text': text,
    'zoom': zoom,
    'density': density.name,
    'code': code,
    'enterSends': enterSends,
    'favourites': favourites,
    'hidden': hidden,
    'used': used,
  };

  @override
  bool operator ==(Object other) =>
      other is Prefs &&
      other.look == look &&
      other.text == text &&
      other.zoom == zoom &&
      other.density == density &&
      other.code == code &&
      other.enterSends == enterSends &&
      listEquals(other.favourites, favourites) &&
      listEquals(other.hidden, hidden) &&
      listEquals(other.used, used);
  @override
  int get hashCode => Object.hash(
    look,
    text,
    zoom,
    density,
    code,
    enterSends,
    Object.hashAll(favourites),
    Object.hashAll(hidden),
    Object.hashAll(used),
  );
}

/// Where they can be changed from anywhere in the window.
class PrefsScope extends InheritedNotifier<ValueNotifier<Prefs>> {
  const PrefsScope(ValueNotifier<Prefs> prefs, {required super.child, super.key})
    : super(notifier: prefs);
  static Prefs of(BuildContext c) =>
      c.dependOnInheritedWidgetOfExactType<PrefsScope>()?.notifier?.value ?? const Prefs();
  static ValueNotifier<Prefs>? maybeOf(BuildContext c) =>
      c.getInheritedWidgetOfExactType<PrefsScope>()?.notifier;
}
