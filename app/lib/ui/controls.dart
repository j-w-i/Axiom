import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'editable.dart';
import 'icons.dart';
import 'press.dart';
import 'spring.dart';
import 'surface.dart';
import 'tokens.dart';

enum AxTone { key, primary, quiet }

/// A key: the main act, any other, or one that shows its ground when the hand is there.
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
    this.focusNode,
  }) : round = false;
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
    this.focusNode,
    this.round = false,
  }) : iconOnly = true;

  /// The one act of a surface that is written into.
  final bool round;
  final FocusNode? focusNode;
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
      focusNode: focusNode,
      autofocus: autofocus,
      radius: round ? h / 2 : AxRadius.key,
      builder: (context, s) {
        final ink = !s.enabled
            ? c.idle
            : switch (tone) {
                AxTone.primary => c.onPrimary,
                AxTone.key => c.ink1,
                AxTone.quiet => s.hovered || engaged ? c.ink1 : c.ink2,
              };
        final spec = switch (tone) {
          AxTone.primary =>
            s.pressed
                ? c.primaryDown
                : s.hovered
                ? c.primaryHover
                : c.primary,
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
            if (icon != null)
              AxGlyph(icon!, size: iconOnly ? 17 : 15, color: ink, filled: iconFilled),
            if (icon != null && !iconOnly) const SizedBox(width: 7),
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
          width: width ?? (iconOnly ? h : null),
          child: Padding(
            padding: EdgeInsets.symmetric(horizontal: iconOnly ? 0 : t.density.pad + 1),
            child: Center(widthFactor: 1, child: row),
          ),
        );
        if (!s.enabled) {
          // A main act keeps its place while it cannot be pressed.
          return tone == AxTone.quiet
              ? body
              : round
              ? Ground(c.selected, radius: h / 2, child: body)
              : DecoratedBox(
                  decoration: BoxDecoration(
                    border: Border.all(color: c.meterTrack),
                    borderRadius: BorderRadius.circular(round ? h / 2 : AxRadius.key),
                  ),
                  child: body,
                );
        }
        if (tone == AxTone.quiet) {
          return Ground(
            s.pressed || engaged
                ? c.selected
                : s.hovered
                ? c.hover
                : null,
            child: body,
          );
        }
        // It gives a little under the hand.
        return SpringTo(
          target: s.pressed ? 1 : 0,
          spring: AxMotion.snap,
          builder: (_, v, child) => Transform.scale(scale: 1 - .03 * v, child: child),
          child: Surface(spec, radius: round ? h / 2 : AxRadius.key, child: body),
        );
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
        child: Trough(
          index: items.indexWhere((e) => e.$1 == value),
          children: [
            for (final (v, name, icon) in items)
              Pressable(
                label: name,
                selected: v == value,
                onPressed: () => onChanged(v),
                builder: (context, s) {
                  final ink = v == value
                      ? t.colors.ink1
                      : s.hovered
                      ? t.colors.ink2
                      : t.colors.ink3;
                  return SizedBox(
                    height: t.density.control - 4,
                    child: Padding(
                      padding: EdgeInsets.symmetric(horizontal: icon == null ? 11 : 8),
                      child: Center(
                        widthFactor: 1,
                        child: icon == null
                            ? Text(name, style: t.type.labelStrong.ink(ink))
                            : AxGlyph(icon, size: 14, color: ink),
                      ),
                    ),
                  );
                },
              ),
          ],
        ),
      ),
    );
  }
}

/// Tabs: a trough, and a raised thumb under the one that is chosen. Choosing moves nothing else.
class AxTabs extends StatelessWidget {
  const AxTabs({required this.tabs, required this.index, required this.onChanged, super.key});

