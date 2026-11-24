import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'icons.dart';
import 'press.dart';
import 'spring.dart';
import 'surface.dart';
import 'tokens.dart';

enum AxTone { key, primary, answer, quiet }

/// A key: raised at rest, recessed while held or engaged.
class AxButton extends StatelessWidget {
  const AxButton({
    required this.label,
    required this.onPressed,
    super.key,
    this.icon,
    this.tone = AxTone.key,
    this.iconOnly = false,
    this.engaged = false,
    this.iconFilled = false,
    this.size,
    this.width,
    this.shortcut,
    this.autofocus = false,
  });
  const AxButton.icon(
    AxIcon this.icon, {
    required this.label,
    required this.onPressed,
    super.key,
    this.tone = AxTone.key,
    this.engaged = false,
    this.iconFilled = false,
    this.size,
    this.width,
    this.shortcut,
    this.autofocus = false,
  }) : iconOnly = true;
  final String label;
  final AxIcon? icon;
  final AxTone tone;
  final bool iconOnly, engaged, iconFilled, autofocus;
  final double? size, width;
  final String? shortcut;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final h = size ?? t.density.control;
    return Pressable(
      label: label,
      hint: shortcut,
      toggled: engaged ? true : null,
      onPressed: onPressed,
      autofocus: autofocus,
      builder: (context, s) {
        final ink = !s.enabled
            ? c.idle
            : switch (tone) {
                AxTone.primary => c.onPrimary,
                AxTone.answer => c.onAttention,
                AxTone.key => c.ink1,
                AxTone.quiet => s.hovered || engaged ? c.ink1 : c.ink2,
              };
        final spec = switch (tone) {
          AxTone.primary when !s.enabled => c.keyDown,
          AxTone.primary =>
            s.pressed
                ? c.primaryDown
                : s.hovered
                ? c.primaryHover
                : c.primary,
          AxTone.answer => s.pressed ? c.keyDown : c.answer,
          _ when !s.enabled => c.keyDown,
          _ =>
            s.pressed || engaged
                ? c.keyDown
                : s.hovered
                ? c.keyHover
                : c.key,
        };
        final row = Row(
          mainAxisSize: MainAxisSize.min,
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            if (icon != null) AxGlyph(icon!, size: 13, color: ink, filled: iconFilled),
            if (icon != null && !iconOnly) const SizedBox(width: 6),
            if (!iconOnly)
              Flexible(
                child: Text(
                  label,
                  maxLines: 1,
                  softWrap: false,
                  overflow: TextOverflow.ellipsis,
                  style: t.type.labelStrong.ink(ink),
                ),
              ),
          ],
        );
        final body = SizedBox(
          height: h,
          width: width ?? (iconOnly ? h + 4 : null),
          child: Padding(
            padding: EdgeInsets.symmetric(horizontal: iconOnly ? 0 : t.density.pad + 2),
            child: Transform.translate(
              offset: Offset(0, s.pressed ? .5 : 0),
              child: Center(widthFactor: 1, child: row),
            ),
          ),
        );
        if (tone == AxTone.quiet && !s.pressed && !engaged) {
          return DecoratedBox(
            decoration: BoxDecoration(
              color: s.hovered ? c.hover : null,
              borderRadius: BorderRadius.circular(AxRadius.key),
            ),
            child: body,
          );
        }
        return Surface(spec, radius: AxRadius.key, child: body);
      },
    );
  }
}

/// A set of keys where one stays down. Arrow keys move between them.
class AxSegments<T> extends StatelessWidget {
  const AxSegments({
    required this.label,
    required this.items,
    required this.value,
    required this.onChanged,
    super.key,
  });
  final String label;
  final List<(T, String, AxIcon?)> items;
  final T value;
  final ValueChanged<T> onChanged;

