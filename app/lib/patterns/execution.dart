import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../ui/commands.dart';
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
  const ExecutionChoice(
    this.label,
    this.items,
    this.value,
    this.onChanged, {
    this.cost,
    this.write,
    this.written,
  });

  /// Takes one that is not among them, as it is written; [written]: the one that was.
  final ValueChanged<String>? write;
  final AxMenuItem<String>? written;
  final String label, value;
  final List<AxMenuItem<String>> items;
  final ValueChanged<String> onChanged;

  /// What changing to a value costs, in words, if it costs anything: a change with a cost is asked
  /// about before it is made.
  final String? Function(String value)? cost;

  AxMenuItem<String>? get item => items.where((e) => e.value == value).firstOrNull ?? written;
}

/// The agent's execution as one key: what it runs on, and everything about it when opened. [choices]
/// begin with the model and the harness, which the key names.
class ExecutionKey extends StatelessWidget {
  const ExecutionKey(
    this.choices, {
    required this.of,
    super.key,
    this.onOpen,
    this.words,
    this.mark,
    this.panel,
    this.width = 440,
  });
  final List<ExecutionChoice> choices;

  /// What it says, and what it opens, in place of [choices].
  final List<String>? words;
  final ServiceMark? mark;
  final Widget Function(BuildContext, VoidCallback close)? panel;
  final double width;

  /// When somebody is about to look.
  final VoidCallback? onOpen;

  /// Whose execution it is. The panel of one agent closes when the key becomes another's.
  final Object of;

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final mark = panel == null ? choices.first.item?.mark : this.mark;
    final words =
        this.words ??
        [
          choices.first.item?.label ?? 'Choose a ${choices.first.label.toLowerCase()}',
          for (final ch in choices.skip(1).take(1)) ?ch.item?.label,
        ];
    return AxPopover(
      key: ValueKey(of),
      width: width,
      anchor: (context, open, toggle) => OnCommand(
        Command.model,
        () {
          if (!open) onOpen?.call();
          toggle();
        },
        child: Pressable(
          label: 'Runs on ${words.join(', ')}. Change',
          expanded: open,
          onPressed: () {
            if (!open) onOpen?.call();
            toggle();
          },
          builder: (context, s) => DecoratedBox(
            decoration: BoxDecoration(
              color: open
                  ? c.selected
                  : s.hovered
                  ? c.hover
                  : null,
              borderRadius: BorderRadius.circular(AxRadius.key),
            ),
            child: ConstrainedBox(
              constraints: BoxConstraints(minHeight: t.density.control),
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 8),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    if (mark case final m?) ...[
                      AxServiceMark(m, size: 13),
                      const SizedBox(width: 7),
                    ],
                    Flexible(
                      child: Text.rich(
                        TextSpan(
                          children: [
                            TextSpan(
                              text: words.first,
                              style: TextStyle(fontVariations: t.type.labelStrong.fontVariations),
                            ),
                            if (words.length > 1)
                              TextSpan(
                                text: '   ${words.skip(1).join('   ')}',
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
                      builder: (_, v, child) => Transform.rotate(angle: 3.1416 * v, child: child),
                      child: AxGlyph(AxIcon.down, color: open ? c.ink1 : c.ink3),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
      panel: panel ?? (context, close) => ExecutionPanel(choices),
    );
  }
}

/// A change that costs something, while it is asked about.
class _Question {
  const _Question(this.label, this.from, this.to, this.keep, this.change, this.cost);
  final String label, from, to, keep, change, cost;
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
  /// What has to be chosen first is open.
  late String? _open = widget.choices.first.item == null ? widget.choices.first.label : null;
  _Question? _asked;

  /// The keyboard stays in the panel: on a question when one is asked, and on the part it was
  /// about once it is answered.
  final _keep = FocusNode();
  final _heads = <String, FocusNode>{};
  final _question = GlobalKey();

  FocusNode _head(String label) => _heads.putIfAbsent(label, FocusNode.new);

  @override
  void dispose() {
    _keep.dispose();
    for (final n in _heads.values) {
      n.dispose();
    }
    super.dispose();
  }

  ExecutionChoice? _choice(String label) =>
      widget.choices.where((c) => c.label == label).firstOrNull;

  /// A question is about one change, from what is to what would be, at one cost. If any of it is
  /// not so any more, it is not asked any more.
  bool _stands(_Question q) {
    final c = _choice(q.label);
    final to = c?.items.where((i) => i.value == q.to).firstOrNull;
    return c != null &&
        to != null &&
        to.enabled &&
        c.value == q.from &&
        _name(c) == q.keep &&
        to.label == q.change &&
        c.cost?.call(q.to) == q.cost;
  }

  static String _name(ExecutionChoice c) => c.item?.label ?? 'as it is';

  @override
  void didUpdateWidget(ExecutionPanel old) {
    super.didUpdateWidget(old);
    if (_asked case final q? when !_stands(q)) _asked = null;
    if (_open case final o? when _choice(o) == null) _open = null;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      _heads.removeWhere((label, node) {
        if (_choice(label) != null) return false;
        node.dispose();
        return true;
      });
      final scope = FocusScope.of(context);
      // What had the focus has gone, and an open panel keeps it.
      if (scope.focusedChild == null && widget.choices.isNotEmpty) {
        _head((_open == null ? null : _choice(_open!)?.label) ?? widget.choices.first.label)
            .requestFocus();
      }
    });
  }

  void _close(ExecutionChoice c) {
    setState(() {
      _open = null;
      _asked = null;
    });
    _head(c.label).requestFocus();
  }

  void _apply(ExecutionChoice c, String v) {
    _close(c);
    c.onChanged(v);
  }

  void _choose(ExecutionChoice c, AxMenuItem<String> i) {
    if (i.value == c.value) return _close(c);
    final cost = c.cost?.call(i.value);
    if (cost == null) return _apply(c, i.value);
    setState(() => _asked = _Question(c.label, c.value, i.value, _name(c), i.label, cost));
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || _asked == null) return;
      _keep.requestFocus();
      if (_question.currentContext case final q?) {
        unawaited(
          Scrollable.ensureVisible(
            q,
            alignmentPolicy: ScrollPositionAlignmentPolicy.keepVisibleAtEnd,
          ),
        );
      }
    });
  }

  void _answer(_Question q) {
    final c = _choice(q.label);
    if (c == null || !_stands(q)) return setState(() => _asked = null);
    _apply(c, q.to);
  }

  @override
  Widget build(BuildContext context) => Settle(
    stamp: (_open, _asked),
    child: CallbackShortcuts(
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
          for (final ch in widget.choices)
            KeyedSubtree(key: ValueKey(ch.label), child: _part(context, ch)),
        ],
      ),
    ),
  );

