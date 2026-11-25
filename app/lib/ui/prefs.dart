import 'package:flutter/widgets.dart';

import 'tokens.dart';

enum Look { system, light, dark }

/// What a person chose about the window.
@immutable
class Prefs {
  const Prefs({
    this.look = Look.system,
    this.text = 1,
    this.density = AxDensity.regular,
    this.enterSends = true,
  });

  /// Whatever is not known is as it comes: an older or a newer Axiom may have written it.
  factory Prefs.decode(Object? j) {
    if (j is! Map) return const Prefs();
    T of<T extends Enum>(List<T> all, Object? v, T or) =>
        all.where((e) => e.name == v).firstOrNull ?? or;
    final text = j['text'];
    return Prefs(
      look: of(Look.values, j['look'], Look.system),
      text: text is num && sizes.contains(text.toDouble()) ? text.toDouble() : 1,
      density: of(AxDensity.values, j['density'], AxDensity.regular),
      enterSends: j['enterSends'] is bool ? j['enterSends'] as bool : true,
    );
  }

  final Look look;

  /// How large text is, beside what the system says.
  final double text;
  static const sizes = [.85, 1.0, 1.15, 1.3, 1.5, 1.75, 2.0];
  final AxDensity density;

  /// Enter sends and Shift+Enter breaks the line; or Enter breaks it, and with Control or Command it
  /// sends.
  final bool enterSends;

  Prefs copyWith({Look? look, double? text, AxDensity? density, bool? enterSends}) => Prefs(
    look: look ?? this.look,
    text: text ?? this.text,
    density: density ?? this.density,
    enterSends: enterSends ?? this.enterSends,
  );

  /// The next size up or down, as far as there is one.
  Prefs sized(int by) {
    final at = sizes.indexOf(text) + by;
    return copyWith(text: sizes[at.clamp(0, sizes.length - 1)]);
  }

  Map<String, Object> encode() => {
    'look': look.name,
    'text': text,
    'density': density.name,
    'enterSends': enterSends,
  };

  @override
  bool operator ==(Object other) =>
      other is Prefs &&
      other.look == look &&
      other.text == text &&
      other.density == density &&
      other.enterSends == enterSends;
  @override
  int get hashCode => Object.hash(look, text, density, enterSends);
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
