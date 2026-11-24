import 'package:flutter/widgets.dart';

import 'press.dart';
import 'tokens.dart';

enum AgentState {
  running('Running'),
  asking('Asking'),
  waiting('Waiting'),
  paused('Paused'),
  done('Done'),
  failed('Failed');

  const AgentState(this.word);
  final String word;
  Color mark(AxColors c) => switch (this) {
    running => c.live,
    asking => c.attention,
    done => c.done,
    failed => c.fault,
    waiting || paused => c.idle,
  };
  Color text(AxColors c) => switch (this) {
    running => c.liveText,
    asking => c.attentionText,
    done => c.doneText,
    failed => c.faultText,
    waiting || paused => c.ink3,
  };
}

/// A state lamp: a flat capsule. Shape backs up colour (waiting is hollow, paused is split), and a
/// running lamp breathes slowly. The word is always in the semantics.
class AxLamp extends StatefulWidget {
  const AxLamp(this.state, {super.key, this.width = 14, this.breathe = true});
  final AgentState state;
  final double width;
  final bool breathe;
  @override
  State<AxLamp> createState() => _AxLampState();
}

class _AxLampState extends State<AxLamp> with SingleTickerProviderStateMixin {
  late final _c = AnimationController(vsync: this, duration: AxMotion.breath);
  void _sync() {
    final run = widget.breathe && widget.state == AgentState.running && !reducedMotion(context);
    if (run && !_c.isAnimating) _c.repeat(reverse: true);
    if (!run) {
      _c.stop();
      _c.value = 0;
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _sync();
  }

  @override
  void didUpdateWidget(AxLamp old) {
    super.didUpdateWidget(old);
    _sync();
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Semantics(
    label: widget.state.word,
    child: RepaintBoundary(
      child: CustomPaint(
        size: Size(widget.width, 5),
        painter: _LampPainter(widget.state, widget.state.mark(Ax.of(context).colors), _c),
      ),
    ),
  );
}

class _LampPainter extends CustomPainter {
  _LampPainter(this.state, this.color, this.t) : super(repaint: t);
  final AgentState state;
  final Color color;
  final Animation<double> t;
  @override
  void paint(Canvas c, Size s) {
    final r = RRect.fromRectAndRadius(Offset.zero & s, Radius.circular(s.height / 2));
    final p = Paint()
      ..color = color.withValues(alpha: color.a * (1 - .45 * Curves.easeInOut.transform(t.value)));
    switch (state) {
      case AgentState.waiting:
        c.drawRRect(
          r.deflate(.6),
          p
            ..style = PaintingStyle.stroke
            ..strokeWidth = 1.2,
        );
      case AgentState.paused:
        final w = (s.width - 3) / 2;
        for (final x in [0.0, w + 3]) {
          c.drawRRect(
            RRect.fromRectAndRadius(Rect.fromLTWH(x, 0, w, s.height), const Radius.circular(1.5)),
            p,
          );
        }
      case _:
        c.drawRRect(r, p);
    }
  }

  @override
  bool shouldRepaint(_LampPainter o) => o.state != state || o.color != color;
}

/// How Axiom knows a number (requirement: never present an estimate as a report).
enum Provenance {
  reported('reported by the provider'),
  measured('measured by Axiom'),
  estimated('estimated'),
  unknown('not available');

  const Provenance(this.words);
  final String words;
}

/// A level. Solid when reported or measured, dashed when estimated, an empty slot when unknown.
class AxMeter extends StatelessWidget {
  const AxMeter({
    required this.label,
    required this.value,
    super.key,
    this.provenance = Provenance.measured,
    this.vertical = false,
    this.thickness = 4,
    this.warnAt = .85,
  });
  final String label;
  final double? value;
  final Provenance provenance;
  final bool vertical;
  final double thickness, warnAt;
  @override
  Widget build(BuildContext context) {
    final c = Ax.of(context).colors;
    final v = provenance == Provenance.unknown ? null : value;
    return Semantics(
      label: label,
      value: v == null ? 'not available' : '${(v * 100).round()}%, ${provenance.words}',
      child: CustomPaint(
        size: vertical ? Size(thickness, double.infinity) : Size(double.infinity, thickness),
        painter: _MeterPainter(
          v?.clamp(0, 1).toDouble(),
          provenance == Provenance.estimated,
          vertical,
          c.meterTrack,
          v != null && v >= warnAt ? c.attention : c.live,
          c.meterLow,
        ),
      ),
    );
  }
}

class _MeterPainter extends CustomPainter {
  const _MeterPainter(this.v, this.dashed, this.vertical, this.track, this.hot, this.low);
  final double? v;
  final bool dashed, vertical;
  final Color track, hot, low;
  @override
  void paint(Canvas c, Size s) {
    final full = Offset.zero & s;
    final radius = Radius.circular(s.shortestSide / 2);
    c.drawRRect(RRect.fromRectAndRadius(full, radius), Paint()..color = track);
    final x = v;
    if (x == null || x <= 0) return;
    final r = vertical
        ? Rect.fromLTRB(0, s.height * (1 - x), s.width, s.height)
        : Rect.fromLTWH(0, 0, s.width * x, s.height);
    c.saveLayer(full, Paint());
    c.clipRRect(RRect.fromRectAndRadius(r, radius));
    c.drawRect(
      r,
      Paint()
        ..shader = LinearGradient(
          begin: vertical ? Alignment.bottomCenter : Alignment.centerLeft,
          end: vertical ? Alignment.topCenter : Alignment.centerRight,
          colors: [low, hot],
        ).createShader(full),
    );
    if (dashed) {
      final cut = Paint()..blendMode = BlendMode.clear;
      for (var i = 3.0; i < (vertical ? s.height : s.width); i += 5) {
        c.drawRect(
          vertical ? Rect.fromLTWH(0, i, s.width, 2) : Rect.fromLTWH(i, 0, 2, s.height),
          cut,
        );
      }
    }
    c.restore();
  }

  @override
  bool shouldRepaint(_MeterPainter o) =>
      o.v != v || o.dashed != dashed || o.hot != hot || o.track != track || o.vertical != vertical;
}
