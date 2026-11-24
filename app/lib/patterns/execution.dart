import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/marks.dart';
import '../ui/overlay.dart';
import '../ui/press.dart';
import '../ui/spring.dart';
import '../ui/status.dart';
import '../ui/tokens.dart';

/// One part of how an agent is run. Model, harness, account, effort and access are separate because
/// they are separate (docs/architecture/overview.md, "Five things, never one").
class ExecutionChoice {
  const ExecutionChoice(this.label, this.items, this.value, this.onChanged, {this.cost});
  final String label, value;
  final List<AxMenuItem<String>> items;
  final ValueChanged<String> onChanged;

  /// What changing to a value costs, in words, if it costs anything: a change with a cost is asked
  /// about before it is made.
  final String? Function(String value)? cost;

  AxMenuItem<String>? get item => items.where((e) => e.value == value).firstOrNull;
}

/// The agent's execution as one key: what it runs on, and everything about it when opened. [choices]
/// begin with the model and the harness, which the key names.
class ExecutionKey extends StatelessWidget {
  const ExecutionKey(this.choices, {super.key});
  final List<ExecutionChoice> choices;

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final hue = choices.first.item?.mark?.color(c);
    final words = [for (final ch in choices.take(2)) ?ch.item?.label];
    return AxPopover(
      width: 372,
      anchor: (context, open, toggle) => Pressable(
        label: 'Runs on ${words.join(', ')}. Change',
        expanded: open,
        onPressed: toggle,
        builder: (context, s) => DecoratedBox(
          decoration: BoxDecoration(
            color: open || s.hovered ? c.tinted(hue) : null,
            borderRadius: BorderRadius.circular(AxRadius.key),
          ),
          child: ConstrainedBox(
            constraints: BoxConstraints(minHeight: t.density.control),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 8),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  if (choices.first.item?.mark case final m?) ...[
                    AxServiceMark(m, size: 13),
                    const SizedBox(width: 7),
                  ],
                  Flexible(
                    child: Text.rich(
                      TextSpan(
                        children: [
                          TextSpan(text: words.firstOrNull ?? 'Choose a model'),
                          if (words.length > 1)
                            TextSpan(
                              text: '   ${words[1]}',
                              style: TextStyle(color: c.ink3),
                            ),
                        ],
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: t.type.label.ink(c.ink1),
                    ),
                  ),
                  const SizedBox(width: 7),
                  SpringTo(
                    target: open ? 1 : 0,
                    spring: AxMotion.snap,
                    builder: (_, v, child) =>
                        Transform.rotate(angle: 3.1416 * (1 - v), child: child),
                    child: AxGlyph(AxIcon.down, size: 9, color: open ? c.ink1 : c.ink3),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
      panel: (context, close) => ExecutionPanel(choices),
    );
  }
}

/// Every part of the execution, one open at a time. A change that costs something is explained and
/// confirmed here.
class ExecutionPanel extends StatefulWidget {
  const ExecutionPanel(this.choices, {super.key});
  final List<ExecutionChoice> choices;
  @override
  State<ExecutionPanel> createState() => _ExecutionPanelState();
}

class _ExecutionPanelState extends State<ExecutionPanel> {
  String? _open;

  ({ExecutionChoice choice, AxMenuItem<String> item, String cost})? _asked;

  /// The keyboard stays in the panel: on a question when one is asked, and on the part it was
  /// about once it is answered.
  final _keep = FocusNode();
  late final _heads = {for (final c in widget.choices) c.label: FocusNode()};

  @override
  void dispose() {
    _keep.dispose();
    for (final n in _heads.values) {
      n.dispose();
    }
    super.dispose();
  }

  void _close(ExecutionChoice c) {
    setState(() {
      _open = null;
      _asked = null;
    });
    _heads[c.label]?.requestFocus();
  }

  void _apply(ExecutionChoice c, String v) {
    _close(c);
    c.onChanged(v);
  }

  void _choose(ExecutionChoice c, AxMenuItem<String> i) {
    if (i.value == c.value) return _close(c);
    final cost = c.cost?.call(i.value);
    if (cost == null) return _apply(c, i.value);
    setState(() => _asked = (choice: c, item: i, cost: cost));
    WidgetsBinding.instance.addPostFrameCallback((_) => mounted ? _keep.requestFocus() : null);
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.arrowDown): () =>
            FocusScope.of(context).nextFocus(),
        const SingleActivator(LogicalKeyboardKey.arrowUp): () =>
            FocusScope.of(context).previousFocus(),
      },
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final ch in widget.choices) ...[
            _row(
              context,
              label: '${ch.label}: ${ch.item?.label ?? 'none'}',
              expanded: _open == ch.label,
              node: _heads[ch.label],
              hue: ch.item?.mark?.color(c),
              onPressed: () => setState(() {
                _open = _open == ch.label ? null : ch.label;
                _asked = null;
              }),
              children: [
                SizedBox(width: 82, child: Text(ch.label, style: t.type.label.ink(c.ink3))),
                if (ch.item?.mark case final m?) ...[AxServiceMark(m), const SizedBox(width: 8)],
                Expanded(
                  child: Text(
                    ch.item?.label ?? 'None',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: t.type.ui,
                  ),
                ),
                SpringTo(
                  target: _open == ch.label ? 1 : 0,
                  spring: AxMotion.snap,
                  builder: (_, v, child) => Transform.rotate(angle: v * 1.5708, child: child),
                  child: AxGlyph(AxIcon.right, size: 9, color: c.ink4),
                ),
              ],
            ),
            if (_open == ch.label)
              Padding(
                padding: const EdgeInsets.only(left: 82, bottom: 5),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    for (final i in ch.items)
                      _row(
                        context,
                        label: i.label,
                        hint: i.enabled ? i.detail : i.why,
                        selected: i.value == ch.value,
                        held: _asked?.choice.label == ch.label && _asked?.item.value == i.value,
                        hue: i.mark?.color(c),
                        onPressed: i.enabled ? () => _choose(ch, i) : null,
                        children: [
                          SizedBox(
                            width: 18,
                            child: Align(
                              alignment: Alignment.centerLeft,
                              child: i.value == ch.value
                                  ? AxGlyph(
                                      AxIcon.check,
                                      size: 12,
                                      color: i.mark?.color(c) ?? c.liveText,
                                    )
                                  : null,
                            ),
                          ),
                          if (i.mark case final m?) ...[
                            AxServiceMark(m, color: i.enabled ? null : c.idle),
                            const SizedBox(width: 8),
                          ],
                          Expanded(
                            child: Text(
                              i.label,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: t.type.ui.ink(i.enabled ? c.ink1 : c.idle),
                            ),
                          ),
                          if ((i.enabled ? i.detail : i.why) case final d?) ...[
                            const SizedBox(width: 12),
                            Text(d, style: t.type.small.ink(c.ink3)),
                          ],
                        ],
                      ),
                    if (_asked case final a? when a.choice.label == ch.label)
                      _Cost(
                        words: a.cost,
                        keep: 'Keep ${ch.item?.label ?? 'as it is'}',
                        change: 'Change to ${a.item.label}',
                        node: _keep,
                        onKeep: () => _close(ch),
                        onChange: () => _apply(ch, a.item.value),
                      ),
                  ],
                ),
              ),
          ],
        ],
      ),
    );
  }

  Widget _row(
    BuildContext context, {
    required String label,
    required List<Widget> children,
    required VoidCallback? onPressed,
    String? hint,
    bool? expanded,
    bool? selected,
    Color? hue,
    FocusNode? node,
    bool held = false,
  }) {
    final t = Ax.of(context), c = t.colors;
    return Pressable(
      label: label,
      hint: hint,
      expanded: expanded,
      selected: selected,
      focusNode: node,
      onPressed: onPressed,
      builder: (_, s) => DecoratedBox(
        decoration: BoxDecoration(
          color: held || s.hovered || s.focused ? c.tinted(hue) : null,
          borderRadius: BorderRadius.circular(AxRadius.key),
        ),
        child: ConstrainedBox(
          constraints: BoxConstraints(minHeight: t.density.row + 1),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
            child: Row(children: children),
          ),
        ),
      ),
    );
  }
}

