// Axiom's controls on the widgets layer (ADR-0020 §1, §4).

import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/physics.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';

import 'icon_data.dart';
import 'overlays.dart';
import 'pressable.dart';
import 'surfaces.dart';
import 'theme.dart';
import 'tokens.dart';

enum ButtonKind { standard, quiet, danger }

/// A text button (optionally with a leading icon).
class AxButton extends StatelessWidget {
  const AxButton({
    super.key,
    required this.label,
    required this.onPressed,
    this.kind = ButtonKind.standard,
    this.icon,
    this.hint,
    this.autofocus = false,
    this.focusNode,
  });
  final String label;
  final VoidCallback? onPressed;
  final ButtonKind kind;
  final IconData16? icon;
  final String? hint;
  final bool autofocus;
  final FocusNode? focusNode;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    return Pressable(
      label: label,
      hint: hint,
      onPressed: onPressed,
      autofocus: autofocus,
      focusNode: focusNode,
      builder: (context, s) {
        final fg = !s.enabled
            ? c.ink3
            : kind == ButtonKind.danger
            ? c.dangerText
            : c.ink;
        final content = Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (icon != null) ...[AxiomIcon(icon!, color: fg), const SizedBox(width: Metrics.s3)],
            Text(label, style: t.type.label.copyWith(color: fg)),
          ],
        );
        final body = SizedBox(
          height: Metrics.controlHeight,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 10),
            child: Center(widthFactor: 1, child: content),
          ),
        );
        final overlay = s.pressed ? c.pressed : (s.hovered ? c.hover : null);
        Widget box = kind == ButtonKind.quiet
            ? DecoratedBox(
                decoration: BoxDecoration(
                  color: overlay,
                  borderRadius: BorderRadius.circular(Metrics.radiusControl),
                ),
                child: body,
              )
            : Raised(
                radius: Metrics.radiusControl,
                color: overlay == null ? c.raised : Color.alphaBlend(overlay, c.raised),
                child: body,
              );
        box = Opacity(opacity: s.enabled ? 1 : 0.55, child: box);
        return FocusRing(visible: s.focused, child: box);
      },
    );
  }
}

/// A square icon button with a tooltip (its name, and the shortcut as keycaps).
class AxIconButton extends StatelessWidget {
  const AxIconButton({
    super.key,
    required this.icon,
    required this.label,
    required this.onPressed,
    this.shortcut,
    this.selected,
    this.size = Metrics.controlHeight,
    this.color,
    this.focusNode,
  });
  final IconData16 icon;
  final String label;
  final VoidCallback? onPressed;

  /// Shown in the tooltip and the hint line, written with `Mod` (e.g. `Mod+B`).
  final String? shortcut;
  final bool? selected;
  final double size;
  final Color? color;
  final FocusNode? focusNode;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return AxTooltip(
      message: label,
      shortcut: shortcut,
      child: Pressable(
        label: label,
        hint: shortcut == null ? label : '$label — $shortcut',
        onPressed: onPressed,
        selected: selected,
        focusNode: focusNode,
        builder: (context, s) {
          final fg = !s.enabled
              ? c.ink3
              : (selected ?? false) || s.hovered
              ? c.ink
              : (color ?? c.ink2);
          return FocusRing(
            visible: s.focused,
            child: DecoratedBox(
              decoration: BoxDecoration(
                color: s.pressed ? c.pressed : (s.hovered ? c.hover : null),
                borderRadius: BorderRadius.circular(Metrics.radiusControl),
              ),
              child: SizedBox.square(
                dimension: size,
                child: Center(child: AxiomIcon(icon, color: fg)),
              ),
            ),
          );
        },
      ),
    );
  }
}

/// The one high-contrast action of a surface (Send): an ink circle. It answers a press with a
/// small spring (about 4 % overshoot).
class InkCircleButton extends StatefulWidget {
  const InkCircleButton({
    super.key,
    required this.icon,
    required this.label,
    required this.onPressed,
    this.shortcut,
    this.diameter = 28,
  });
  final IconData16 icon;
  final String label;
  final VoidCallback? onPressed;
  final String? shortcut;
  final double diameter;

  @override
  State<InkCircleButton> createState() => _InkCircleButtonState();
}

class _InkCircleButtonState extends State<InkCircleButton> with SingleTickerProviderStateMixin {
  late final AnimationController _scale = AnimationController.unbounded(vsync: this, value: 1);

  void _spring(double to) {
    final m = context.axiom.motion;
    if (!m.animates) {
      _scale.value = to;
      return;
    }
    _scale.animateWith(SpringSimulation(m.expressive, _scale.value, to, _scale.velocity));
  }

