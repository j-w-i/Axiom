import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

import 'tokens.dart';

/// Paints [SurfaceSpec]s. Shaders are cached until the size or the spec changes, so a surface whose
/// contents repaint costs three rounded rectangles.
class Surface extends SingleChildRenderObjectWidget {
  const Surface(this.spec, {super.key, this.radius = AxRadius.plate, super.child});
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
    _drop();
  }

  double _radius;
  set radius(double v) {
    if (v == _radius) return;
    _radius = v;
    _drop();
  }

  ui.Shader? _fill, _inset, _edge;
  Size? _for;
  void _drop() {
    _for = null;
    markNeedsPaint();
  }

  void _build(Size s) {
    if (_for == s) return;
    _for = s;
    const o = Offset.zero;
    _fill = ui.Gradient.linear(o, Offset(0, s.height), [_spec.top, _spec.bottom]);
    _inset = ui.Gradient.linear(o, const Offset(0, 5), [_spec.inset, _spec.inset.withAlpha(0)]);
    _edge = ui.Gradient.linear(
      o,
      Offset(0, s.height),
      [_spec.edge, _spec.edge.withValues(alpha: _spec.edge.a * .2), _spec.edge.withAlpha(0)],
      [0, math.min(.5, 9 / math.max(s.height, 1)), 1],
    );
  }

  @override
  void paint(PaintingContext context, Offset offset) {
    _build(size);
    final c = context.canvas..save();
    c.translate(offset.dx, offset.dy);
    final rr = RRect.fromRectAndRadius(Offset.zero & size, Radius.circular(_radius));
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
    if (_spec.lip.a > 0) c.drawRRect(rr.shift(const Offset(0, 1)), Paint()..color = _spec.lip);
    c.drawRRect(rr, Paint()..shader = _fill);
    if (_spec.inset.a > 0) {
      c.save();
      c.clipRRect(rr);
      c.drawRect(
        Rect.fromLTWH(0, 0, size.width, math.min(5, size.height)),
        Paint()..shader = _inset,
      );
      c.restore();
    }
    if (_spec.edge.a > 0) {
      c.drawRRect(
        rr.deflate(_spec.border.a > 0 ? 1.5 : .5),
        Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1
          ..shader = _edge,
      );
    }
    if (_spec.border.a > 0) {
      c.drawRRect(
        rr.deflate(.5),
        Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1
          ..color = _spec.border,
      );
    }
    c.restore();
    super.paint(context, offset);
  }
}

/// A seam between two parts of the chassis: a dark line and the light its far edge catches.
class Seam extends StatelessWidget {
  const Seam({super.key, this.vertical = false});
  final bool vertical;
  @override
  Widget build(BuildContext context) {
    final c = Ax.of(context).colors;
    Widget line(Color k) => ColoredBox(
      color: k,
      child: SizedBox(
        width: vertical ? 1 : double.infinity,
        height: vertical ? double.infinity : 1,
      ),
    );
    return vertical
        ? Row(mainAxisSize: MainAxisSize.min, children: [line(c.rule), line(c.ruleLight)])
        : Column(mainAxisSize: MainAxisSize.min, children: [line(c.rule), line(c.ruleLight)]);
  }
}
