// A single- or multi-line text field in a recessed well, with a placeholder, a focus ring, its
// accessible name, the desktop's selection gestures and a Cut/Copy/Paste/Select All context menu
// (right-click; AUD-073), all on the widgets layer.

import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'controls.dart';
import 'overlays.dart';
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

  /// Prose, so always in the sans face (a mono field's placeholder too).
  final String? placeholder;
  final ValueChanged<String>? onChanged;
  final ValueChanged<String>? onSubmitted;
  final bool autofocus;
  final int? maxLines;
  final int? minLines;

  /// Code, commands and paths: the typed text is in the mono face.
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

class _AxTextInputState extends State<AxTextInput>
    implements TextSelectionGestureDetectorBuilderDelegate {
  TextEditingController? _ownController;
  FocusNode? _ownFocus;
  late TextEditingController _controller;
  late FocusNode _focus;
  bool _focused = false;
  bool _empty = true;
  ValueNotifier<String?>? _hintLine;

  @override
  final GlobalKey<EditableTextState> editableTextKey = GlobalKey<EditableTextState>();

  @override
  bool get forcePressEnabled => false;

  @override
  bool get selectionEnabled => true;

  late final _gestures = TextSelectionGestureDetectorBuilder(delegate: this);

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
    _empty = _controller.text.isEmpty;
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _hintLine = HintScope.maybeOf(context);
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
    final h = _hintLine;
    if (h == null) return;
    if (_focused) {
      h.value = widget.label;
    } else if (h.value == widget.label) {
      h.value = null;
    }
  }

  /// Only the placeholder depends on the text, and only on whether there is any: EditableText
  /// repaints the text itself, so a keystroke rebuilds nothing here (AUD-073).
  void _onText() {
    final empty = _controller.text.isEmpty;
    if (empty != _empty) setState(() => _empty = empty);
  }

  @override
  void dispose() {
    final h = _hintLine;
    if (h != null && _focused && h.value == widget.label) h.value = null;
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
        child: _gestures.buildGestureDetector(
          behavior: HitTestBehavior.translucent,
          child: EditableText(
            key: editableTextKey,
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
            // The gesture detector above handles clicks, drags and right-clicks.
            rendererIgnoresPointer: true,
            enableInteractiveSelection: true,
            selectionControls: null,
            contextMenuBuilder: (context, editable) => AxTextContextMenu(editable: editable),
            mouseCursor: SystemMouseCursors.text,
          ),
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
              if (_empty && widget.placeholder != null)
                Positioned.fill(
                  child: IgnorePointer(
                    child: ExcludeSemantics(
                      child: Text(
                        widget.placeholder!,
                        style: t.type.body.copyWith(fontSize: style.fontSize, color: c.ink3),
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
        ? const EdgeInsets.fromLTRB(Metrics.s4 + Metrics.s1, Metrics.s4, Metrics.s3, Metrics.s3)
        : EdgeInsets.symmetric(
            horizontal: Metrics.s4,
            vertical: (widget.maxLines ?? 2) > 1 ? Metrics.s3 : Metrics.s2,
          );
    return focusing(
      DecoratedBox(
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(Metrics.radiusHalo),
          border: Border.all(
            color: _focused ? c.accentSoft : c.accentSoft.withValues(alpha: 0),
            width: Metrics.focusWidth,
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

/// The context menu of a text field (and any [EditableText]): Cut, Copy, Paste and Select All
/// as the field offers them, at the pointer, kept inside the window. It never takes focus from
/// the field, so the selection it acts on stays.
class AxTextContextMenu extends StatelessWidget {
  const AxTextContextMenu({super.key, required this.editable});
  final EditableTextState editable;

  static String labelOf(ContextMenuButtonItem item) => switch (item.type) {
    ContextMenuButtonType.cut => 'Cut',
    ContextMenuButtonType.copy => 'Copy',
    ContextMenuButtonType.paste => 'Paste',
    ContextMenuButtonType.selectAll => 'Select All',
    ContextMenuButtonType.delete => 'Delete',
    ContextMenuButtonType.lookUp => 'Look Up',
    ContextMenuButtonType.searchWeb => 'Search the Web',
    ContextMenuButtonType.share => 'Share',
    ContextMenuButtonType.liveTextInput => 'Scan Text',
    ContextMenuButtonType.custom => item.label ?? '',
  };

  static String? shortcutOf(ContextMenuButtonType type) => switch (type) {
    ContextMenuButtonType.cut => 'Mod+X',
    ContextMenuButtonType.copy => 'Mod+C',
    ContextMenuButtonType.paste => 'Mod+V',
    ContextMenuButtonType.selectAll => 'Mod+A',
    _ => null,
  };

  @override
  Widget build(BuildContext context) {
    final items = editable.contextMenuButtonItems;
    if (items.isEmpty) return const SizedBox.shrink();
    final at = editable.contextMenuAnchors.primaryAnchor;
    // The overlay's origin is the window's: the anchor is global.
    final box = Overlay.maybeOf(context)?.context.findRenderObject() as RenderBox?;
    final local = box == null ? at : box.globalToLocal(at);
    return TextFieldTapRegion(
      child: AnchoredOverlay(
        anchor: Rect.fromLTWH(local.dx, local.dy, 0, 0),
        placement: const OverlayPlacement(gap: 0),
        child: Semantics(
          role: SemanticsRole.menu,
          label: 'Edit',
          container: true,
          child: ConstrainedBox(
            constraints: const BoxConstraints(minWidth: 180, maxWidth: 280),
            child: Sheet(
              radius: Metrics.radiusRaised,
              padding: const EdgeInsets.all(Metrics.s2),
              child: IntrinsicWidth(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [for (final item in items) _ContextItem(item: item)],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _ContextItem extends StatefulWidget {
  const _ContextItem({required this.item});
  final ContextMenuButtonItem item;

  @override
  State<_ContextItem> createState() => _ContextItemState();
}

class _ContextItemState extends State<_ContextItem> {
  // Clicking an item must not take focus from the field (that would hide the menu and the
  // selection it acts on).
  final _node = FocusNode(canRequestFocus: false, skipTraversal: true);

  @override
  void dispose() {
    _node.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final item = widget.item;
    final shortcut = AxTextContextMenu.shortcutOf(item.type);
    return Pressable(
      label: AxTextContextMenu.labelOf(item),
      role: PressRole.menuItem,
      focusNode: _node,
      onPressed: item.onPressed,
      builder: (context, s) => DecoratedBox(
        decoration: BoxDecoration(
          color: s.hovered ? c.hover : null,
          borderRadius: BorderRadius.circular(Metrics.radiusControl),
        ),
        child: SizedBox(
          height: context.scaled(Metrics.controlHeight),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: Metrics.s4),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    AxTextContextMenu.labelOf(item),
                    style: t.type.body.copyWith(color: s.enabled ? c.ink : c.ink3),
                  ),
                ),
                if (shortcut != null) ...[const SizedBox(width: Metrics.s6), Keycaps(shortcut)],
              ],
            ),
          ),
        ),
      ),
    );
  }
}