  @override
  void dispose() {
    _scale.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return AxTooltip(
      message: widget.label,
      shortcut: widget.shortcut,
      child: Listener(
        onPointerDown: widget.onPressed == null ? null : (_) => _spring(0.9),
        onPointerUp: (_) => _spring(1),
        onPointerCancel: (_) => _spring(1),
        child: Pressable(
          label: widget.label,
          hint: widget.shortcut == null ? widget.label : '${widget.label} — ${widget.shortcut}',
          onPressed: widget.onPressed,
          builder: (context, s) {
            final fill = s.enabled ? c.ink : c.raised;
            final circle = DecoratedBox(
              decoration: BoxDecoration(color: fill, shape: BoxShape.circle),
              child: SizedBox.square(
                dimension: widget.diameter,
                child: Center(child: AxiomIcon(widget.icon, color: s.enabled ? c.onInk : c.ink3)),
              ),
            );
            return FocusRing(
              visible: s.focused,
              radius: widget.diameter,
              child: AnimatedBuilder(
                animation: _scale,
                builder: (_, child) => Transform.scale(scale: _scale.value, child: child),
                child: circle,
              ),
            );
          },
        ),
      ),
    );
  }
}

/// Keycaps for a shortcut written with `Mod` ("Mod+Shift+P" → Ctrl ⇧ P, or ⌘ ⇧ P on macOS).
class Keycaps extends StatelessWidget {
  const Keycaps(this.shortcut, {super.key, this.mac});
  final String shortcut;
  final bool? mac;

  static List<String> keys(String shortcut, {required bool mac}) => [
    for (final k in shortcut.split(RegExp(r'[+ ]')).where((k) => k.isNotEmpty))
      switch (k) {
        'Mod' => mac ? '⌘' : 'Ctrl',
        'Alt' => mac ? '⌥' : 'Alt',
        'Shift' => mac ? '⇧' : 'Shift',
        _ => k,
      },
  ];

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final isMac = mac ?? defaultTargetPlatform == TargetPlatform.macOS;
    return ExcludeSemantics(
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          for (final k in keys(shortcut, mac: isMac))
            Padding(
              padding: const EdgeInsets.only(left: Metrics.s1),
              child: DecoratedBox(
                decoration: BoxDecoration(
                  color: t.colors.recessed,
                  borderRadius: BorderRadius.circular(4),
                  border: Border.all(color: t.colors.hairline),
                ),
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 1),
                  child: Text(
                    k,
                    style: t.type.numeric.copyWith(fontSize: 11, color: t.colors.ink2),
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

/// A count badge (e.g. on a rail item).
class AxBadge extends StatelessWidget {
  const AxBadge(this.count, {super.key, this.attention = true});
  final int count;
  final bool attention;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final bg = attention ? t.colors.accent : t.colors.raised;
    final fg = attention ? t.colors.onAccent : t.colors.ink2;
    return ExcludeSemantics(
      child: DecoratedBox(
        decoration: BoxDecoration(color: bg, borderRadius: BorderRadius.circular(8)),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 5),
          child: Text(
            count > 99 ? '99+' : '$count',
            style: t.type.numeric.copyWith(
              fontSize: 10,
              height: 1.5,
              color: fg,
              fontWeight: FontWeight.w600,
            ),
          ),
        ),
      ),
    );
  }
}

enum DotState { off, ok, running, warn, error }

/// A status dot set in a cut-out ring of the surface it sits on (the "older Discord" mark). The
/// state is also given in words by the control around it.
class StatusDot extends StatelessWidget {
  const StatusDot(this.state, {super.key, this.size = 8, this.ring});
  final DotState state;
  final double size;

  /// The surface behind it (the cut-out); defaults to the bar tone.
  final Color? ring;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final fill = switch (state) {
      DotState.off => c.ink3,
      DotState.ok => c.ok,
      DotState.running => c.accent,
      DotState.warn => c.warn,
      DotState.error => c.danger,
    };
    return ExcludeSemantics(
      child: Container(
        width: size + 4,
        height: size + 4,
        decoration: BoxDecoration(color: ring ?? c.bar, shape: BoxShape.circle),
        alignment: Alignment.center,
        child: Container(
          width: size,
          height: size,
          decoration: BoxDecoration(color: fill, shape: BoxShape.circle),
        ),
      ),
    );
  }
}

/// A checkbox with its label.
class AxCheckbox extends StatelessWidget {
  const AxCheckbox({super.key, required this.label, required this.value, required this.onChanged});
  final String label;
  final bool value;
  final ValueChanged<bool>? onChanged;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    return Pressable(
      label: label,
      role: PressRole.checkbox,
      checked: value,
      onPressed: onChanged == null ? null : () => onChanged!(!value),
      builder: (context, s) => FocusRing(
        visible: s.focused,
        radius: 4,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 2, vertical: 3),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                width: 14,
                height: 14,
                decoration: BoxDecoration(
                  color: value ? c.accent : c.recessed,
                  borderRadius: BorderRadius.circular(3),
                  border: Border.all(color: value ? c.accent : c.ink3),
                ),
                child: value ? AxiomIcon(_check, size: 14, color: c.onAccent) : null,
              ),
              const SizedBox(width: Metrics.s3),
              Text(label, style: t.type.body.copyWith(color: s.enabled ? c.ink : c.ink3)),
            ],
          ),
        ),
      ),
    );
  }
}

