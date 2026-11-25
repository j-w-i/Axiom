import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../ui/controls.dart';
import '../ui/editable.dart';
import '../ui/icons.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';

/// Where an agent is written to: one card, with how the agent runs under the words. Enter sends,
/// Shift+Enter breaks the line.
class Composer extends StatefulWidget {
  const Composer({
    required this.placeholder,
    required this.action,
    required this.onSubmit,
    super.key,
    this.controller,
    this.onAttach,
    this.enabled = true,
    this.leading = const [],
  });
  final String placeholder, action;

  /// What is being written, if it is kept elsewhere.
  final TextEditingController? controller;

  /// Takes what was written. If it fails, the words are put back, before what was written since.
  final FutureOr<bool> Function(String) onSubmit;

  /// Whether there is anybody to send it to; writing goes on either way.
  final bool enabled;
  final VoidCallback? onAttach;

  /// What stands under the words: how the agent runs, how full its context is.
  final List<Widget> leading;
  @override
  State<Composer> createState() => _ComposerState();
}

class _ComposerState extends State<Composer> {
  late final _text = widget.controller ?? TextEditingController();
  final _focus = FocusNode();
  @override
  void initState() {
    super.initState();
    _text.addListener(_changed);
    _focus.addListener(_changed);
  }

  void _changed() => setState(() {});

  @override
  void dispose() {
    _text.removeListener(_changed);
    if (widget.controller == null) _text.dispose();
    _focus.dispose();
    super.dispose();
  }

  /// Enter sends. While an input method is composing, Enter is its own and is left to it.
  KeyEventResult _key(FocusNode _, KeyEvent e) {
    final enter =
        e.logicalKey == LogicalKeyboardKey.enter || e.logicalKey == LogicalKeyboardKey.numpadEnter;
    final k = HardwareKeyboard.instance;
    if (!enter || e is! KeyDownEvent || k.isShiftPressed || k.isAltPressed) {
      return KeyEventResult.ignored;
    }
    final v = _text.value;
    if (v.composing.isValid && !v.composing.isCollapsed) return KeyEventResult.ignored;
    unawaited(_submit());
    return KeyEventResult.handled;
  }

  Future<void> _submit() async {
    final written = _text.text, s = written.trim();
    if (s.isEmpty || !widget.enabled) return;
    _text.clear();
    final taken = await widget.onSubmit(s);
    if (taken || !(mounted || widget.controller != null)) return;
    final since = _text.text;
    final all = since.trim().isEmpty ? written : '$written\n\n$since';
    _text.value = TextEditingValue(
      text: all,
      selection: TextSelection.collapsed(offset: all.length),
    );
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Surface(
      c.written,
      radius: AxRadius.card,
      ring: _focus.hasFocus ? c.writtenFocus : null,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(17, 14, 17, 6),
            child: Focus(
              canRequestFocus: false,
              skipTraversal: true,
              onKeyEvent: _key,
              child: GestureDetector(
                behavior: HitTestBehavior.opaque,
                onTap: _focus.requestFocus,
                child: ConstrainedBox(
                  constraints: const BoxConstraints(minHeight: 25, maxHeight: 220),
                  child: AxEditable(
                    label: widget.placeholder,
                    placeholder: widget.placeholder,
                    controller: _text,
                    focusNode: _focus,
                    style: t.type.prose,
                  ),
                ),
              ),
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(9, 2, 11, 11),
            child: Row(
              children: [
                Expanded(
                  child: Wrap(
                    spacing: 2,
                    runSpacing: 2,
                    crossAxisAlignment: WrapCrossAlignment.center,
                    children: widget.leading,
                  ),
                ),
                const SizedBox(width: 12),
                if (widget.onAttach != null)
                  AxButton.icon(
                    AxIcon.attach,
                    label: 'Attach',
                    tone: AxTone.quiet,
                    onPressed: widget.onAttach,
                  ),
                const SizedBox(width: 4),
                AxButton.icon(
                  AxIcon.up,
                  label: widget.action,
                  tone: AxTone.primary,
                  round: true,
                  size: 30,
                  onPressed: _text.text.trim().isEmpty || !widget.enabled
                      ? null
                      : () => unawaited(_submit()),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
