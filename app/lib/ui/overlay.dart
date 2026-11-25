import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'commands.dart';
import 'controls.dart';
import 'icons.dart';
import 'marks.dart';
import 'press.dart';
import 'spring.dart';
import 'surface.dart';
import 'tokens.dart';

/// Opens [panel] under (or over, near the window's edge) its anchor. Escape and outside clicks
/// close it and give focus back.
class AxPopover extends StatefulWidget {
  const AxPopover({
    required this.anchor,
    required this.panel,
    super.key,
    this.width,
    this.maxHeight,
    this.alignEnd = false,
  });
  final Widget Function(BuildContext, bool open, VoidCallback toggle) anchor;
  final Widget Function(BuildContext, VoidCallback close) panel;
  final double? width, maxHeight;
  final bool alignEnd;
  @override
  State<AxPopover> createState() => AxPopoverState();
}

class AxPopoverState extends State<AxPopover> with WidgetsBindingObserver {
  final _ctl = OverlayPortalController();
  final _scope = FocusScopeNode();
  FocusNode? _back;
  Rect _at = Rect.zero;
  _Side _side = _Side();

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  /// The anchor has moved: the panel would hang where it was.
  @override
  void didChangeMetrics() => close();

  void toggle() => _ctl.isShowing ? close() : open();
  void open() {
    final box = context.findRenderObject()! as RenderBox;
    final overlay = Overlay.of(context).context.findRenderObject()! as RenderBox;
    _at = box.localToGlobal(Offset.zero, ancestor: overlay) & box.size;
    _side = _Side();
    _back = FocusManager.instance.primaryFocus;
    setState(_ctl.show);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      _scope.requestFocus();
      // A panel without a menu of its own still takes the keyboard.
      if (_scope.focusedChild == null) _scope.nextFocus();
    });
  }

  void close() {
    if (!_ctl.isShowing) return;
    setState(_ctl.hide);
    _back?.requestFocus();
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _scope.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => TapRegion(
    groupId: this,
    child: OverlayPortal(
      controller: _ctl,
      overlayChildBuilder: (context) => CustomSingleChildLayout(
        delegate: _Place(_at, _side, end: widget.alignEnd),
        child: TapRegion(
          groupId: this,
          onTapOutside: (_) => close(),
          child: CallbackShortcuts(
            bindings: {const SingleActivator(LogicalKeyboardKey.escape): close},
            child: FocusScope(
              node: _scope,
              child: _Rise(
                child: ConstrainedBox(
                  constraints: BoxConstraints(
                    minWidth: widget.width ?? _at.width,
                    maxWidth: widget.width ?? (_at.width > 360 ? _at.width : 360),
                    maxHeight: widget.maxHeight ?? double.infinity,
                  ),
                  child: Surface(
                    Ax.of(context).colors.overlay,
                    radius: AxRadius.card,
                    child: Padding(
                      padding: const EdgeInsets.all(AxSpace.menu),
                      // However much there is, it stays in the window and scrolls.
                      child: SingleChildScrollView(child: widget.panel(context, close)),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
      child: widget.anchor(context, _ctl.isShowing, toggle),
    ),
  );
}

class _Rise extends StatefulWidget {
  const _Rise({required this.child});
  final Widget child;
  @override
  State<_Rise> createState() => _RiseState();
}

class _RiseState extends State<_Rise> {
  bool _in = false;
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback(
      (_) => mounted ? setState(() => _in = true) : null,
    );
  }

  @override
  Widget build(BuildContext context) => SpringTo(
    target: _in || reducedMotion(context) ? 1 : 0,
    spring: AxMotion.snap,
    builder: (_, v, child) => Opacity(
      opacity: v.clamp(0, 1),
      child: Transform.translate(offset: Offset(0, (1 - v) * -4), child: child),
    ),
    child: widget.child,
  );
}

/// Which side a panel hangs on: settled when it opens, so that one that grows stays there.
class _Side {
  bool? below;
}

class _Place extends SingleChildLayoutDelegate {
  const _Place(this.at, this.side, {required this.end});
  final _Side side;
  final Rect at;
  final bool end;
  @override
  BoxConstraints getConstraintsForChild(BoxConstraints c) =>
      BoxConstraints(maxWidth: c.maxWidth - 16, maxHeight: c.maxHeight - 16);
  @override
  Offset getPositionForChild(Size s, Size child) {
    final below = side.below ??= at.bottom + 5 + child.height <= s.height - 8;
    final x = end ? at.right - child.width : at.left;
    final y = below ? at.bottom + 5 : at.top - 5 - child.height;
    return Offset(
      x.clamp(8, (s.width - child.width - 8).clamp(8, double.infinity)),
      y.clamp(8, (s.height - child.height - 8).clamp(8, double.infinity)),
    );
  }

  @override
  bool shouldRelayout(_Place o) => o.at != at || o.end != end || o.side != side;
}

class AxMenuItem<T> {
  const AxMenuItem(
    this.value,
    this.label, {
    this.detail,
    this.icon,
    this.mark,
    this.enabled = true,
    this.why,
  });
  final T value;
  final String label;
  final String? detail;
  final AxIcon? icon;
  final ServiceMark? mark;
  final bool enabled;

  /// Why a disabled item is disabled; shown in place of [detail].
  final String? why;
}

/// Of many items only some are shown, and a field to narrow them down.
class AxNarrow<T> extends StatefulWidget {
  const AxNarrow({required this.items, required this.builder, super.key, this.value});
  final List<AxMenuItem<T>> items;
  final T? value;
  final Widget Function(BuildContext, List<AxMenuItem<T>> shown) builder;

  /// More than this are narrowed down.
  static const most = 40;
  @override
  State<AxNarrow<T>> createState() => _AxNarrowState<T>();
}

class _AxNarrowState<T> extends State<AxNarrow<T>> {
  final _words = TextEditingController();
  final _field = FocusNode();

  @override
  void initState() {
    super.initState();
    _words.addListener(() => setState(() {}));
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && widget.items.length > AxNarrow.most) _field.requestFocus();
    });
  }

  @override
  void dispose() {
    _field.dispose();
    _words.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final seen = <T>{};
    final all = [
      for (final i in widget.items)
        if (seen.add(i.value)) i,
    ];
    if (all.length <= AxNarrow.most) return widget.builder(context, all);
    final t = Ax.of(context), c = t.colors;
    final words = _words.text.toLowerCase().split(' ').where((w) => w.isNotEmpty).toList();
    final fit = [
      for (final i in all)
        if (words.every('${i.label} ${i.detail ?? ''}'.toLowerCase().contains)) i,
    ];
    // What is chosen now is among them.
    final shown = fit.take(AxNarrow.most).toList();
    final chosen = fit.where((i) => i.value == widget.value).firstOrNull;
    if (chosen != null && !shown.contains(chosen)) shown.insert(0, chosen);
    final more = fit.length - shown.length;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.only(bottom: 6),
          child: CallbackShortcuts(
            bindings: {
              const SingleActivator(LogicalKeyboardKey.arrowDown): () =>
                  FocusScope.of(context).nextFocus(),
            },
            child: AxField(
              label: 'Find one of ${all.length}',
              placeholder: 'Find one of ${all.length}',
              controller: _words,
              focusNode: _field,
            ),
          ),
        ),
        widget.builder(context, shown),
        if (more > 0 || fit.isEmpty)
          Padding(
            padding: const EdgeInsets.fromLTRB(AxSpace.row, 6, AxSpace.row, 4),
            child: Text(fit.isEmpty ? 'None fits' : '$more more', style: t.type.small.ink(c.ink3)),
          ),
      ],
    );
  }
}

/// Arrow keys, Home, End and type-ahead move; Enter chooses.
class AxMenu<T> extends StatelessWidget {
  const AxMenu({
    required this.items,
    required this.onChosen,
    super.key,
    this.value,
    this.label = 'Menu',
  });
  final List<AxMenuItem<T>> items;
  final T? value;
  final String label;
  final ValueChanged<T> onChosen;
  @override
  Widget build(BuildContext context) => AxNarrow<T>(
    items: items,
    value: value,
    builder: (context, shown) => Settle(
      // What comes under a pointer by itself is not what is pressed.
      stamp: Object.hashAll([for (final i in shown) i.value]),
      child: _Items<T>(
        items: shown,
        value: value,
        label: label,
        onChosen: onChosen,
        take: items.length <= AxNarrow.most,
      ),
    ),
  );
}

class _Items<T> extends StatefulWidget {
  const _Items({
    required this.items,
    required this.value,
    required this.label,
    required this.onChosen,
    required this.take,
  });
  final List<AxMenuItem<T>> items;
  final T? value;
  final String label;
  final ValueChanged<T> onChosen;

  /// The focus, when it comes.
  final bool take;
  @override
  State<_Items<T>> createState() => _ItemsState<T>();
}

class _ItemsState<T> extends State<_Items<T>> {
  /// Of each value: an item that moves keeps the focus it has.
  final _all = <T, FocusNode>{};
  List<FocusNode> get _nodes => [for (final i in widget.items) _node(i.value)];
  FocusNode _node(T v) => _all.putIfAbsent(v, FocusNode.new);

  @override
  void initState() {
    super.initState();
    if (!widget.take) return;
    final i = widget.items.indexWhere((e) => e.value == widget.value && e.enabled);
    final first = i >= 0 ? i : widget.items.indexWhere((e) => e.enabled);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && first >= 0 && first < widget.items.length) _take(first);
    });
  }

  @override
  void didUpdateWidget(_Items<T> old) {
    super.didUpdateWidget(old);
    final now = {for (final i in widget.items) i.value};
    final focused = old.items.indexWhere((i) => _all[i.value]?.hasFocus ?? false);
    final lost = focused >= 0 && !now.contains(old.items[focused].value);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      // What had the focus has gone: its place has it.
      if (lost && widget.items.isNotEmpty) _take(focused.clamp(0, widget.items.length - 1));
      _all.removeWhere((v, n) {
        if (widget.items.any((i) => i.value == v)) return false;
        n.dispose();
        return true;
      });
    });
  }

  @override
  void dispose() {
    for (final n in _all.values) {
      n.dispose();
    }
    super.dispose();
  }

  void _go(int from, int step) {
    var i = from;
    final n = widget.items.length;
    for (var k = 0; k < n; k++) {
      i = (i + step) % n;
      if (widget.items[i].enabled) return _take(i);
    }
  }

  /// Focus, and into view if the menu scrolls.
  void _take(int i) {
    final node = _node(widget.items[i].value)..requestFocus();
    if (node.context case final c?) {
      unawaited(
        Scrollable.ensureVisible(
          c,
          alignmentPolicy: ScrollPositionAlignmentPolicy.keepVisibleAtEnd,
        ),
      );
      unawaited(
        Scrollable.ensureVisible(
          c,
          alignmentPolicy: ScrollPositionAlignmentPolicy.keepVisibleAtStart,
        ),
      );
    }
  }

  KeyEventResult _key(FocusNode _, KeyEvent e) {
    if (e is KeyUpEvent) return KeyEventResult.ignored;
    final k = HardwareKeyboard.instance;
    if (k.isMetaPressed || k.isControlPressed || k.isAltPressed) return KeyEventResult.ignored;
    final nodes = _nodes;
    if (nodes.isEmpty) return KeyEventResult.ignored;
    final at = nodes.indexWhere((n) => n.hasFocus);
    switch (e.logicalKey) {
      case LogicalKeyboardKey.arrowDown:
        _go(at, 1);
      case LogicalKeyboardKey.arrowUp:
        _go(at < 0 ? 0 : at, -1);
      case LogicalKeyboardKey.home:
        _go(-1, 1);
      case LogicalKeyboardKey.end:
        _go(0, -1);
      default:
        final ch = e.character?.toLowerCase();
        // Some systems deliver Escape and friends as control characters.
        if (ch == null || ch.trim().isEmpty || ch.codeUnitAt(0) < 32 || ch.codeUnitAt(0) == 127) {
          return KeyEventResult.ignored;
        }
        for (var k = 1; k <= nodes.length; k++) {
          final i = (at + k) % nodes.length;
          final it = widget.items[i];
          if (it.enabled && it.label.toLowerCase().startsWith(ch)) {
            _take(i);
            break;
          }
        }
    }
    return KeyEventResult.handled;
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Focus(
      canRequestFocus: false,
      onKeyEvent: _key,
      child: Semantics(
        label: widget.label,
        explicitChildNodes: true,
        container: true,
        child: IntrinsicWidth(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              for (final it in widget.items)
                Pressable(
                  key: ValueKey(it.value),
                  label: it.label,
                  hint: it.enabled ? it.detail : it.why,
                  selected: it.value == widget.value,
                  focusNode: _node(it.value),
                  onPressed: it.enabled ? () => widget.onChosen(it.value) : null,
                  builder: (context, s) => DecoratedBox(
                    decoration: BoxDecoration(
                      color: s.hovered || s.focused ? c.tinted(it.mark?.color(c)) : null,
                      borderRadius: BorderRadius.circular(AxRadius.key),
                    ),
                    child: Padding(
                      padding: EdgeInsets.symmetric(
                        horizontal: AxSpace.row,
                        vertical: t.density.pad / 2 + 1,
                      ),
                      child: Row(
                        children: [
                          SizedBox(
                            width: 20,
                            child: Align(
                              alignment: Alignment.centerLeft,
                              child: it.value == widget.value
                                  ? AxGlyph(AxIcon.check, color: it.mark?.color(c) ?? c.liveText)
                                  : it.icon != null
                                  ? AxGlyph(it.icon!)
                                  : null,
                            ),
                          ),
                          if (it.mark case final m?) ...[
                            AxServiceMark(m, color: it.enabled ? null : c.idle),
                            const SizedBox(width: 8),
                          ],
                          Expanded(
                            child: Text(
                              it.label,
                              style: t.type.ui.ink(it.enabled ? c.ink1 : c.idle),
                            ),
                          ),
                          if ((it.enabled ? it.detail : it.why) case final d?) ...[
                            const SizedBox(width: 18),
                            Text(d, style: t.type.small.ink(c.ink3)),
                          ],
                        ],
                      ),
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

/// A key that says what is set and opens what it can be set to, without a ground until it is under
/// the hand.
class AxChip extends StatelessWidget {
  const AxChip({
    required this.label,
    required this.words,
    required this.open,
    required this.onPressed,
    super.key,
    this.icon,
    this.mark,
    this.calls = false,
  });
  final String label, words;
  final AxIcon? icon;
  final ServiceMark? mark;

  /// Something has to be set here first.
  final bool open, calls;
  final VoidCallback onPressed;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Pressable(
      label: label,
      expanded: open,
      onPressed: onPressed,
      builder: (context, s) {
        final ink = open || s.hovered || calls ? c.ink1 : c.ink2;
        return Ground(
          open
              ? c.selected
              : s.hovered
              ? c.hover
              : null,
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
                  ] else if (icon case final i?) ...[
                    AxGlyph(i, color: ink),
                    const SizedBox(width: 7),
                  ],
                  Flexible(
                    child: Text(
                      words,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: (calls ? t.type.labelStrong : t.type.label).ink(ink),
                    ),
                  ),
                  const SizedBox(width: 6),
                  AxGlyph(AxIcon.down, color: c.ink3),
                ],
              ),
            ),
          ),
        );
      },
    );
  }
}

/// A selector key: the choice, after what is being chosen where that is not plain.
class AxSelect<T> extends StatelessWidget {
  const AxSelect({
    required this.label,
    required this.items,
    required this.value,
    required this.onChanged,
    super.key,
    this.named = true,
    this.face,
    this.quiet = false,
    this.icon,
    this.command,
  });

  /// Opens it.
  final Command? command;

  /// It stands among others that have no ground of their own.
  final bool quiet;
  final AxIcon? icon;

  /// Shown in place of the choice.
  final String? face;
  final String label;

  /// Its label is shown, and not only heard.
  final bool named;
  final List<AxMenuItem<T>> items;
  final T value;
  final ValueChanged<T> onChanged;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final chosen = items.where((e) => e.value == value).firstOrNull;
    return AxPopover(
      anchor: (context, open, toggle) => _commanded(
        toggle,
        quiet
            ? AxChip(
                label: '$label: ${chosen?.label ?? 'none'}',
                words: face ?? chosen?.label ?? 'None',
                icon: icon,
                mark: chosen?.mark,
                open: open,
                onPressed: toggle,
              )
            : Pressable(
                label: '$label: ${chosen?.label ?? 'none'}',
                expanded: open,
                onPressed: toggle,
                builder: (context, s) => Surface(
                  s.pressed || open
                      ? c.keyDown
                      : s.hovered
                      ? c.keyHover
                      : c.key,
                  radius: AxRadius.key,
                  child: ConstrainedBox(
                    // Taller with larger text.
                    constraints: BoxConstraints(minHeight: t.density.control),
                    child: Padding(
                      padding: const EdgeInsets.fromLTRB(12, 0, 10, 0),
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          if (chosen?.mark case final m? when face == null) ...[
                            AxServiceMark(m, size: 13),
                            const SizedBox(width: 8),
                          ],
                          if (named && face == null) ...[
                            Text(label, style: t.type.label.ink(c.ink3)),
                            const SizedBox(width: 6),
                          ],
                          Flexible(
                            child: Text(
                              face ?? chosen?.label ?? 'None',
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: t.type.labelStrong,
                            ),
                          ),
                          const SizedBox(width: 6),
                          AxGlyph(AxIcon.down, color: c.ink3),
                        ],
                      ),
                    ),
                  ),
                ),
              ),
      ),
      panel: (context, close) => AxMenu<T>(
        label: label,
        items: items,
        value: value,
        onChosen: (v) {
          close();
          onChanged(v);
        },
      ),
    );
  }

  Widget _commanded(VoidCallback toggle, Widget w) =>
      command == null ? w : OnCommand(command!, toggle, child: w);
}
