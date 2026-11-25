import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

import 'tokens.dart';

/// Paints a [SurfaceSpec]: a fill, a hairline, and a shadow if it floats.
class Surface extends SingleChildRenderObjectWidget {
  const Surface(this.spec, {super.key, this.radius = AxRadius.key, super.child});
  final SurfaceSpec spec;
  final double radius;

  @override
  RenderObject createRenderObject(BuildContext context) => RenderSurface(spec, radius);
  @override
  void updateRenderObject(BuildContext context, RenderSurface renderObject) => renderObject
    ..spec = spec
    ..radius = radius;
}

class RenderSurface extends RenderProxyBox {
  RenderSurface(this._spec, this._radius);
  SurfaceSpec _spec;
  set spec(SurfaceSpec v) {
    if (identical(v, _spec)) return;
    _spec = v;
    markNeedsPaint();
  }

  double _radius;
  set radius(double v) {
    if (v == _radius) return;
    _radius = v;
    markNeedsPaint();
  }

  @override
  void paint(PaintingContext context, Offset offset) {
    final c = context.canvas;
    final rr = RRect.fromRectAndRadius(offset & size, Radius.circular(_radius));
    for (final s in _spec.shadows) {
      c.drawRRect(
        rr.shift(s.offset).inflate(s.spreadRadius),
        Paint()
          ..color = s.color
          ..maskFilter = debugDisableShadows
              ? null
              : MaskFilter.blur(BlurStyle.normal, s.blurSigma),
      );
    }
    c.drawRRect(rr, Paint()..color = _spec.fill);
    if (_spec.border.a > 0) {
      c.drawRRect(
        rr.deflate(.5),
        Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1
          ..color = _spec.border,
      );
    }
    super.paint(context, offset);
  }
}

/// A hairline between two parts.
class Hairline extends StatelessWidget {
  const Hairline({super.key, this.vertical = false});
  final bool vertical;
  @override
  Widget build(BuildContext context) => ColoredBox(
    color: Ax.of(context).colors.hairline,
    child: SizedBox(width: vertical ? 1 : double.infinity, height: vertical ? double.infinity : 1),
  );
}
