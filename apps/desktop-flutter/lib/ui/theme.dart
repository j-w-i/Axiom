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

  @override
  bool operator ==(Object other) =>
      other is AxiomThemeData && other.colors == colors && other.motionMode == motionMode;

  @override
  int get hashCode => Object.hash(colors, motionMode);
}

/// Puts [data] in scope, with the body text style as the default.
class AxiomTheme extends StatelessWidget {
  const AxiomTheme({super.key, required this.data, required this.child});
  final AxiomThemeData data;
  final Widget child;

  static AxiomThemeData of(BuildContext context) {
    final scope = context.dependOnInheritedWidgetOfExactType<_AxiomThemeScope>();
    assert(scope != null, 'No AxiomTheme above this widget');
    return scope!.data;
  }

  @override
  Widget build(BuildContext context) {
    return _AxiomThemeScope(
      data: data,
      child: DefaultTextStyle(
        style: data.type.body,
        child: IconTheme(
          data: IconThemeData(color: data.colors.ink2, size: 16),
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
}
