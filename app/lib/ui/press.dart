import 'package:flutter/gestures.dart';
import 'package:flutter/semantics.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'tokens.dart';

class PressState {
  const PressState({
    this.hovered = false,
    this.pressed = false,
    this.focused = false,
    this.enabled = true,
  });
  final bool hovered, pressed, focused, enabled;
}

/// The one interaction primitive: pointer, keyboard, focus and semantics in one place, so every
/// control behaves the same. Enter and Space activate; the focus ring shows for the keyboard only.
class Pressable extends StatefulWidget {
  const Pressable({
    required this.label,
    required this.builder,
    required this.onPressed,
    super.key,
    this.hint,
    this.selected,
    this.toggled,
    this.expanded,
    this.checked,
    this.actions,
    this.button = true,
    this.radius = AxRadius.key,
    this.autofocus = false,
    this.focusNode,
    this.onSecondary,
  });
  final String label;
  final String? hint;
  final bool? selected, toggled, expanded, checked;

  /// Further things a screen reader can do here.
  final Map<CustomSemanticsAction, VoidCallback>? actions;
  final bool button, autofocus;
  final double radius;
  final VoidCallback? onPressed;
  final void Function(Offset global)? onSecondary;
  final FocusNode? focusNode;
  final Widget Function(BuildContext, PressState) builder;
  @override
  State<Pressable> createState() => _PressableState();
}

class _PressableState extends State<Pressable> {
  bool _hover = false, _down = false, _focus = false;
  bool get _enabled => widget.onPressed != null;

  void _activate() => widget.onPressed?.call();

  @override
  void didUpdateWidget(Pressable old) {
    super.didUpdateWidget(old);
    if (!_enabled) _down = false;
  }

  @override
  Widget build(BuildContext context) {
    final c = Ax.of(context).colors;
    final s = PressState(
      hovered: _hover && _enabled,
      pressed: _down && _enabled,
      focused: _focus,
      enabled: _enabled,
    );
    return Semantics(
      container: true,
      button: widget.button,
      enabled: _enabled,
      selected: widget.selected,
      toggled: widget.toggled,
      expanded: widget.expanded,
      checked: widget.checked,
      focusable: _enabled,
      focused: _focus,
      customSemanticsActions: widget.actions,
      label: widget.label,
      hint: widget.hint,
      onTap: _enabled ? _activate : null,
      excludeSemantics: true,
      child: FocusableActionDetector(
        enabled: _enabled,
        autofocus: widget.autofocus,
        focusNode: widget.focusNode,
        mouseCursor: _enabled ? SystemMouseCursors.click : SystemMouseCursors.basic,
        onShowHoverHighlight: (v) => setState(() => _hover = v),
        onShowFocusHighlight: (v) => setState(() => _focus = v),
        shortcuts: const {
          SingleActivator(LogicalKeyboardKey.enter): ActivateIntent(),
          SingleActivator(LogicalKeyboardKey.space): ActivateIntent(),
        },
        actions: {
          ActivateIntent: CallbackAction<ActivateIntent>(
            onInvoke: (_) {
              _activate();
              return null;
            },
          ),
        },
        child: RawGestureDetector(
          behavior: HitTestBehavior.opaque,
          gestures: {
            TapGestureRecognizer: GestureRecognizerFactoryWithHandlers<TapGestureRecognizer>(
              TapGestureRecognizer.new,
              (t) => t
                ..onTapDown = _enabled ? ((_) => setState(() => _down = true)) : null
                ..onTapUp = _enabled ? ((_) => setState(() => _down = false)) : null
                ..onTapCancel = _enabled ? (() => setState(() => _down = false)) : null
                ..onTap = _enabled ? _activate : null
                ..onSecondaryTapUp = widget.onSecondary == null
                    ? null
                    : ((d) => widget.onSecondary!(d.globalPosition)),
            ),
          },
          child: CustomPaint(
            foregroundPainter: _focus ? FocusRing(c.focus, widget.radius) : null,
            child: widget.builder(context, s),
          ),
        ),
      ),
    );
  }
}

class FocusRing extends CustomPainter {
  const FocusRing(this.color, this.radius);
  final Color color;
  final double radius;
  @override
  void paint(Canvas c, Size s) => c.drawRRect(
    // A part that fills its row gets the ring inside, where nothing paints over it.
    RRect.fromRectAndRadius(
      radius == 0 ? (Offset.zero & s).deflate(2) : (Offset.zero & s).inflate(2),
      Radius.circular(radius == 0 ? 3 : radius + 2),
    ),
    Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2
      ..color = color,
  );
  @override
  bool shouldRepaint(FocusRing o) => o.color != color || o.radius != radius;
}

bool reducedMotion(BuildContext c) => MediaQuery.maybeDisableAnimationsOf(c) ?? false;
