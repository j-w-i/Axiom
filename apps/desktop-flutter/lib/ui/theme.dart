import 'package:flutter/widgets.dart';

import 'tokens.dart';

/// The design system in scope: colours, type and motion for one theme and motion setting.
@immutable
final class AxiomThemeData {
  AxiomThemeData({required this.colors, this.motionMode = MotionMode.standard})
    : type = AxiomType(colors),
      motion = AxiomMotion(motionMode);

  final AxiomColors colors;
  final MotionMode motionMode;
  final AxiomType type;
  final AxiomMotion motion;

  /// This theme with another motion mode.
  AxiomThemeData withMotion(MotionMode mode) =>
      mode == motionMode ? this : AxiomThemeData(colors: colors, motionMode: mode);

  @override
  bool operator ==(Object other) =>
      other is AxiomThemeData && other.colors == colors && other.motionMode == motionMode;

  @override
  int get hashCode => Object.hash(colors, motionMode);
}

/// The motion mode in effect: the person's setting, raised to at least Reduced when the system
/// asks for reduced motion (`MediaQuery.disableAnimations`: GNOME's and KDE's "reduce
/// animations", macOS's "Reduce motion"; motion-system.md §2: the OS preference forces at least
/// `reduced`).
MotionMode effectiveMotion(MotionMode setting, {required bool systemReduces}) =>
    systemReduces && setting == MotionMode.standard ? MotionMode.reduced : setting;

/// Puts [data] in scope, with the body text style as the default. The motion follows the
/// system's reduced-motion preference on top of [data]'s mode ([effectiveMotion]).
class AxiomTheme extends StatelessWidget {
  const AxiomTheme({super.key, required this.data, required this.child});
  final AxiomThemeData data;
  final Widget child;

  static AxiomThemeData of(BuildContext context) {
    final scope = context.dependOnInheritedWidgetOfExactType<_AxiomThemeScope>();
    assert(scope != null, 'No AxiomTheme above this widget');
    return scope!.data;
  }

  /// The theme in scope, or null outside every [AxiomTheme].
  static AxiomThemeData? maybeOf(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<_AxiomThemeScope>()?.data;

  @override
  Widget build(BuildContext context) {
    final reduces = MediaQuery.maybeDisableAnimationsOf(context) ?? false;
    final effective = data.withMotion(effectiveMotion(data.motionMode, systemReduces: reduces));
    return _AxiomThemeScope(
      data: effective,
      child: DefaultTextStyle(
        style: effective.type.body,
        child: IconTheme(
          data: IconThemeData(color: effective.colors.ink2, size: 16),
          child: child,
        ),
      ),
    );
  }
}

class _AxiomThemeScope extends InheritedWidget {
  const _AxiomThemeScope({required this.data, required super.child});
  final AxiomThemeData data;

  @override
  bool updateShouldNotify(_AxiomThemeScope old) => old.data != data;
}

extension AxiomContext on BuildContext {
  AxiomThemeData get axiom => AxiomTheme.of(this);
  AxiomColors get colors => AxiomTheme.of(this).colors;

  /// [base] (a row's or control's height at text scale 1) grown by what the system's text scale
  /// adds to one line of [fontSize] px text with a [lineHeight] px line, so scaled text is never
  /// clipped (AUD-020). Never smaller than [base].
  double scaled(double base, {double fontSize = 13, double lineHeight = 18}) => scaledExtent(
    MediaQuery.maybeTextScalerOf(this) ?? TextScaler.noScaling,
    base,
    fontSize: fontSize,
    lineHeight: lineHeight,
  );
}

/// [base] grown by what [scaler] adds to one line of text ([AxiomContext.scaled]).
double scaledExtent(
  TextScaler scaler,
  double base, {
  double fontSize = 13,
  double lineHeight = 18,
}) {
  final grow = lineHeight * (scaler.scale(fontSize) / fontSize - 1);
  return grow > 0 ? (base + grow).ceilToDouble() : base;
}

/// Whether the window is in front and focused: looping indicators run only then
/// (motion-system.md §5.5, performance.md P9). Fed by the app's lifecycle; a window that never
/// reported a state counts as active.
final class WindowActivity {
  WindowActivity._() {
    _listener = AppLifecycleListener(onStateChange: _onState);
    _onState(WidgetsBinding.instance.lifecycleState);
  }
  static WindowActivity? _instance;
  static WindowActivity get instance => _instance ??= WindowActivity._();

  late final AppLifecycleListener _listener;

  /// True while the window is resumed (shown and focused).
  final active = ValueNotifier<bool>(true);

  void _onState(AppLifecycleState? s) => active.value = s == null || s == AppLifecycleState.resumed;

  /// For tests: report a lifecycle state.
  @visibleForTesting
  void debugSet(AppLifecycleState s) => _onState(s);

  /// Stops listening (tests).
  @visibleForTesting
  void dispose() {
    _listener.dispose();
    _instance = null;
  }
}
