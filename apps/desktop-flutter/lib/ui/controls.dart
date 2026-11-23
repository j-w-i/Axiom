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

/// A button's look (the Tauri shell's `.btn` variants): a raised key, the brass primary, quiet
/// (no fill until hovered), danger (a raised key with red text).
enum ButtonKind { standard, primary, quiet, danger }

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
    this.trailing,
  });
  final String label;
  final VoidCallback? onPressed;
  final ButtonKind kind;
  final IconData16? icon;

  /// After the label, inside the button (the welcome's keycaps).
  final Widget? trailing;
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
        final fg = switch (kind) {
          ButtonKind.primary => c.onAccent,
          ButtonKind.danger => c.dangerText,
          ButtonKind.quiet => s.hovered && s.enabled ? c.ink : c.ink2,
          ButtonKind.standard => c.ink,
        };
        final content = Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (icon != null) ...[AxiomIcon(icon!, color: fg), const SizedBox(width: Metrics.s3)],
            Text(
              label,
              style: t.type.body.copyWith(
                color: fg,
                fontWeight: kind == ButtonKind.primary ? FontWeight.w600 : FontWeight.w500,
              ),
            ),
            if (trailing != null) ...[const SizedBox(width: Metrics.s4), trailing!],
          ],
        );
        final body = SizedBox(
          height: context.scaled(Metrics.controlHeight),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: Metrics.s5),
            child: Center(widthFactor: 1, child: content),
          ),
        );
        final down = s.pressed && s.enabled;
        final over = s.hovered && s.enabled;
        Widget box = switch (kind) {
          ButtonKind.quiet => DecoratedBox(
            decoration: BoxDecoration(
              color: down ? c.pressed : (over ? c.hover : null),
              borderRadius: BorderRadius.circular(Metrics.radiusControl),
            ),
            child: body,
          ),
          ButtonKind.primary => Raised(
            top: down ? c.accentLo : (over ? Color.lerp(c.accentHi, c.lift, 0.15) : c.accentHi),
            color: down ? c.accent : (c.isDark ? c.accentLo : c.accent),
            border: c.primaryBorder,
            lit: !down,
            child: body,
          ),
          _ => Raised(
            top: down ? c.raised : (over ? Color.lerp(c.raisedTop, c.lift, 0.12) : c.raisedTop),
            color: down ? c.raisedTop : (over ? Color.lerp(c.raised, c.lift, 0.10) : c.raised),
            lit: !down,
            child: body,
          ),
        };
        if (down && t.motion.pressMoves) {
          box = Transform.translate(offset: const Offset(0, 0.5), child: box);
        }
        box = Opacity(opacity: s.enabled ? 1 : 0.42, child: box);
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
    this.engaged = true,
    this.tooltipSide = OverlaySide.below,
  });
  final IconData16 icon;
  final String label;
  final VoidCallback? onPressed;

  /// Selected shows pressed into a well (a toggle that is on); the rail marks its selection
  /// itself and passes false.
  final bool engaged;

  /// Shown in the tooltip and the hint line, written with `Mod` (e.g. `Mod+B`).
  final String? shortcut;
  final bool? selected;
  final double size;
  final Color? color;
  final FocusNode? focusNode;

  /// Where the tooltip goes (right for the rail's buttons).
  final OverlaySide tooltipSide;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return AxTooltip(
      message: label,
      shortcut: shortcut,
      side: tooltipSide,
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
          final square = SizedBox.square(
            dimension: size,
            child: Center(child: AxiomIcon(icon, color: fg)),
          );
          return FocusRing(
            visible: s.focused,
            child: Opacity(
              opacity: s.enabled ? 1 : 0.42,
              // Engaged (a toggle that is on): pressed into a well, as Tauri's aria-pressed.
              child: (selected ?? false) && engaged
                  ? Well(child: square)
                  : DecoratedBox(
                      decoration: BoxDecoration(
                        color: s.pressed ? c.pressed : (s.hovered ? c.hover : null),
                        borderRadius: BorderRadius.circular(Metrics.radiusControl),
                      ),
                      child: square,
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
    // A spring is a glide: Standard only (motion-system.md §6).
    if (!m.glides) {
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
            // Tauri's composer send: an icon button in the brass text colour, faint when idle.
            final circle = DecoratedBox(
              decoration: BoxDecoration(
                color: s.pressed ? c.pressed : (s.hovered && s.enabled ? c.hover : null),
                borderRadius: BorderRadius.circular(Metrics.radiusControl),
              ),
              child: SizedBox.square(
                dimension: widget.diameter,
                child: Center(
                  child: AxiomIcon(widget.icon, color: s.enabled ? c.accentText : c.ink3),
                ),
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
              child: Raised(
                child: ConstrainedBox(
                  constraints: const BoxConstraints(minWidth: 16),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 1),
                    child: Text(
                      k,
                      textAlign: TextAlign.center,
                      style: t.type.tiny.copyWith(
                        height: 15 / 11,
                        color: t.colors.ink2,
                        fontWeight: FontWeight.w500,
                      ),
                    ),
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
        decoration: BoxDecoration(
          color: bg,
          borderRadius: BorderRadius.circular(Metrics.radiusPill),
        ),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: Metrics.s1 + 1),
          child: Text(
            count > 99 ? '99+' : '$count',
            style: t.type.numeric.copyWith(
              fontSize: 11,
              height: 14 / 11,
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

/// A status dot, optionally set in a cut-out ring of the surface it sits on (the "older Discord"
/// mark, where it overlaps something). The state is also given in words by the control around it.
class StatusDot extends StatelessWidget {
  const StatusDot(this.state, {super.key, this.size = 8, this.ring});
  final DotState state;
  final double size;

  /// The surface behind it, drawn as the cut-out ring; none by default (a ring in another tone
  /// than the surface reads as a halo; AUD-069). Pass the exact surface colour when the dot sits
  /// on something it must stand out from.
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
        decoration: ring == null ? null : BoxDecoration(color: ring, shape: BoxShape.circle),
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
        radius: Metrics.radiusRaised,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: Metrics.s1, vertical: 3),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              SizedBox.square(
                dimension: 14,
                child: value
                    ? DecoratedBox(
                        decoration: BoxDecoration(
                          color: c.accent,
                          borderRadius: BorderRadius.circular(Metrics.radiusControl),
                          border: Border.all(color: c.primaryBorder),
                        ),
                        child: AxiomIcon(_check, size: 14, color: c.onAccent),
                      )
                    : Well(border: c.lineInput, child: const SizedBox.expand()),
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
        radius: Metrics.radiusPill,
        child: Padding(
          padding: const EdgeInsets.all(Metrics.s1),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              // Tauri's switch: a recessed track with the input border; on, brass with the
              // primary border; a light gradient thumb. The thumb glides by a transform
              // (Standard only); the track's colour changes in step.
              AnimatedContainer(
                duration: t.motion.medium,
                curve: t.motion.curve,
                width: 30,
                height: 17,
                padding: const EdgeInsets.all(1),
                decoration: BoxDecoration(
                  color: value ? c.accent : c.recessed,
                  borderRadius: BorderRadius.circular(Metrics.radiusPill),
                  border: Border.all(color: value ? c.primaryBorder : c.lineInput),
                ),
                alignment: Alignment.centerLeft,
                child: TweenAnimationBuilder<double>(
                  tween: Tween(end: value ? 13 : 0),
                  duration: t.motion.glides ? t.motion.medium : Duration.zero,
                  curve: t.motion.curve,
                  builder: (context, x, child) =>
                      Transform.translate(offset: Offset(x, 0), child: child),
                  child: Container(
                    width: 13,
                    height: 13,
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      gradient: LinearGradient(
                        begin: Alignment.topCenter,
                        end: Alignment.bottomCenter,
                        colors: [c.thumbTop, c.thumbBottom],
                      ),
                      boxShadow: [
                        BoxShadow(color: c.contactShade, blurRadius: 2, offset: const Offset(0, 1)),
                      ],
                    ),
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
    this.skipTraversal = false,
    this.listFocused,
  });
  final String label;
  final Widget child;
  final VoidCallback? onPressed;
  final bool selected;
  final String? hint;

  /// At text scale 1: grown with the system's text scale ([AxiomContext.scaled]).
  final double height;
  final bool groove;
  final FocusNode? focusNode;

  /// A row of a list with one Tab stop ([RovingFocus]): out of the Tab order.
  final bool skipTraversal;

  /// Whether the list holds focus (the selected row's marker is brass then, whether focus came
  /// from the keyboard or a click); null follows this row's keyboard focus.
  final bool? listFocused;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final h = context.scaled(height);
    return Pressable(
      label: label,
      hint: hint,
      onPressed: onPressed,
      selected: selected,
      focusNode: focusNode,
      skipTraversal: skipTraversal,
      builder: (context, s) {
        Widget row = SizedBox(height: h, child: child);
        if (selected) {
          // Tauri's selected tree row: the selected tint and a 2 px marker on the left edge,
          // brass while the list has focus.
          row = DecoratedBox(
            decoration: BoxDecoration(color: c.selected),
            child: Stack(
              children: [
                row,
                Positioned(
                  left: 0,
                  top: 3,
                  bottom: 3,
                  width: 2,
                  child: ColoredBox(color: (listFocused ?? s.focused) ? c.accent : c.ink3),
                ),
              ],
            ),
          );
        } else if (s.hovered || s.pressed) {
          row = ColoredBox(color: s.pressed ? c.pressed : c.hover, child: row);
        }
        row = FocusRing(visible: s.focused, child: row);
        if (!groove) return row;
        return Column(mainAxisSize: MainAxisSize.min, children: [row, const Groove()]);
      },
    );
  }
}

/// A spinner, stepped at 8 frames a second (motion-system.md §5: `steps(8)` over 0.8 s). It
/// turns only at Standard motion, while the window is active ([WindowActivity]) and its ticker
/// runs (TickerMode stops it on hidden pages); otherwise, or when [still], it is a still arc.
/// A region shows one turning spinner; the others pass [still] (§5.3).
class Spinner extends StatefulWidget {
  const Spinner({super.key, this.size = 14, this.label = 'Working', this.still = false});
  final double size;
  final String label;

  /// Drawn still (another spinner in the same region already turns).
  final bool still;

  /// Frames per second and per turn.
  static const steps = 8;

  @override
  State<Spinner> createState() => _SpinnerState();
}

class _SpinnerState extends State<Spinner> with SingleTickerProviderStateMixin {
  late final Ticker _ticker = createTicker(_tick);
  int _step = 0;
  final _active = WindowActivity.instance.active;

  void _tick(Duration d) {
    final step = (d.inMilliseconds * Spinner.steps ~/ 1000) % Spinner.steps;
    if (step != _step) setState(() => _step = step);
  }

  bool get _turns => !widget.still && _active.value && context.axiom.motion.loops;

  void _sync() {
    if (!mounted) return;
    final turns = _turns;
    if (turns && !_ticker.isActive) _ticker.start();
    if (!turns && _ticker.isActive) _ticker.stop();
  }

  @override
  void initState() {
    super.initState();
    _active.addListener(_sync);
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _sync();
  }

  @override
  void didUpdateWidget(Spinner old) {
    super.didUpdateWidget(old);
    _sync();
  }

  /// Whether it is turning (tests).
  @visibleForTesting
  bool get turning => _ticker.isActive;

  @override
  void dispose() {
    _active.removeListener(_sync);
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
        child: CustomPaint(
          painter: _ArcPainter(c.accent, c.hairline, _step * 2 * math.pi / Spinner.steps),
        ),
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
