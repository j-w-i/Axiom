import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../ui/controls.dart';
import '../ui/editable.dart';
import '../ui/icons.dart';
import '../ui/tokens.dart';

/// Writing sits on the display itself. Enter sends, Shift+Enter breaks the line.
class Composer extends StatefulWidget {
  const Composer({
    required this.placeholder,
    required this.action,
    required this.onSubmit,
    super.key,
    this.onAttach,
    this.enabled = true,
    this.leading = const [],
  });
  final String placeholder, action;

  /// Takes what was written. If it fails, the words are put back unless others have been written.
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
  final _text = TextEditingController();
  final _focus = FocusNode();
  @override
  void initState() {
    super.initState();
    _text.addListener(() => setState(() {}));
    _focus.addListener(() => setState(() {}));
  }

  @override
  void dispose() {
    _text.dispose();
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
    if (!taken && mounted && _text.text.isEmpty) _text.text = written;
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        ColoredBox(color: _focus.hasFocus ? c.live : c.bayRule, child: const SizedBox(height: 1)),
        Padding(
          padding: const EdgeInsets.fromLTRB(24, 13, 24, 4),
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
          padding: const EdgeInsets.fromLTRB(16, 2, 14, 10),
          child: Row(
            children: [
              Expanded(
                child: Wrap(
                  spacing: 6,
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
              const SizedBox(width: 6),
              AxButton(
                label: widget.action,
                icon: AxIcon.send,
                tone: AxTone.primary,
                onPressed: _text.text.trim().isEmpty || !widget.enabled
                    ? null
                    : () => unawaited(_submit()),
              ),
            ],
          ),
        ),
      ],
    );
  }
}
