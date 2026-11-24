import 'package:flutter/physics.dart';
import 'package:flutter/widgets.dart';

import 'press.dart';
import 'tokens.dart';

/// Follows [target] with a spring. Retargeting keeps the velocity, so an interrupted movement turns
/// instead of restarting. Reduced motion jumps.
class SpringTo extends StatefulWidget {
  const SpringTo({required this.target, required this.builder, super.key, this.spring, this.child});
  final double target;
  final SpringDescription? spring;
  final Widget? child;
  final Widget Function(BuildContext, double, Widget?) builder;
  @override
  State<SpringTo> createState() => _SpringToState();
}

class _SpringToState extends State<SpringTo> with SingleTickerProviderStateMixin {
  late final _c = AnimationController.unbounded(vsync: this, value: widget.target);
  @override
  void didUpdateWidget(SpringTo old) {
    super.didUpdateWidget(old);
    if (old.target == widget.target) return;
    if (reducedMotion(context)) {
      _c.value = widget.target;
      return;
    }
    _c.animateWith(
      SpringSimulation(widget.spring ?? AxMotion.glide, _c.value, widget.target, _c.velocity)
        ..tolerance = const Tolerance(distance: .002, velocity: .01),
    );
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: _c,
    builder: (b, child) => widget.builder(b, _c.value, child),
    child: widget.child,
  );
}
