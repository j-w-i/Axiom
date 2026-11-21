// A splitter between two regions: drag, or keyboard (←/→ or ↑/↓ by 16, Home to the default),
// with a value announced like the Tauri shell's separators.

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'pressable.dart';
import 'theme.dart';
import 'tokens.dart';

class AxSplitter extends StatefulWidget {
  const AxSplitter({
    super.key,
    required this.label,
    required this.value,
    required this.min,
    required this.max,
    required this.defaultValue,
    required this.onChanged,
    this.vertical = true,
    this.invert = false,
    this.hint,
  });

  /// e.g. "Resize the side bar".
  final String label;

  /// The size of the region it resizes, in logical pixels.
  final double value;
  final double min, max, defaultValue;
  final ValueChanged<double> onChanged;

  /// A vertical bar (resizes widths); false for a horizontal bar (heights).
  final bool vertical;

  /// The region grows when dragged towards the start (a panel below, a side bar on the right).
  final bool invert;
  final String? hint;

  @override
  State<AxSplitter> createState() => _AxSplitterState();
}

class _AxSplitterState extends State<AxSplitter> {
  bool _hover = false;
  bool _drag = false;
  bool _focusVisible = false;

  void _set(double v) => widget.onChanged(v.clamp(widget.min, widget.max).roundToDouble());

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final step = 16.0;
    final grow = widget.invert ? -1.0 : 1.0;
    final active = _hover || _drag || _focusVisible;
    return Semantics(
      label: widget.label,
      value: '${widget.value.round()}',
      increasedValue: '${(widget.value + step).clamp(widget.min, widget.max).round()}',
      decreasedValue: '${(widget.value - step).clamp(widget.min, widget.max).round()}',
      onIncrease: () => _set(widget.value + step),
      onDecrease: () => _set(widget.value - step),
      slider: true,
      child: FocusableActionDetector(
        mouseCursor: widget.vertical
            ? SystemMouseCursors.resizeColumn
            : SystemMouseCursors.resizeRow,
        onShowHoverHighlight: (v) {
          setState(() => _hover = v);
          final h = HintScope.maybeOf(context);
          if (h != null) {
            final text = widget.hint ?? '${widget.label} — Drag, or Arrow keys';
            if (v) {
              h.value = text;
            } else if (h.value == text) {
              h.value = null;
            }
          }
        },
        onShowFocusHighlight: (v) => setState(() => _focusVisible = v),
        shortcuts: {
          SingleActivator(
            widget.vertical ? LogicalKeyboardKey.arrowRight : LogicalKeyboardKey.arrowDown,
          ): _NudgeIntent(
            step * grow,
          ),
          SingleActivator(
            widget.vertical ? LogicalKeyboardKey.arrowLeft : LogicalKeyboardKey.arrowUp,
          ): _NudgeIntent(
            -step * grow,
          ),
          const SingleActivator(LogicalKeyboardKey.home): const _ResetIntent(),
        },
        actions: {
          _NudgeIntent: CallbackAction<_NudgeIntent>(onInvoke: (i) => _set(widget.value + i.delta)),
          _ResetIntent: CallbackAction<_ResetIntent>(onInvoke: (_) => _set(widget.defaultValue)),
        },
        child: GestureDetector(
          behavior: HitTestBehavior.translucent,
          onHorizontalDragStart: widget.vertical ? (_) => setState(() => _drag = true) : null,
          onHorizontalDragUpdate: widget.vertical
              ? (d) => _set(widget.value + d.delta.dx * grow)
              : null,
          onHorizontalDragEnd: widget.vertical ? (_) => setState(() => _drag = false) : null,
          onVerticalDragStart: widget.vertical ? null : (_) => setState(() => _drag = true),
          onVerticalDragUpdate: widget.vertical
              ? null
              : (d) => _set(widget.value + d.delta.dy * grow),
          onVerticalDragEnd: widget.vertical ? null : (_) => setState(() => _drag = false),
          onDoubleTap: () => _set(widget.defaultValue),
          child: SizedBox(
            width: widget.vertical ? 5 : double.infinity,
            height: widget.vertical ? double.infinity : 5,
            child: Center(
              child: Container(
                width: widget.vertical ? (active ? 2 : 1) : double.infinity,
                height: widget.vertical ? double.infinity : (active ? 2 : 1),
                color: _focusVisible ? c.focus : (active ? c.accent : c.groove),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _NudgeIntent extends Intent {
  const _NudgeIntent(this.delta);
  final double delta;
}

class _ResetIntent extends Intent {
  const _ResetIntent();
}

/// The width [Metrics] reserve for a splitter.
const splitterThickness = 5.0;
