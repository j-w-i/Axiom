import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../ui/controls.dart';
import '../ui/editable.dart';
import '../ui/icons.dart';
import '../ui/overlay.dart';
import '../ui/press.dart';
import '../ui/tokens.dart';

/// One choice on the execution line. Harness, model, connection, effort and access are separate
/// because they are separate (docs/architecture/overview.md, "Five things, never one").
class ExecutionChoice {
  const ExecutionChoice(this.label, this.items, this.value, this.onChanged);
  final String label, value;
  final List<AxMenuItem<String>> items;
  final ValueChanged<String> onChanged;
}

/// The agent's execution, beside its name: these belong to the agent, not to one message.
class ExecutionLine extends StatelessWidget {
  const ExecutionLine(this.choices, {super.key});
  final List<ExecutionChoice> choices;
  @override
  Widget build(BuildContext context) => Wrap(children: [for (final ch in choices) _Choice(ch)]);
}

/// Writing sits on the display itself. Enter sends, Shift+Enter breaks the line.
class Composer extends StatefulWidget {
  const Composer({
    required this.placeholder,
    required this.action,
    required this.onSubmit,
    super.key,
    this.onAttach,
  });
  final String placeholder, action;
  final ValueChanged<String> onSubmit;
  final VoidCallback? onAttach;
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
    _submit();
    return KeyEventResult.handled;
  }

  void _submit() {
    final s = _text.text.trim();
    if (s.isEmpty) return;
    widget.onSubmit(s);
    _text.clear();
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
          padding: const EdgeInsets.fromLTRB(24, 10, 14, 10),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              Expanded(
                child: Focus(
                  canRequestFocus: false,
                  skipTraversal: true,
                  onKeyEvent: _key,
                  child: GestureDetector(
                    behavior: HitTestBehavior.opaque,
                    onTap: _focus.requestFocus,
                    child: Padding(
                      padding: const EdgeInsets.symmetric(vertical: 3),
                      child: ConstrainedBox(
                        constraints: const BoxConstraints(minHeight: 23, maxHeight: 220),
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
                onPressed: _text.text.trim().isEmpty ? null : _submit,
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _Choice extends StatelessWidget {
  const _Choice(this.ch);
  final ExecutionChoice ch;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final label = ch.items.where((e) => e.value == ch.value).firstOrNull?.label ?? ch.value;
    return AxPopover(
      anchor: (context, open, toggle) => Pressable(
        label: '${ch.label}: $label',
        expanded: open,
        onPressed: toggle,
        builder: (context, s) => DecoratedBox(
          decoration: BoxDecoration(
            color: open || s.hovered ? c.hover : null,
            borderRadius: BorderRadius.circular(AxRadius.key),
          ),
          child: SizedBox(
            height: t.density.control,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 7),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(label, style: t.type.label.ink(open || s.hovered ? c.ink1 : c.ink2)),
                  const SizedBox(width: 5),
                  AxGlyph(AxIcon.down, size: 9, color: open || s.hovered ? c.ink2 : c.ink4),
                ],
              ),
            ),
          ),
        ),
      ),
      panel: (context, close) => AxMenu<String>(
        label: ch.label,
        items: ch.items,
        value: ch.value,
        onChosen: (v) {
          close();
          ch.onChanged(v);
        },
      ),
    );
  }
}