  void _step(int d) {
    final i = items.indexWhere((e) => e.$1 == value) + d;
    if (i >= 0 && i < items.length) onChanged(items[i].$1);
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context);
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: label,
      child: CallbackShortcuts(
        bindings: {
          const SingleActivator(LogicalKeyboardKey.arrowLeft): () => _step(-1),
          const SingleActivator(LogicalKeyboardKey.arrowRight): () => _step(1),
        },
        child: Surface(
          t.colors.well,
          radius: AxRadius.bay,
          child: Padding(
            padding: const EdgeInsets.all(3),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                for (final (i, (v, name, icon)) in items.indexed) ...[
                  if (i > 0) const SizedBox(width: 2),
                  Pressable(
                    label: name,
                    selected: v == value,
                    onPressed: () => onChanged(v),
                    builder: (context, s) {
                      final on = v == value;
                      final ink = on
                          ? t.colors.ink1
                          : s.hovered
                          ? t.colors.ink2
                          : t.colors.ink3;
                      final child = SizedBox(
                        height: t.density.control - 4,
                        child: Padding(
                          padding: EdgeInsets.symmetric(horizontal: icon == null ? 10 : 8),
                          child: Center(
                            widthFactor: 1,
                            child: icon == null
                                ? Text(name, style: t.type.labelStrong.ink(ink))
                                : AxGlyph(icon, size: 13, color: ink),
                          ),
                        ),
                      );
                      return on
                          ? Surface(t.colors.key, radius: AxRadius.key - 1, child: child)
                          : child;
                    },
                  ),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Tabs on a display. The mark under the active tab glides.
class AxTabs extends StatefulWidget {
  const AxTabs({required this.tabs, required this.index, required this.onChanged, super.key});
  final List<(String, String?)> tabs;
  final int index;
  final ValueChanged<int> onChanged;
  @override
  State<AxTabs> createState() => _AxTabsState();
}

class _AxTabsState extends State<AxTabs> {
  late List<GlobalKey> _keys = [for (final _ in widget.tabs) GlobalKey()];
  double _x = 0, _w = 0;
  @override
  void didUpdateWidget(AxTabs old) {
    super.didUpdateWidget(old);
    if (_keys.length != widget.tabs.length) _keys = [for (final _ in widget.tabs) GlobalKey()];
  }

  void _measure() {
    final box = context.findRenderObject() as RenderBox?;
    final tab = _keys[widget.index].currentContext?.findRenderObject() as RenderBox?;
    if (!mounted || box == null || tab == null) return;
    final x = tab.localToGlobal(Offset.zero, ancestor: box).dx;
    if (x != _x || tab.size.width != _w) {
      setState(() {
        _x = x;
        _w = tab.size.width;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    WidgetsBinding.instance.addPostFrameCallback((_) => _measure());
    final t = Ax.of(context);
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.arrowLeft): () =>
            widget.onChanged((widget.index - 1).clamp(0, widget.tabs.length - 1)),
        const SingleActivator(LogicalKeyboardKey.arrowRight): () =>
            widget.onChanged((widget.index + 1).clamp(0, widget.tabs.length - 1)),
      },
      child: Stack(
        children: [
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              for (final (i, (name, count)) in widget.tabs.indexed)
                KeyedSubtree(
                  key: _keys[i],
                  child: Pressable(
                    label: count == null ? name : '$name, $count',
                    selected: i == widget.index,
                    onPressed: () => widget.onChanged(i),
                    builder: (context, s) {
                      final on = i == widget.index;
                      return Padding(
                        padding: const EdgeInsets.symmetric(horizontal: 10),
                        child: SizedBox(
                          height: 36,
                          child: Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              // The strong width is always reserved: selecting a tab moves nothing.
                              Stack(
                                alignment: Alignment.center,
                                children: [
                                  Visibility.maintain(
                                    visible: false,
                                    child: Text(name, style: t.type.labelStrong),
                                  ),
                                  Text(
                                    name,
                                    style: (on ? t.type.labelStrong : t.type.label).ink(
                                      on
                                          ? t.colors.ink1
                                          : s.hovered
                                          ? t.colors.ink2
                                          : t.colors.ink3,
                                    ),
                                  ),
                                ],
                              ),
                              if (count != null) ...[
                                const SizedBox(width: 5),
                                Text(count, style: t.type.small.ink(t.colors.ink4)),
                              ],
                            ],
                          ),
                        ),
                      );
                    },
                  ),
                ),
            ],
          ),
          if (_w > 0)
            Positioned(
              left: 0,
              bottom: 0,
              child: SpringTo(
                target: _x + 10,
                builder: (_, x, child) => Transform.translate(offset: Offset(x, 0), child: child),
                child: SpringTo(
                  target: _w - 20,
                  builder: (_, w, _) => SizedBox(
                    width: w.clamp(0, 600),
                    height: 2,
                    child: ColoredBox(color: t.colors.live),
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

class AxSwitch extends StatelessWidget {
  const AxSwitch({required this.label, required this.value, required this.onChanged, super.key});
  final String label;
  final bool value;
  final ValueChanged<bool>? onChanged;
  @override
  Widget build(BuildContext context) {
    final c = Ax.of(context).colors;
    return Pressable(
      label: label,
      button: false,
      toggled: value,
      radius: 8,
      onPressed: onChanged == null ? null : () => onChanged!(!value),
      builder: (context, s) => SizedBox(
        width: 32,
        height: 18,
        child: Surface(
          value ? (s.enabled ? c.primaryDown : c.keyDown) : c.keyDown,
          radius: 6,
          child: SpringTo(
            target: value ? 1 : 0,
            spring: AxMotion.snap,
            builder: (_, v, child) => Align(alignment: Alignment(v * 2 - 1, 0), child: child),
            child: Padding(
              padding: const EdgeInsets.all(2),
              child: SizedBox(
                width: 14,
                child: Surface(
                  s.enabled ? c.keyHover : c.key,
                  radius: 4,
                  child: const SizedBox.expand(),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class AxCheck extends StatelessWidget {
  const AxCheck({required this.label, required this.value, required this.onChanged, super.key});
  final String label;
  final bool value;
  final ValueChanged<bool>? onChanged;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Semantics(
      checked: value,
      child: Pressable(
        label: label,
        button: false,
        onPressed: onChanged == null ? null : () => onChanged!(!value),
        builder: (context, s) => Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            SizedBox.square(
              dimension: 16,
              child: Surface(
                value && s.enabled ? c.primary : c.keyDown,
                radius: 3,
                child: value
                    ? Center(
                        child: AxGlyph(
                          AxIcon.check,
                          size: 12,
                          color: s.enabled ? c.onPrimary : c.idle,
                        ),
                      )
                    : null,
              ),
            ),
            const SizedBox(width: 8),
            Text(label, style: t.type.ui.ink(s.enabled ? c.ink2 : c.idle)),
          ],
        ),
      ),
    );
  }
}

/// A recessed text field.
class AxField extends StatefulWidget {
  const AxField({
    required this.label,
    super.key,
    this.controller,
    this.focusNode,
    this.placeholder = '',
    this.lines = 1,
    this.mono = false,
    this.error,
    this.enabled = true,
    this.leading,
    this.onSubmitted,
    this.onChanged,
  });
  final String label, placeholder;
  final String? error;
  final TextEditingController? controller;
  final FocusNode? focusNode;
  final int lines;
  final bool mono, enabled;
  final Widget? leading;
  final ValueChanged<String>? onSubmitted, onChanged;
  @override
  State<AxField> createState() => _AxFieldState();
}

class _AxFieldState extends State<AxField> {
  late final TextEditingController _c = widget.controller ?? TextEditingController();
  late final FocusNode _f = widget.focusNode ?? FocusNode();
  final _editable = GlobalKey<EditableTextState>();
  @override
  void initState() {
    super.initState();
    _f.addListener(_changed);
    _c.addListener(_changed);
  }

  void _changed() => setState(() {});
  @override
  void dispose() {
    _f.removeListener(_changed);
    _c.removeListener(_changed);
    if (widget.controller == null) _c.dispose();
    if (widget.focusNode == null) _f.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final style = (widget.mono ? t.type.code : t.type.ui).ink(widget.enabled ? c.ink1 : c.idle);
    final bad = widget.error != null;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        Semantics(
          label: widget.label,
          textField: true,
          enabled: widget.enabled,
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTapDown: (_) => widget.enabled ? _f.requestFocus() : null,
            child: Surface(
              widget.enabled ? c.bay : c.keyDown,
              radius: AxRadius.key,
              child: DecoratedBox(
                position: DecorationPosition.foreground,
                decoration: BoxDecoration(
                  borderRadius: BorderRadius.circular(AxRadius.key),
                  border: _f.hasFocus || bad
                      ? Border.all(color: bad ? c.fault : c.focus, width: 1.5)
                      : null,
                ),
                child: ConstrainedBox(
                  constraints: BoxConstraints(minHeight: t.density.control),
                  child: Padding(
                    padding: EdgeInsets.symmetric(
                      horizontal: t.density.pad + 1,
                      vertical: widget.lines > 1 ? 7 : 0,
                    ),
                    child: Row(
                      crossAxisAlignment: widget.lines > 1
                          ? CrossAxisAlignment.start
                          : CrossAxisAlignment.center,
                      children: [
                        if (widget.leading != null) ...[widget.leading!, const SizedBox(width: 7)],
                        Expanded(
                          child: Stack(
                            alignment: Alignment.topLeft,
                            children: [
                              if (_c.text.isEmpty)
                                IgnorePointer(
                                  child: ExcludeSemantics(
                                    child: Text(
                                      widget.placeholder,
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: style.ink(widget.enabled ? c.ink4 : c.idle),
                                    ),
                                  ),
                                ),
                              EditableText(
                                key: _editable,
                                controller: _c,
                                focusNode: _f,
                                readOnly: !widget.enabled,
                                style: style,
                                strutStyle: StrutStyle.fromTextStyle(style, forceStrutHeight: true),
                                cursorColor: c.liveText,
                                cursorWidth: 1.5,
                                backgroundCursorColor: c.idle,
                                selectionColor: c.selection,
                                maxLines: widget.lines == 1 ? 1 : null,
                                minLines: widget.lines,
                                onSubmitted: widget.onSubmitted,
                                onChanged: widget.onChanged,
                                selectionControls: emptyTextSelectionControls,
                                rendererIgnoresPointer: false,
                              ),
                            ],
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
        if (bad) ...[
          const SizedBox(height: 4),
          Row(
            children: [
              AxGlyph(AxIcon.warn, size: 12, color: c.faultText),
              const SizedBox(width: 5),
              Expanded(child: Text(widget.error!, style: t.type.small.ink(c.faultText))),
            ],
          ),
        ],
      ],
    );
  }
}

class AxKeycap extends StatelessWidget {
  const AxKeycap(this.keys, {super.key});
  final String keys;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context);
    return DecoratedBox(
      decoration: BoxDecoration(
        border: Border.all(color: t.colors.bayRule),
        borderRadius: BorderRadius.circular(3),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 4, vertical: .5),
        child: Text(keys, style: t.type.caption.ink(t.colors.ink3)),
      ),
    );
  }
}
