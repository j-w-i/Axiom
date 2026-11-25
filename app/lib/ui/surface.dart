import 'dart:math' as math;
import 'dart:ui' as ui show FragmentProgram, FragmentShader, Gradient;

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
  _Shape(this.size, this.radius)
    : face = continuous(Offset.zero & size, radius),
      edge = continuous((Offset.zero & size).deflate(.5), radius - .5);
  final Size size;
  final double radius;
  final Path face, edge;
  Color? _lip;
  Shader? _light;

  /// Strongest at the upper edge, gone by the middle.
  Shader light(Color lip) {
    if (_lip == lip) return _light!;
    _lip = lip;
    return _light = ui.Gradient.linear(Offset.zero, Offset(0, math.min(size.height, 40)), [
      lip,
      lip.withValues(alpha: 0),
    ]);
  }
}

/// Paints a [SurfaceSpec]: its soft shades, its fill and the light that falls on it.
class Surface extends SingleChildRenderObjectWidget {
  const Surface(this.spec, {super.key, this.radius = AxRadius.key, this.ring, super.child});
  final SurfaceSpec spec;
  final double radius;

  /// Round what has the keyboard, or what is wrong.
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
    if (m == null || m.size != size || m.radius != _radius) m = _made = _Shape(size, _radius);
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
    _shade(c, m, s);
  }
  c.drawPath(m.face, Paint()..color = spec.fill);
  Paint line() => Paint()
    ..style = PaintingStyle.stroke
    ..strokeWidth = 1;
  if (spec.border.a > 0) c.drawPath(m.edge, line()..color = spec.border);
  if (spec.lip.a > .01) c.drawPath(m.edge, line()..shader = m.light(spec.lip));
  if (ring != null) {
    c.drawPath(
      m.edge,
      line()
        ..strokeWidth = 1.5
        ..color = ring,
    );
  }
}

/// What shades are drawn with. Until it is there, and where it cannot be, they are plain blurs.
abstract final class Shades {
  static ui.FragmentShader? _shader;
  static Future<void> load() async {
    try {
      _shader ??= (await ui.FragmentProgram.fromAsset('shaders/shade.frag')).fragmentShader();
    } on Object catch (e) {
      debugPrint('shades are plain: $e');
    }
  }
}

void _shade(Canvas c, _Shape m, Shade s) {
  final shader = Shades._shader;
  if (shader == null || debugDisableShadows) {
    c.drawPath(
      m.face.shift(Offset(0, s.y)),
      Paint()
        ..color = s.color
        ..maskFilter = debugDisableShadows ? null : MaskFilter.blur(BlurStyle.normal, s.sigma),
    );
    return;
  }
  final box = (Offset.zero & m.size).translate(0, s.y);
  shader
    ..setFloat(0, box.left)
    ..setFloat(1, box.top)
    ..setFloat(2, box.right)
    ..setFloat(3, box.bottom)
    ..setFloat(4, math.min(m.radius, m.size.shortestSide / 2))
    ..setFloat(5, math.max(s.sigma, .5))
    ..setFloat(6, s.color.r)
    ..setFloat(7, s.color.g)
    ..setFloat(8, s.color.b)
    ..setFloat(9, s.color.a)
    ..setFloat(10, 1 / (255 * s.ground));
  // What it is set to is taken when it is drawn, so one is enough.
  // Nor is anything drawn where the face will be.
  final r = math.min(m.radius, m.size.shortestSide / 2);
  c.drawDRRect(
    RRect.fromRectXY(box.inflate(s.sigma * 3 + 1), 0, 0),
    RRect.fromRectXY((Offset.zero & m.size).deflate(math.min(2, m.size.shortestSide / 4)), r, r),
    Paint()..shader = shader,
  );
}

/// A shape to paint with [paintSurface].
Object surfaceShape(Size size, double radius) => _Shape(size, radius);

/// Clips to the outline of a surface.
class ContinuousClip extends CustomClipper<Path> {
  const ContinuousClip(this.radius);
  final double radius;
  @override
  Path getClip(Size size) => continuous(Offset.zero & size, radius);
  @override
  bool shouldReclip(ContinuousClip o) => o.radius != radius;
}

/// A hairline between rows of one kind. Parts of the window are told apart by tone and room.
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

/// A ground that comes and goes under the hand.
class Ground extends StatelessWidget {
  const Ground(this.color, {required this.child, super.key, this.radius = AxRadius.key});
  final Color? color;
  final double radius;
  final Widget child;
  @override
  Widget build(BuildContext context) => AnimatedContainer(
    duration: (MediaQuery.maybeDisableAnimationsOf(context) ?? false)
        ? Duration.zero
        : AxMotion.fade,
    decoration: BoxDecoration(
      color: color ?? Ax.of(context).colors.hover.withValues(alpha: 0),
      borderRadius: BorderRadius.circular(radius),
    ),
    child: child,
  );
}

/// What scrolls goes out of sight gradually.
class Fade extends StatelessWidget {
  const Fade({required this.up, super.key, this.height = 28});

  /// At the upper edge.
  final bool up;
  final double height;
  @override
  Widget build(BuildContext context) {
    final c = Ax.of(context).colors.bay.fill;
    return IgnorePointer(
      child: DecoratedBox(
        decoration: BoxDecoration(
          gradient: LinearGradient(
            begin: up ? Alignment.topCenter : Alignment.bottomCenter,
            end: up ? Alignment.bottomCenter : Alignment.topCenter,
            colors: [c, c.withValues(alpha: 0)],
          ),
        ),
        child: SizedBox(height: height, width: double.infinity),
      ),
    );
  }
}

/// One row of a raised surface that has many: built as it comes into sight, and drawn as a part of the
/// whole.
class CardPart extends SingleChildRenderObjectWidget {
  const CardPart({required this.first, required this.last, required super.child, super.key});
  final bool first, last;
  @override
  RenderObject createRenderObject(BuildContext context) =>
      RenderCardPart(Ax.of(context).colors.plate, first, last);
  @override
  void updateRenderObject(BuildContext context, RenderCardPart renderObject) => renderObject
    ..spec = Ax.of(context).colors.plate
    ..ends = (first, last);
}

class RenderCardPart extends RenderProxyBox {
  RenderCardPart(this._spec, this._first, this._last);
  SurfaceSpec _spec;
  set spec(SurfaceSpec v) {
    if (identical(v, _spec)) return;
    _spec = v;
    markNeedsPaint();
  }

  bool _first, _last;
  set ends((bool, bool) v) {
    if (v == (_first, _last)) return;
    _first = v.$1;
    _last = v.$2;
    _shape = null;
    markNeedsPaint();
  }

  /// How far beyond its ends the whole goes on, and how far a shade reaches.
  static const _on = 48.0, _reach = 24.0;
  _Shape? _shape;
  Size? _for;

  @override
  void paint(PaintingContext context, Offset offset) {
    final top = _first ? 0.0 : _on, bottom = _last ? 0.0 : _on;
    if (_shape == null || _for != size) {
      _for = size;
      _shape = _Shape(Size(size.width, size.height + top + bottom), AxRadius.card);
    }
    final c = context.canvas..save();
    c.translate(offset.dx, offset.dy);
    c.clipRect(
      Rect.fromLTRB(
        -_reach,
        _first ? -_reach : 0,
        size.width + _reach,
        size.height + (_last ? _reach : 0),
      ),
    );
    c.translate(0, -top);
    paintSurface(c, _shape!, _spec, null);
    c.restore();
    super.paint(context, offset);
  }
}