  Widget _part(BuildContext context, ExecutionChoice ch) {
    final t = Ax.of(context), c = t.colors;
    final open = _open == ch.label;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _row(
          context,
          label: '${ch.label}: ${ch.item?.label ?? 'none'}',
          expanded: open,
          node: _head(ch.label),
          hue: ch.item?.mark?.color(c),
          onPressed: () => setState(() {
            _open = open ? null : ch.label;
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
              target: open ? 1 : 0,
              spring: AxMotion.snap,
              builder: (_, v, child) => Transform.rotate(angle: v * 1.5708, child: child),
              child: AxGlyph(AxIcon.right, color: c.ink3),
            ),
          ],
        ),
        if (open)
          Padding(
            padding: const EdgeInsets.only(left: 82, bottom: 5),
            child: AxNarrow<String>(
              items: ch.items,
              value: ch.value,
              builder: (context, shown) => Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  for (final i in shown) ..._item(context, ch, i),
                  if (ch.write case final f?) _Write(ch, onChanged: f, onDone: () => _close(ch)),
                ],
              ),
            ),
          ),
      ],
    );
  }

  List<Widget> _item(BuildContext context, ExecutionChoice ch, AxMenuItem<String> i) {
    final t = Ax.of(context), c = t.colors;
    final q = _asked != null && _asked!.label == ch.label && _asked!.to == i.value ? _asked : null;
    final detail = i.enabled ? i.detail : i.why;
    return [
      _row(
        context,
        label: i.label,
        hint: detail,
        selected: i.value == ch.value,
        held: q != null,
        hue: i.mark?.color(c),
        onPressed: i.enabled ? () => _choose(ch, i) : null,
        children: [
          SizedBox(
            width: 18,
            child: Align(
              alignment: Alignment.centerLeft,
              child: i.value == ch.value
                  ? AxGlyph(AxIcon.check, color: i.mark?.color(c) ?? c.liveText)
                  : null,
            ),
          ),
          if (i.mark case final m?) ...[
            AxServiceMark(m, color: i.enabled ? null : c.idle),
            const SizedBox(width: 8),
          ],
          Expanded(
            flex: 3,
            child: Text(
              i.label,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: t.type.ui.ink(i.enabled ? c.ink1 : c.idle),
            ),
          ),
          if (detail != null) ...[
            const SizedBox(width: 12),
            Expanded(
              flex: 2,
              child: Text(
                detail,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                textAlign: TextAlign.right,
                style: t.type.small.ink(c.ink3),
              ),
            ),
          ],
        ],
      ),
      if (q != null)
        _Cost(
          key: _question,
          words: q.cost,
          keep: 'Keep ${q.keep}',
          change: 'Change to ${q.change}',
          node: _keep,
          onKeep: () => _close(ch),
          onChange: () => _answer(q),
        ),
    ];
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
            padding: const EdgeInsets.symmetric(horizontal: AxSpace.row, vertical: 3),
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
    super.key,
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
      label: words,
      child: Padding(
        padding: const EdgeInsets.only(top: 5),
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: c.hover,
            borderRadius: BorderRadius.circular(AxRadius.key),
          ),
          child: Padding(
            padding: const EdgeInsets.fromLTRB(AxSpace.row, 9, AxSpace.row, 10),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Padding(
                      padding: const EdgeInsets.only(top: 5),
                      child: const ExcludeSemantics(child: AxLamp(AgentState.asking)),
                    ),
                    const SizedBox(width: 9),
                    Expanded(
                      child: ExcludeSemantics(child: Text(words, style: t.type.label.ink(c.ink1))),
                    ),
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

/// One that is not listed, by the name it has where it is.
class _Write extends StatefulWidget {
  const _Write(this.choice, {required this.onChanged, required this.onDone});
  final ExecutionChoice choice;
  final ValueChanged<String> onChanged;
  final VoidCallback onDone;
  @override
  State<_Write> createState() => _WriteState();
}

class _WriteState extends State<_Write> {
  late final _text = TextEditingController(text: widget.choice.written?.value);
  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(0, 4, 0, 2),
    child: AxField(
      label: '${widget.choice.label} name',
      controller: _text,
      mono: true,
      placeholder: widget.choice.items.isEmpty ? 'As the provider calls it' : 'Another, by name',
      autofocus: widget.choice.items.isEmpty,
      onChanged: (v) => widget.onChanged(v.trim()),
      onSubmitted: (_) => widget.onDone(),
    ),
  );
}
