import 'package:flutter/widgets.dart';

/// The whole of what it holds, larger or smaller: laid out in as much less or more room, and the paint
/// scaled, so that letters and lines are drawn for the size they are seen at.
class Zoom extends StatelessWidget {
  const Zoom(this.scale, {required this.child, super.key});
  final double scale;
  final Widget child;

  /// How much larger than it is laid out what is here is seen.
  static double of(BuildContext c) => c.dependOnInheritedWidgetOfExactType<_Zoomed>()?.scale ?? 1;

  @override
  Widget build(BuildContext context) {
    final m = MediaQuery.of(context), by = 1 / scale;
    final size = m.size * by;
    return MediaQuery(
      data: m.copyWith(
        size: size,
        devicePixelRatio: m.devicePixelRatio * scale,
        padding: m.padding * by,
        viewPadding: m.viewPadding * by,
        viewInsets: m.viewInsets * by,
        systemGestureInsets: m.systemGestureInsets * by,
      ),
      child: _Zoomed(
        scale,
        // The same widgets at every scale: nothing in it is made anew when it changes.
        child: FittedBox(
          fit: BoxFit.fill,
          alignment: Alignment.topLeft,
          child: SizedBox.fromSize(size: size, child: child),
        ),
      ),
    );
  }
}

class _Zoomed extends InheritedWidget {
  const _Zoomed(this.scale, {required super.child});
  final double scale;
  @override
  bool updateShouldNotify(_Zoomed old) => old.scale != scale;
}
