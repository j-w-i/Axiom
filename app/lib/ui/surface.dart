import 'dart:math' as math;

import 'package:flutter/physics.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

import 'tokens.dart';

/// An outline whose corners turn gradually: no visible point where the straight ends and the corner
/// begins. The same on every system.
Path continuous(Rect r, double radius) {
  final half = r.shortestSide / 2;
  if (radius <= 0) return Path()..addRect(r);
  // A round end stays round.
  if (radius >= half) return Path()..addRRect(RRect.fromRectAndRadius(r, Radius.circular(half)));
  final p = math.min(radius * 1.26, half), k = p * .33;
  return Path()
    ..moveTo(r.left + p, r.top)
    ..lineTo(r.right - p, r.top)
    ..cubicTo(r.right - k, r.top, r.right, r.top + k, r.right, r.top + p)
    ..lineTo(r.right, r.bottom - p)
    ..cubicTo(r.right, r.bottom - k, r.right - k, r.bottom, r.right - p, r.bottom)
    ..lineTo(r.left + p, r.bottom)
    ..cubicTo(r.left + k, r.bottom, r.left, r.bottom - k, r.left, r.bottom - p)
    ..lineTo(r.left, r.top + p)
    ..cubicTo(r.left, r.top + k, r.left + k, r.top, r.left + p, r.top)
    ..close();
}

/// The outlines of one shape, made once for a size.
class _Shape {
  _Shape(this.size, this.radius, this.ring)
    : outer = ring > 0 ? continuous(Offset.zero & size, radius + ring) : null,
      face = continuous((Offset.zero & size).deflate(ring), radius);
  final Size size;
  final double radius, ring;
  final Path face;
  final Path? outer;
  late final Path lip = Path.combine(
    PathOperation.difference,
    face,
    face.shift(const Offset(0, 1)),
  );
  late final Path foot = Path.combine(
    PathOperation.difference,
    face.shift(const Offset(0, 1)),
    face,
  );
  late final Path edge = continuous((Offset.zero & size).deflate(ring + .5), radius - .5);
}

/// Paints a [SurfaceSpec]. Raised, from below: a soft shade (light theme), a contact shade, the fill,
/// a lit top edge. Recessed: the fill. Written into: a ring round it.
class Surface extends SingleChildRenderObjectWidget {
  const Surface(this.spec, {super.key, this.radius = AxRadius.key, this.ring, super.child});
  final SurfaceSpec spec;

  /// Of the face. What has a ring is as much rounder outside.
  final double radius;

  /// The colour of the ring, if it is not the spec's: what has the keyboard, what is wrong.
  final Color? ring;

  @override
  RenderObject createRenderObject(BuildContext context) => RenderSurface(spec, radius, ring);
  @override
  void updateRenderObject(BuildContext context, RenderSurface renderObject) => renderObject
    ..spec = spec
    ..ring = ring
    ..radius = radius;
}

class RenderSurface extends RenderProxyBox {
  RenderSurface(this._spec, this._radius, this._ring);
  SurfaceSpec _spec;
  SurfaceSpec get spec => _spec;
  set spec(SurfaceSpec v) {
    if (identical(v, _spec)) return;
    _spec = v;
    markNeedsPaint();
  }

  Color? _ring;
  set ring(Color? v) {
    if (v == _ring) return;
    _ring = v;
    markNeedsPaint();
  }

  double _radius;
  set radius(double v) {
    if (v == _radius) return;
    _radius = v;
    markNeedsPaint();
  }

  _Shape? _made;

  @override
  void paint(PaintingContext context, Offset offset) {
    var m = _made;
    if (m == null || m.size != size || m.radius != _radius || m.ring != _spec.ring) {
      m = _made = _Shape(size, _radius, _spec.ring);
    }
    final c = context.canvas..save();
    c.translate(offset.dx, offset.dy);
    paintSurface(c, m, _spec, _ring);
    c.restore();
    super.paint(context, offset);
  }
}

void paintSurface(Canvas c, Object shape, SurfaceSpec spec, Color? ring) {
  final m = shape as _Shape;
  for (final s in spec.shadows) {
    c.drawPath(
      (m.outer ?? m.face).shift(s.offset),
      Paint()
        ..color = s.color
        ..maskFilter = debugDisableShadows ? null : MaskFilter.blur(BlurStyle.normal, s.blurSigma),
    );
  }
  if (m.outer case final o?) c.drawPath(o, Paint()..color = ring ?? spec.bezel);
  if (spec.shade.a > 0) c.drawPath(m.foot, Paint()..color = spec.shade);
  c.drawPath(m.face, Paint()..color = spec.fill);
  if (spec.border.a > 0) {
    c.drawPath(
      m.edge,
      Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = 1
        ..color = spec.border,
    );
  }
  if (spec.lip.a > 0) c.drawPath(m.lip, Paint()..color = spec.lip);
}

