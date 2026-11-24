import 'package:flutter/widgets.dart';

import 'press.dart';
import 'surface.dart';
import 'tokens.dart';

/// Text editing as people expect it: drag and double-click select, right click offers the clipboard,
/// input methods work. Fields and the composer are this with a surface around it.
class AxEditable extends StatefulWidget {
  const AxEditable({
    required this.label,
    required this.controller,
    required this.focusNode,
    required this.style,
    super.key,
    this.placeholder = '',
    this.enabled = true,
    this.maxLines,
    this.minLines,
    this.onSubmitted,
    this.onChanged,
  });
  final String label, placeholder;
  final TextEditingController controller;
  final FocusNode focusNode;
  final TextStyle style;
  final bool enabled;
  final int? maxLines, minLines;
  final ValueChanged<String>? onSubmitted, onChanged;
  @override
  State<AxEditable> createState() => _AxEditableState();
}

class _AxEditableState extends State<AxEditable>
    implements TextSelectionGestureDetectorBuilderDelegate {
  @override
  final editableTextKey = GlobalKey<EditableTextState>();
  @override
  bool get forcePressEnabled => false;
  @override
  bool get selectionEnabled => widget.enabled;
  late final _gestures = TextSelectionGestureDetectorBuilder(delegate: this);

  @override
  Widget build(BuildContext context) {
    final c = Ax.of(context).colors;
    final editable = EditableText(
      key: editableTextKey,
      controller: widget.controller,
      focusNode: widget.focusNode,
      readOnly: !widget.enabled,
      style: widget.style,
      strutStyle: StrutStyle.fromTextStyle(widget.style, forceStrutHeight: true),
      cursorColor: c.liveText,
      cursorWidth: 1.5,
      cursorOpacityAnimates: false,
      backgroundCursorColor: c.idle,
      selectionColor: c.selection,
      maxLines: widget.maxLines,
      minLines: widget.minLines,
      onSubmitted: widget.onSubmitted,
      onChanged: widget.onChanged,
      selectionControls: emptyTextSelectionControls,
      rendererIgnoresPointer: true,
      mouseCursor: SystemMouseCursors.text,
      showSelectionHandles: false,
      contextMenuBuilder: (context, s) =>
          AxContextMenu(anchors: s.contextMenuAnchors, items: s.contextMenuButtonItems),
    );
    return MergeSemantics(
      child: Semantics(
        label: widget.label,
        enabled: widget.enabled,
        child: Stack(
          children: [
            ValueListenableBuilder(
              valueListenable: widget.controller,
              builder: (_, v, _) => v.text.isNotEmpty
                  ? const SizedBox.shrink()
                  : IgnorePointer(
                      child: ExcludeSemantics(
                        child: Text(
                          widget.placeholder,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: widget.style.ink(widget.enabled ? c.ink4 : c.idle),
                        ),
                      ),
                    ),
            ),
            MouseRegion(
              cursor: widget.enabled ? SystemMouseCursors.text : SystemMouseCursors.basic,
              child: _gestures.buildGestureDetector(
                behavior: HitTestBehavior.translucent,
                child: editable,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Cut, copy, paste, select all: as our own menu.
class AxContextMenu extends StatelessWidget {
  const AxContextMenu({required this.anchors, required this.items, super.key});
  final TextSelectionToolbarAnchors anchors;
  final List<ContextMenuButtonItem> items;

  static String _word(ContextMenuButtonItem i) => switch (i.type) {
    ContextMenuButtonType.cut => 'Cut',
    ContextMenuButtonType.copy => 'Copy',
    ContextMenuButtonType.paste => 'Paste',
    ContextMenuButtonType.selectAll => 'Select all',
    ContextMenuButtonType.delete => 'Delete',
    _ => i.label ?? '',
  };

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final known = items.where((i) => _word(i).isNotEmpty).toList();
    if (known.isEmpty) return const SizedBox.shrink();
    return CustomSingleChildLayout(
      delegate: _At(anchors.primaryAnchor),
      child: Surface(
        c.overlay,
        radius: AxRadius.overlay,
        child: Padding(
          padding: const EdgeInsets.all(5),
          child: IntrinsicWidth(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                for (final i in known)
                  Pressable(
                    label: _word(i),
                    onPressed: i.onPressed,
                    builder: (_, s) => DecoratedBox(
                      decoration: BoxDecoration(
                        color: s.hovered || s.focused ? c.hover : null,
                        borderRadius: BorderRadius.circular(AxRadius.key),
                      ),
                      child: Padding(
                        padding: const EdgeInsets.fromLTRB(10, 4, 28, 4),
                        child: Text(_word(i), style: t.type.ui),
                      ),
                    ),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _At extends SingleChildLayoutDelegate {
  const _At(this.at);
  final Offset at;
  @override
  BoxConstraints getConstraintsForChild(BoxConstraints c) => c.loosen();
  @override
  Offset getPositionForChild(Size s, Size child) => Offset(
    at.dx.clamp(4, (s.width - child.width - 4).clamp(4, double.infinity)),
    (at.dy + 2).clamp(4, (s.height - child.height - 4).clamp(4, double.infinity)),
  );
  @override
  bool shouldRelayout(_At o) => o.at != at;
}
