// A single- or multi-line text field in a recessed well, with a placeholder, a focus ring and its
// accessible name.

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'pressable.dart';
import 'surfaces.dart';
import 'theme.dart';
import 'tokens.dart';

class AxTextInput extends StatefulWidget {
  const AxTextInput({
    super.key,
    required this.label,
    this.controller,
    this.focusNode,
    this.placeholder,
    this.onChanged,
    this.onSubmitted,
    this.autofocus = false,
    this.maxLines = 1,
    this.minLines,
    this.mono = false,
    this.leading,
    this.trailing,
    this.bezel = false,
    this.obscure = false,
    this.inputFormatters,
    this.textInputAction,
  });

  /// The accessible name (and the hint line's text).
  final String label;
  final TextEditingController? controller;
  final FocusNode? focusNode;
  final String? placeholder;
  final ValueChanged<String>? onChanged;
  final ValueChanged<String>? onSubmitted;
  final bool autofocus;
  final int? maxLines;
  final int? minLines;
  final bool mono;
  final Widget? leading;
  final Widget? trailing;

  /// The composer's look: the same field with more room.
  final bool bezel;
  final bool obscure;
  final List<TextInputFormatter>? inputFormatters;
  final TextInputAction? textInputAction;

  @override
  State<AxTextInput> createState() => _AxTextInputState();
}

class _AxTextInputState extends State<AxTextInput> {
  TextEditingController? _ownController;
  FocusNode? _ownFocus;
  late TextEditingController _controller;
  late FocusNode _focus;
  bool _focused = false;

  @override
  void initState() {
    super.initState();
    _bind();
  }

  /// The widget's controller and focus node, or this state's own.
  void _bind() {
    _controller = widget.controller ?? (_ownController ??= TextEditingController());
    _focus = widget.focusNode ?? (_ownFocus ??= FocusNode(debugLabel: widget.label));
    _focus.addListener(_onFocus);
    _controller.addListener(_onText);
    _focused = _focus.hasFocus;
  }

  @override
  void didUpdateWidget(AxTextInput old) {
    super.didUpdateWidget(old);
    // The element can be reused for another field (a form whose rows change): follow the new
    // widget's controller and focus node.
    if (old.controller != widget.controller || old.focusNode != widget.focusNode) {
      _focus.removeListener(_onFocus);
      _controller.removeListener(_onText);
      _bind();
    }
  }

  void _onFocus() {
    if (_focused == _focus.hasFocus) return;
    setState(() => _focused = _focus.hasFocus);
    final h = HintScope.maybeOf(context);
    if (h == null) return;
    if (_focused) {
      h.value = widget.label;
    } else if (h.value == widget.label) {
      h.value = null;
    }
  }

  void _onText() => setState(() {});

  @override
  void dispose() {
    _focus.removeListener(_onFocus);
    _controller.removeListener(_onText);
    _ownController?.dispose();
    _ownFocus?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final style = widget.mono ? t.type.code : t.type.body;
    // The name is merged into EditableText's own text-field node (a wrapper node would leave
    // the field itself unnamed).
    final field = MergeSemantics(
      child: Semantics(
        label: widget.label,
        child: EditableText(
          controller: _controller,
          focusNode: _focus,
          style: style,
          cursorColor: c.accent,
          backgroundCursorColor: c.ink3,
          selectionColor: c.selection,
          autofocus: widget.autofocus,
          maxLines: widget.maxLines,
          minLines: widget.minLines,
          obscureText: widget.obscure,
          onChanged: widget.onChanged,
          onSubmitted: widget.onSubmitted,
          inputFormatters: widget.inputFormatters,
          textInputAction: widget.textInputAction,
          keyboardType: (widget.maxLines ?? 2) > 1 ? TextInputType.multiline : TextInputType.text,
          rendererIgnoresPointer: false,
          enableInteractiveSelection: true,
          selectionControls: null,
          mouseCursor: SystemMouseCursors.text,
        ),
      ),
    );
    final inner = Row(
      crossAxisAlignment: (widget.maxLines ?? 2) > 1
          ? CrossAxisAlignment.start
          : CrossAxisAlignment.center,
      children: [
        if (widget.leading != null) ...[widget.leading!, const SizedBox(width: Metrics.s3)],
        Expanded(
          child: Stack(
            children: [
              if (_controller.text.isEmpty && widget.placeholder != null)
                Positioned.fill(
                  child: IgnorePointer(
                    child: ExcludeSemantics(
                      child: Text(
                        widget.placeholder!,
                        style: style.copyWith(color: c.ink3),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                  ),
                ),
              field,
            ],
          ),
        ),
        if (widget.trailing != null) ...[const SizedBox(width: Metrics.s3), widget.trailing!],
      ],
    );
    // A press anywhere in the box focuses the field. A Listener, not a GestureDetector: the
    // editable's own tap recognizer wins the gesture arena over an ancestor's.
    Widget focusing(Widget child) => Listener(
      behavior: HitTestBehavior.opaque,
      onPointerDown: (_) {
        if (!_focus.hasFocus) _focus.requestFocus();
      },
      child: child,
    );
    // Tauri's field: a recessed well bordered by the input line; focus turns the border brass
    // and adds a soft 2 px brass ring. The composer (bezel) is the same field, roomier.
    final padding = widget.bezel
        ? const EdgeInsets.fromLTRB(10, 8, 6, 6)
        : EdgeInsets.symmetric(horizontal: 8, vertical: (widget.maxLines ?? 2) > 1 ? 6 : 4);
    return focusing(
      DecoratedBox(
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(Metrics.radiusControl + 2),
          border: Border.all(
            color: _focused ? c.accentSoft : c.accentSoft.withValues(alpha: 0),
            width: 2,
          ),
        ),
        child: Well(
          border: _focused ? c.accent : c.lineInput,
          child: Padding(padding: padding, child: inner),
        ),
      ),
    );
  }
}