/// A shape to paint with [paintSurface].
Object surfaceShape(Size size, double radius) => _Shape(size, radius, 0);

/// Clips to the outline of a surface.
class ContinuousClip extends CustomClipper<Path> {
  const ContinuousClip(this.radius);
  final double radius;
  @override
  Path getClip(Size size) => continuous(Offset.zero & size, radius);
  @override
  bool shouldReclip(ContinuousClip o) => o.radius != radius;
}

/// A groove between two parts of one tone.
class Hairline extends StatelessWidget {
  const Hairline({super.key, this.vertical = false});
  final bool vertical;
  @override
  Widget build(BuildContext context) => ColoredBox(
    color: Ax.of(context).colors.hairline,
    child: SizedBox(width: vertical ? 1 : double.infinity, height: vertical ? double.infinity : 1),
  );
}

/// A trough with a raised thumb under one of its children. The thumb slides to the one that is
/// chosen; a person's press moves it at once, and at either end it is squeezed rather than leaving.
class Trough extends StatelessWidget {
  const Trough({required this.index, required this.children, super.key});
  final int index;
  final List<Widget> children;
  static const pad = 3.0;
  @override
  Widget build(BuildContext context) {
    final c = Ax.of(context).colors;
    return Surface(
      c.track,
      radius: AxRadius.key + pad,
      child: Padding(
        padding: const EdgeInsets.all(pad),
        child: _Sliding(index: index, slab: c.slab, children: children),
      ),
    );
  }
}

class _Sliding extends StatefulWidget {
  const _Sliding({required this.index, required this.slab, required this.children});
  final int index;
  final SurfaceSpec slab;
  final List<Widget> children;
  @override
  State<_Sliding> createState() => _SlidingState();
}

class _SlidingState extends State<_Sliding> with SingleTickerProviderStateMixin {
  late final _at = AnimationController.unbounded(vsync: this, value: widget.index.toDouble());
  @override
  void didUpdateWidget(_Sliding old) {
    super.didUpdateWidget(old);
    if (old.index == widget.index) return;
    if (MediaQuery.maybeDisableAnimationsOf(context) ?? false) {
      _at.value = widget.index.toDouble();
      return;
    }
    _at.animateWith(
      SpringSimulation(AxMotion.slide, _at.value, widget.index.toDouble(), _at.velocity)
        ..tolerance = const Tolerance(distance: .002, velocity: .01),
    );
  }

  @override
  void dispose() {
    _at.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) =>
      _SlidingRow(at: _at, slab: widget.slab, children: widget.children);
}

class _SlidingRow extends Flex {
  const _SlidingRow({required this.at, required this.slab, required super.children})
    : super(direction: Axis.horizontal, mainAxisSize: MainAxisSize.min);
  final Animation<double> at;
  final SurfaceSpec slab;
  @override
  RenderFlex createRenderObject(BuildContext context) =>
      _RenderSliding(at, slab, textDirection: Directionality.of(context));
  @override
  void updateRenderObject(BuildContext context, covariant _RenderSliding renderObject) {
    super.updateRenderObject(context, renderObject);
    renderObject.slab = slab;
  }
}

class _RenderSliding extends RenderFlex {
  _RenderSliding(this.at, this._slab, {super.textDirection})
    : super(direction: Axis.horizontal, mainAxisSize: MainAxisSize.min);
  final Animation<double> at;
  SurfaceSpec _slab;
  set slab(SurfaceSpec v) {
    if (identical(v, _slab)) return;
    _slab = v;
    markNeedsPaint();
  }

  @override
  void attach(PipelineOwner owner) {
    super.attach(owner);
    at.addListener(markNeedsPaint);
  }

  @override
  void detach() {
    at.removeListener(markNeedsPaint);
    super.detach();
  }

  Object? _shape;
  Size? _for;

  @override
  void paint(PaintingContext context, Offset offset) {
    final all = getChildrenAsList();
    if (all.isNotEmpty) {
      Rect of(int i) {
        final b = all[i.clamp(0, all.length - 1)];
        return (b.parentData! as FlexParentData).offset & b.size;
      }

      final v = at.value, low = v.floor(), x = v - low;
      final full = Rect.lerp(of(low), of(low + 1), x)!;
      // Beyond an end it is squeezed against the wall.
      final past = math.max(-v, v - (all.length - 1)).clamp(0.0, .4);
      final r = past == 0
          ? full
          : v < 0
          ? Rect.fromLTRB(full.left, full.top, full.right - full.width * past * .4, full.bottom)
          : Rect.fromLTRB(full.left + full.width * past * .4, full.top, full.right, full.bottom);
      if (_for != r.size) {
        _for = r.size;
        _shape = surfaceShape(r.size, AxRadius.key);
      }
      final c = context.canvas..save();
      c.translate(offset.dx + r.left, offset.dy + r.top);
      paintSurface(c, _shape!, _slab, null);
      c.restore();
    }
    super.paint(context, offset);
  }
}
