import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../ui/controls.dart';
import '../ui/editable.dart';
import '../ui/icons.dart';
import '../ui/press.dart';
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
    this.onStop,
    this.enabled = true,
    this.leading = const [],
    this.label,
    this.header,
    this.ready,
    this.autofocus = false,
    this.high = 25,
    this.earlier,
  });

  /// What it is heard as, if not by its placeholder.
  final String? label;

  /// What was said before, the last first: ↑ in an empty box brings it back.
  final List<String> Function()? earlier;

  /// What stands over the words.
  final Widget? header;

  /// Whether it can be sent, where that is not only whether something is written: then what is
  /// written stays until its taker clears it.
  final bool? ready;
  final bool autofocus;

  /// How high it is with nothing in it.
  final double high;
  final String placeholder, action;

  /// What is being written, if it is kept elsewhere.
  final TextEditingController? controller;

  /// Takes what was written. If it fails, the words are put back, before what was written since.
  final FutureOr<bool> Function(String) onSubmit;

  /// Whether there is anybody to send it to; writing goes on either way.
  final bool enabled;
  final VoidCallback? onAttach;

  /// Ends what is being answered. While nothing is written it stands where sending does, and beside
  /// that otherwise.
  final VoidCallback? onStop;

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
    if (widget.autofocus) {
      WidgetsBinding.instance.addPostFrameCallback((_) => mounted ? _focus.requestFocus() : null);
    }
  }

  void _changed() => setState(() {});

  /// What ↑ has brought back, and how it stood then. Whoever has touched it since is writing.
  List<String> _before = const [];
  int _back = -1;
  TextEditingValue? _put;
  bool get _recalling => _back >= 0 && _put == _text.value;

  /// Walks through what was said before. Only from an empty box, or from what was brought back and
  /// not touched: nothing that was written is ever replaced, and the caret moves as ever.
  bool _recall(int by) {
    final recalling = _recalling;
    if (!recalling) {
      if (by < 0 || _text.text.isNotEmpty) return false;
      _before = widget.earlier?.call() ?? const [];
      _back = -1;
    }
    final to = _back + by;
    if (to >= _before.length) return recalling;
    _back = to;
    final s = to < 0 ? '' : _before[to];
    final v = TextEditingValue(
      text: s,
      selection: TextSelection.collapsed(offset: s.length),
    );
    _put = v;
    _text.value = v;
    return true;
  }

  @override
  void dispose() {
    _text.removeListener(_changed);
    if (widget.controller == null) _text.dispose();
    _focus.dispose();
    super.dispose();
  }

  /// Enter sends, ↑ and ↓ go through what was said before. While an input method is composing, they
  /// are its own and are left to it.
  KeyEventResult _key(FocusNode _, KeyEvent e) {
    final enter =
        e.logicalKey == LogicalKeyboardKey.enter || e.logicalKey == LogicalKeyboardKey.numpadEnter;
    final k = HardwareKeyboard.instance;
    final up = e.logicalKey == LogicalKeyboardKey.arrowUp;
    if (up || e.logicalKey == LogicalKeyboardKey.arrowDown) {
      final plain = !k.isShiftPressed && !k.isAltPressed && !k.isControlPressed && !k.isMetaPressed;
      final composing = _text.value.composing;
      if (e is KeyUpEvent || !plain || (composing.isValid && !composing.isCollapsed)) {
        return KeyEventResult.ignored;
      }
      return _recall(up ? 1 : -1) ? KeyEventResult.handled : KeyEventResult.ignored;
    }
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
    if (!widget.enabled || !(widget.ready ?? s.isNotEmpty)) return;
    if (widget.ready != null) {
      await widget.onSubmit(s);
      return;
    }
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
    final empty = _text.text.trim().isEmpty;
    return Surface(
      c.written,
      radius: AxRadius.written,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          ?widget.header,
          Padding(
            padding: const EdgeInsets.fromLTRB(AxSpace.edge, 13, AxSpace.edge, 6),
            child: Focus(
              canRequestFocus: false,
              skipTraversal: true,
              onKeyEvent: _key,
              child: GestureDetector(
                behavior: HitTestBehavior.opaque,
                onTap: _focus.requestFocus,
                child: ConstrainedBox(
                  constraints: BoxConstraints(minHeight: widget.high, maxHeight: 220),
                  child: AxEditable(
                    label: widget.label ?? widget.placeholder,
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
            padding: const EdgeInsets.fromLTRB(AxSpace.chip, 2, 10, 10),
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
                // A click meant to send does not stop, nor the other way round.
                Settle(
                  stamp: (widget.onStop == null, empty),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      if (widget.onStop case final stop?) ...[
                        AxButton.icon(
                          AxIcon.stop,
                          key: const ValueKey('stop'),
                          label: 'Stop',
                          iconFilled: true,
                          tone: AxTone.key,
                          round: true,
                          size: 30,
                          onPressed: stop,
                        ),
                        if (!empty) const SizedBox(width: 6),
                      ],
                      if (!empty || widget.onStop == null)
                        AxButton.icon(
                          AxIcon.up,
                          key: const ValueKey('send'),
                          label: widget.action,
                          tone: AxTone.primary,
                          round: true,
                          size: 30,
                          onPressed: (widget.ready ?? !empty) && widget.enabled
                              ? () => unawaited(_submit())
                              : null,
                        ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