  final List<(String, String?)> tabs;
  final int index;
  final ValueChanged<int> onChanged;

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context);
    void to(int i) => onChanged(i.clamp(0, tabs.length - 1));
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.arrowLeft): () => to(index - 1),
        const SingleActivator(LogicalKeyboardKey.arrowRight): () => to(index + 1),
      },
      child: Trough(
        index: index,
        children: [
          for (final (i, (name, count)) in tabs.indexed)
            Pressable(
              label: count == null ? name : '$name, $count',
              selected: i == index,
              onPressed: () => onChanged(i),
              builder: (context, s) {
                final on = i == index;
                return Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 13),
                  child: ConstrainedBox(
                    constraints: BoxConstraints(minHeight: t.density.control - 4),
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      crossAxisAlignment: CrossAxisAlignment.center,
                      children: [
                        Text(
                          name,
                          style: t.type.labelStrong.ink(
                            on
                                ? t.colors.ink1
                                : s.hovered
                                ? t.colors.ink2
                                : t.colors.ink3,
                          ),
                        ),
                        if (count != null) ...[
                          const SizedBox(width: 5),
                          Text(count, style: t.type.small.ink(t.colors.ink3)),
                        ],
                      ],
                    ),
                  ),
                );
              },
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
      radius: 9,
      onPressed: onChanged == null ? null : () => onChanged!(!value),
      builder: (context, s) => SizedBox(
        width: 32,
        height: 18,
        child: Surface(
          value && s.enabled ? c.primary : c.track,
          radius: 9,
          child: SpringTo(
            target: value ? 1 : 0,
            spring: AxMotion.snap,
            builder: (_, v, child) => Align(alignment: Alignment(v * 2 - 1, 0), child: child),
            child: Padding(
              padding: const EdgeInsets.all(2),
              child: SizedBox(
                width: 14,
                child: Surface(
                  value && s.enabled ? c.track : c.keyHover,
                  radius: 7,
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
  const AxCheck({
    required this.label,
    required this.value,
    required this.onChanged,
    super.key,
    this.bare = false,
  });
  final String label;

  /// Its label is heard and not shown: what it is for stands beside it.
  final bool bare;
  final bool value;
  final ValueChanged<bool>? onChanged;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Pressable(
      label: label,
      button: false,
      checked: value,
      onPressed: onChanged == null ? null : () => onChanged!(!value),
      builder: (context, s) => Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          SizedBox.square(
            dimension: 18,
            child: value
                ? Surface(
                    s.enabled ? c.primary : c.ghost,
                    radius: AxRadius.tick,
                    child: Center(
                      child: AxGlyph(
                        AxIcon.check,
                        size: 14,
                        color: s.enabled ? c.onPrimary : c.idle,
                      ),
                    ),
                  )
                : DecoratedBox(
                    decoration: BoxDecoration(
                      color: s.enabled ? c.track.fill : null,
                      border: Border.all(
                        color: !s.enabled
                            ? c.meterTrack
                            : s.hovered
                            ? c.ink2
                            : c.idle,
                      ),
                      borderRadius: BorderRadius.circular(AxRadius.tick),
                    ),
                  ),
          ),
          if (!bare) ...[
            const SizedBox(width: 8),
            Text(label, style: t.type.ui.ink(s.enabled ? c.ink1 : c.idle)),
          ],
        ],
      ),
    );
  }
}

/// A text field.
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
    this.secret = false,
    this.leading,
    this.onSubmitted,
    this.onChanged,
    this.autofocus = false,
  });

  /// Takes the keyboard when it comes.
  final bool autofocus;
  final String label, placeholder;
  final String? error;
  final TextEditingController? controller;
  final FocusNode? focusNode;
  final int lines;
  final bool mono, enabled, secret;
  final Widget? leading;
  final ValueChanged<String>? onSubmitted, onChanged;
  @override
  State<AxField> createState() => _AxFieldState();
}

class _AxFieldState extends State<AxField> {
  TextEditingController? _ownText;
  FocusNode? _ownFocus;
  TextEditingController get _c => widget.controller ?? (_ownText ??= TextEditingController());
  FocusNode get _f => widget.focusNode ?? (_ownFocus ??= FocusNode());

  @override
  void initState() {
    super.initState();
    if (!widget.autofocus) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _f.requestFocus();
    });
  }

  @override
  void dispose() {
    _ownText?.dispose();
    _ownFocus?.dispose();
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
        GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: widget.enabled ? () => _f.requestFocus() : null,
          child: ListenableBuilder(
            listenable: _f,
            builder: (_, child) => Surface(
              widget.enabled ? c.field : c.keyDown,
              radius: AxRadius.key,
              ring: bad
                  ? c.fault
                  : _f.hasFocus
                  ? c.focus
                  : null,
              child: child,
            ),
            child: ConstrainedBox(
              constraints: BoxConstraints(minHeight: t.density.control + 4),
              child: Padding(
                padding: EdgeInsets.symmetric(
                  horizontal: t.density.pad,
                  vertical: widget.lines > 1 ? 10 : 0,
                ),
                child: Row(
                  crossAxisAlignment: widget.lines > 1
                      ? CrossAxisAlignment.start
                      : CrossAxisAlignment.center,
                  children: [
                    if (widget.leading != null) ...[widget.leading!, const SizedBox(width: 7)],
                    Expanded(
                      child: AxEditable(
                        label: widget.label,
                        placeholder: widget.placeholder,
                        controller: _c,
                        focusNode: _f,
                        style: style,
                        enabled: widget.enabled,
                        secret: widget.secret,
                        maxLines: widget.lines == 1 ? 1 : null,
                        minLines: widget.lines,
                        onSubmitted: widget.onSubmitted,
                        onChanged: widget.onChanged,
                      ),
                    ),
                  ],
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
        border: Border.all(color: t.colors.hairline),
        borderRadius: BorderRadius.circular(3),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 4, vertical: .5),
        child: Text(keys, style: t.type.caption.ink(t.colors.ink3)),
      ),
    );
  }
}