/// What a change costs, and the two answers.
class _Cost extends StatelessWidget {
  const _Cost({
    required this.words,
    required this.keep,
    required this.change,
    required this.node,
    required this.onKeep,
    required this.onChange,
  });
  final String words, keep, change;
  final FocusNode node;
  final VoidCallback onKeep, onChange;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Semantics(
      container: true,
      liveRegion: true,
      explicitChildNodes: true,
      child: Padding(
        padding: const EdgeInsets.only(top: 5),
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: c.hover,
            borderRadius: BorderRadius.circular(AxRadius.key),
          ),
          child: Padding(
            padding: const EdgeInsets.fromLTRB(10, 8, 10, 9),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Padding(
                      padding: const EdgeInsets.only(top: 5),
                      child: const AxLamp(AgentState.asking),
                    ),
                    const SizedBox(width: 9),
                    Expanded(child: Text(words, style: t.type.label.ink(c.ink1))),
                  ],
                ),
                const SizedBox(height: 8),
                Wrap(
                  spacing: 6,
                  runSpacing: 6,
                  alignment: WrapAlignment.end,
                  children: [
                    AxButton(label: keep, tone: AxTone.quiet, focusNode: node, onPressed: onKeep),
                    AxButton(label: change, tone: AxTone.primary, onPressed: onChange),
                  ],
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