const _check = IconData16([IconPath('M4 8.5l2.5 2.5L12 5.5')]);

/// An on/off switch with its label.
class AxToggle extends StatelessWidget {
  const AxToggle({super.key, required this.label, required this.value, required this.onChanged});
  final String label;
  final bool value;
  final ValueChanged<bool>? onChanged;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    return Pressable(
      label: label,
      role: PressRole.toggle,
      checked: value,
      onPressed: onChanged == null ? null : () => onChanged!(!value),
      builder: (context, s) => FocusRing(
        visible: s.focused,
        radius: 10,
        child: Padding(
          padding: const EdgeInsets.all(2),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              AnimatedContainer(
                duration: t.motion.fast,
                curve: t.motion.curve,
                width: 26,
                height: 16,
                padding: const EdgeInsets.all(2),
                decoration: BoxDecoration(
                  color: value ? c.accent : c.recessed,
                  borderRadius: BorderRadius.circular(8),
                  border: Border.all(color: value ? c.accent : c.ink3),
                ),
                alignment: value ? Alignment.centerRight : Alignment.centerLeft,
                child: Container(
                  width: 10,
                  height: 10,
                  decoration: BoxDecoration(
                    color: value ? c.onAccent : c.ink2,
                    shape: BoxShape.circle,
                  ),
                ),
              ),
              const SizedBox(width: Metrics.s3),
              Text(label, style: t.type.body.copyWith(color: s.enabled ? c.ink : c.ink3)),
            ],
          ),
        ),
      ),
    );
  }
}

/// A list row: hover, a raised look when selected, an optional groove under it.
class AxListRow extends StatelessWidget {
  const AxListRow({
    super.key,
    required this.label,
    required this.child,
    this.onPressed,
    this.selected = false,
    this.hint,
    this.height = Metrics.rowHeight,
    this.groove = false,
    this.focusNode,
  });
  final String label;
  final Widget child;
  final VoidCallback? onPressed;
  final bool selected;
  final String? hint;
  final double height;
  final bool groove;
  final FocusNode? focusNode;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Pressable(
      label: label,
      hint: hint,
      onPressed: onPressed,
      selected: selected,
      focusNode: focusNode,
      builder: (context, s) {
        Widget row = SizedBox(height: height, child: child);
        if (selected) {
          row = Raised(radius: Metrics.radiusControl, child: row);
        } else if (s.hovered || s.pressed) {
          row = DecoratedBox(
            decoration: BoxDecoration(
              color: s.pressed ? c.pressed : c.hover,
              borderRadius: BorderRadius.circular(Metrics.radiusControl),
            ),
            child: row,
          );
        }
        row = FocusRing(visible: s.focused, child: row);
        if (!groove) return row;
        return Column(mainAxisSize: MainAxisSize.min, children: [row, const Groove()]);
      },
    );
  }
}

/// A smooth spinner. It turns only while its ticker runs (stopped when the window is hidden, by
/// TickerMode) and is a still arc with motion off.
class Spinner extends StatefulWidget {
  const Spinner({super.key, this.size = 14, this.label = 'Working'});
  final double size;
  final String label;

  @override
  State<Spinner> createState() => _SpinnerState();
}

class _SpinnerState extends State<Spinner> with SingleTickerProviderStateMixin {
  late final Ticker _ticker = createTicker((d) => setState(() => _t = d.inMicroseconds / 1e6));
  double _t = 0;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final animates = context.axiom.motion.animates;
    if (animates && !_ticker.isActive) _ticker.start();
    if (!animates && _ticker.isActive) _ticker.stop();
  }

  @override
  void dispose() {
    _ticker.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Semantics(
      label: widget.label,
      child: SizedBox.square(
        dimension: widget.size,
        child: CustomPaint(painter: _ArcPainter(c.accent, c.hairline, _t * 2 * math.pi * 1.2)),
      ),
    );
  }
}

class _ArcPainter extends CustomPainter {
  _ArcPainter(this.color, this.track, this.angle);
  final Color color, track;
  final double angle;

  @override
  void paint(Canvas canvas, Size size) {
    final rect = (Offset.zero & size).deflate(1.5);
    final p = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.8
      ..strokeCap = StrokeCap.round;
    canvas.drawArc(rect, 0, 2 * math.pi, false, p..color = track);
    canvas.drawArc(rect, angle, math.pi * 0.6, false, p..color = color);
  }

  @override
  bool shouldRepaint(_ArcPainter o) => o.angle != angle || o.color != color;
}

/// Answer-first: the page's one plain sentence (ADR-0020 §4).
class AnswerLine extends StatelessWidget {
  const AnswerLine(this.text, {super.key, this.detail});
  final String text;
  final String? detail;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    return Semantics(
      header: true,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(text, style: t.type.answer),
          if (detail != null)
            Padding(
              padding: const EdgeInsets.only(top: Metrics.s2),
              child: Text(detail!, style: t.type.small),
            ),
        ],
      ),
    );
  }
}
