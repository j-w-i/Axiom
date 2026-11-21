// The interaction base of every control: focus (with a visible ring for keyboard focus), hover,
// pressed, Enter/Space activation, semantics (name, role, enabled), and the hint line: the
// hovered or focused control publishes its "Action — Shortcut" hint (workspace-spec §2.5).

import 'dart:ui' show SemanticsRole;

import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'theme.dart';
import 'tokens.dart';

/// The hint line's source: the hovered or focused control's hint, or null.
class HintScope extends InheritedWidget {
  const HintScope({super.key, required this.hint, required super.child});
  final ValueNotifier<String?> hint;

  static ValueNotifier<String?>? maybeOf(BuildContext context) =>
      context.getInheritedWidgetOfExactType<HintScope>()?.hint;

  @override
  bool updateShouldNotify(HintScope old) => old.hint != hint;
}

/// Whether focus rings show: after keyboard input, not after a pointer press (the web's
/// `:focus-visible`). One per app, fed by the keyboard and the pointer router.
final class FocusVisibility {
  FocusVisibility._() {
    HardwareKeyboard.instance.addHandler(_onKey);
    GestureBinding.instance.pointerRouter.addGlobalRoute(_onPointer);
  }
  static FocusVisibility? _instance;
  static FocusVisibility get instance => _instance ??= FocusVisibility._();

  final keyboard = ValueNotifier<bool>(false);

  bool _onKey(KeyEvent e) {
    if (e is KeyDownEvent) keyboard.value = true;
    return false;
  }

  void _onPointer(PointerEvent e) {
    if (e is PointerDownEvent) keyboard.value = false;
  }
}

/// The state a [Pressable] builds its look from.
@immutable
final class PressState {
  const PressState({
    required this.hovered,
    required this.pressed,
    required this.focused,
    required this.enabled,
  });
  final bool hovered, pressed, focused, enabled;
}

enum PressRole { button, tab, link, menuItem, toggle, checkbox }

class Pressable extends StatefulWidget {
  const Pressable({
    super.key,
    required this.label,
    required this.builder,
    this.onPressed,
    this.hint,
    this.role = PressRole.button,
    this.selected,
    this.checked,
    this.expanded,
    this.focusNode,
    this.autofocus = false,
    this.cursor = SystemMouseCursors.click,
    this.excludeChildSemantics = true,
  });

  /// The accessible name.
  final String label;
  final Widget Function(BuildContext context, PressState state) builder;
  final VoidCallback? onPressed;

  /// "Action — Shortcut" for the hint line; defaults to [label].
  final String? hint;
  final PressRole role;
  final bool? selected;
  final bool? checked;

  /// For tree rows and disclosure buttons.
  final bool? expanded;
  final FocusNode? focusNode;
  final bool autofocus;
  final MouseCursor cursor;
  final bool excludeChildSemantics;

  @override
  State<Pressable> createState() => _PressableState();
}

class _PressableState extends State<Pressable> {
  FocusNode? _own;
  FocusNode get _focus => widget.focusNode ?? (_own ??= FocusNode(debugLabel: widget.label));
  bool _hovered = false;
  bool _pressed = false;
  bool _focused = false;

  bool get _enabled => widget.onPressed != null;

  void _hint(bool on) {
    final h = HintScope.maybeOf(context);
    if (h == null) return;
    final text = widget.hint ?? widget.label;
    if (on) {
      h.value = text;
    } else if (h.value == text) {
      h.value = null;
    }
  }

  @override
  void initState() {
    super.initState();
    FocusVisibility.instance.keyboard.addListener(_onModality);
  }

  void _onModality() {
    if (_focused && mounted) setState(() {});
  }

  @override
  void dispose() {
    FocusVisibility.instance.keyboard.removeListener(_onModality);
    _own?.dispose();
    super.dispose();
  }

  void _activate() {
    if (_enabled) widget.onPressed!();
  }

  @override
  Widget build(BuildContext context) {
    final state = PressState(
      hovered: _hovered && _enabled,
      pressed: _pressed && _enabled,
      focused: _focused && FocusVisibility.instance.keyboard.value,
      enabled: _enabled,
    );
    return Semantics(
      container: true,
      label: widget.label,
      button: widget.role == PressRole.button || widget.role == PressRole.menuItem,
      link: widget.role == PressRole.link,
      selected: widget.selected,
      checked: widget.role == PressRole.checkbox ? (widget.checked ?? false) : null,
      toggled: widget.role == PressRole.toggle ? (widget.checked ?? false) : null,
      enabled: _enabled,
      expanded: widget.expanded,
      role: switch (widget.role) {
        PressRole.tab => SemanticsRole.tab,
        PressRole.menuItem => SemanticsRole.menuItem,
        _ => null,
      },
      onTap: _enabled ? _activate : null,
      excludeSemantics: widget.excludeChildSemantics,
      child: FocusableActionDetector(
        focusNode: _focus,
        autofocus: widget.autofocus,
        enabled: _enabled,
        mouseCursor: _enabled ? widget.cursor : SystemMouseCursors.basic,
        onShowHoverHighlight: (v) {
          setState(() => _hovered = v);
          _hint(v);
        },
        onFocusChange: (v) {
          setState(() => _focused = v);
          _hint(v);
        },
        shortcuts: const {
          SingleActivator(LogicalKeyboardKey.enter): ActivateIntent(),
          SingleActivator(LogicalKeyboardKey.numpadEnter): ActivateIntent(),
          SingleActivator(LogicalKeyboardKey.space): ActivateIntent(),
        },
        actions: {ActivateIntent: CallbackAction<ActivateIntent>(onInvoke: (_) => _activate())},
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTapDown: _enabled
              ? (_) {
                  // A click focuses the control (the ring shows only for keyboard focus).
                  _focus.requestFocus();
                  setState(() => _pressed = true);
                }
              : null,
          onTapUp: _enabled ? (_) => setState(() => _pressed = false) : null,
          onTapCancel: _enabled ? () => setState(() => _pressed = false) : null,
          onTap: _enabled ? _activate : null,
          child: widget.builder(context, state),
        ),
      ),
    );
  }
}

/// The keyboard focus ring around [child].
class FocusRing extends StatelessWidget {
  const FocusRing({
    super.key,
    required this.visible,
    required this.child,
    this.radius = Metrics.radiusControl,
  });
  final bool visible;
  final Widget child;
  final double radius;

  @override
  Widget build(BuildContext context) {
    if (!visible) return child;
    return CustomPaint(foregroundPainter: _RingPainter(context.colors.focus, radius), child: child);
  }
}

class _RingPainter extends CustomPainter {
  _RingPainter(this.color, this.radius);
  final Color color;
  final double radius;

  @override
  void paint(Canvas canvas, Size size) {
    final r = RRect.fromRectAndRadius((Offset.zero & size).deflate(1), Radius.circular(radius));
    canvas.drawRRect(
      r,
      Paint()
        ..color = color
        ..style = PaintingStyle.stroke
        ..strokeWidth = Metrics.focusWidth,
    );
  }

  @override
  bool shouldRepaint(_RingPainter o) => o.color != color || o.radius != radius;
}
