// A single- or multi-line text field in a recessed well, with a placeholder, a focus ring and its
// accessible name.

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'pressable.dart';
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

  /// The composer's look: an outer bezel ring around a lighter inner surface.
  final bool bezel;
  final bool obscure;
  final List<TextInputFormatter>? inputFormatters;
  final TextInputAction? textInputAction;

  @override
  State<AxTextInput> createState() => _AxTextInputState();
}

class _AxTextInputState extends State<AxTextInput> {
  late final TextEditingController _controller = widget.controller ?? TextEditingController();
  late final FocusNode _focus = widget.focusNode ?? FocusNode(debugLabel: widget.label);
  bool _focused = false;

  @override
  void initState() {
    super.initState();
    _focus.addListener(_onFocus);
    _controller.addListener(_onText);
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
    if (widget.controller == null) _controller.dispose();
    if (widget.focusNode == null) _focus.dispose();
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
    final ring = _focused ? c.focus : c.hairline;
    if (widget.bezel) {
      // An outer bezel ring around a slightly lighter inner surface (the Antigravity reference).
      return DecoratedBox(
        decoration: BoxDecoration(
          color: c.recessed,
          borderRadius: BorderRadius.circular(Metrics.radiusSheet),
          border: Border.all(color: _focused ? c.focus : c.hairline, width: _focused ? 1.5 : 1),
        ),
        child: Padding(
          padding: const EdgeInsets.all(3),
          child: DecoratedBox(
            decoration: BoxDecoration(
              color: c.raised,
              borderRadius: BorderRadius.circular(Metrics.radiusSheet - 3),
            ),
            child: Padding(padding: const EdgeInsets.fromLTRB(12, 10, 8, 8), child: inner),
          ),
        ),
      );
    }
    return GestureDetector(
      onTap: _focus.requestFocus,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: c.recessed,
          borderRadius: BorderRadius.circular(Metrics.radiusControl),
          border: Border.all(color: ring, width: _focused ? 1.5 : 1),
        ),
        child: Padding(
          padding: EdgeInsets.symmetric(
            horizontal: 8,
            vertical: (widget.maxLines ?? 2) > 1 ? 6 : 4,
          ),
          child: inner,
        ),
      ),
    );
  }
}
