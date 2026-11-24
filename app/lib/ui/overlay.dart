import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'icons.dart';
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
    this.alignEnd = false,
  });
  final Widget Function(BuildContext, bool open, VoidCallback toggle) anchor;
  final Widget Function(BuildContext, VoidCallback close) panel;
  final double? width;
  final bool alignEnd;
  @override
  State<AxPopover> createState() => AxPopoverState();
}

class AxPopoverState extends State<AxPopover> {
  final _ctl = OverlayPortalController();
  final _scope = FocusScopeNode();
  FocusNode? _back;
  Rect _at = Rect.zero;

  void toggle() => _ctl.isShowing ? close() : open();
  void open() {
    final box = context.findRenderObject()! as RenderBox;
    final overlay = Overlay.of(context).context.findRenderObject()! as RenderBox;
    _at = box.localToGlobal(Offset.zero, ancestor: overlay) & box.size;
    _back = FocusManager.instance.primaryFocus;
    setState(_ctl.show);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _scope.requestFocus();
    });
  }

  void close() {
    if (!_ctl.isShowing) return;
    setState(_ctl.hide);
    _back?.requestFocus();
  }

  @override
  void dispose() {
    _scope.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => TapRegion(
    groupId: this,
    child: OverlayPortal(
      controller: _ctl,
      overlayChildBuilder: (context) => CustomSingleChildLayout(
        delegate: _Place(_at, widget.alignEnd),
        child: TapRegion(
          groupId: this,
          onTapOutside: (_) => close(),
          child: FocusScope(
            node: _scope,
            child: CallbackShortcuts(
              bindings: {const SingleActivator(LogicalKeyboardKey.escape): close},
              child: _Rise(
                child: ConstrainedBox(
                  constraints: BoxConstraints(
                    minWidth: widget.width ?? _at.width,
                    maxWidth: widget.width ?? 360,
                  ),
                  child: Surface(
                    Ax.of(context).colors.overlay,
                    radius: AxRadius.overlay,
                    child: Padding(
                      padding: const EdgeInsets.all(5),
                      child: widget.panel(context, close),
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

class _Place extends SingleChildLayoutDelegate {
  const _Place(this.at, this.end);
  final Rect at;
  final bool end;
  @override
  BoxConstraints getConstraintsForChild(BoxConstraints c) =>
      BoxConstraints(maxWidth: c.maxWidth - 16, maxHeight: c.maxHeight - 16);
  @override
  Offset getPositionForChild(Size s, Size child) {
    final below = at.bottom + 5 + child.height <= s.height - 8;
    final x = end ? at.right - child.width : at.left;
    final y = below ? at.bottom + 5 : at.top - 5 - child.height;
    return Offset(
      x.clamp(8, (s.width - child.width - 8).clamp(8, double.infinity)),
      y.clamp(8, (s.height - child.height - 8).clamp(8, double.infinity)),
    );
  }

  @override
  bool shouldRelayout(_Place o) => o.at != at || o.end != end;
}

class AxMenuItem<T> {
  const AxMenuItem(this.value, this.label, {this.detail, this.icon, this.enabled = true, this.why});
  final T value;
  final String label;
  final String? detail;
  final AxIcon? icon;
  final bool enabled;

  /// Why a disabled item is disabled; shown in place of [detail].
  final String? why;
}

/// Arrow keys, Home, End and type-ahead move; Enter chooses.
class AxMenu<T> extends StatefulWidget {
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
  State<AxMenu<T>> createState() => _AxMenuState<T>();
}

class _AxMenuState<T> extends State<AxMenu<T>> {
  late final _nodes = [for (final _ in widget.items) FocusNode()];
  @override
  void initState() {
    super.initState();
    final i = widget.items.indexWhere((e) => e.value == widget.value && e.enabled);
    final first = i >= 0 ? i : widget.items.indexWhere((e) => e.enabled);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && first >= 0) _nodes[first].requestFocus();
    });
  }

  @override
  void dispose() {
    for (final n in _nodes) {
      n.dispose();
    }
    super.dispose();
  }

  void _go(int from, int step) {
    var i = from;
    for (var k = 0; k < _nodes.length; k++) {
      i = (i + step) % _nodes.length;
      if (widget.items[i].enabled) return _nodes[i].requestFocus();
    }
  }

  KeyEventResult _key(FocusNode _, KeyEvent e) {
    if (e is KeyUpEvent) return KeyEventResult.ignored;
    final at = _nodes.indexWhere((n) => n.hasFocus);
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
        if (ch == null || ch.trim().isEmpty) return KeyEventResult.ignored;
        for (var k = 1; k <= _nodes.length; k++) {
          final i = (at + k) % _nodes.length;
          final it = widget.items[i];
          if (it.enabled && it.label.toLowerCase().startsWith(ch)) {
            _nodes[i].requestFocus();
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
              for (final (i, it) in widget.items.indexed)
                Pressable(
                  label: it.label,
                  hint: it.enabled ? it.detail : it.why,
                  selected: it.value == widget.value,
                  focusNode: _nodes[i],
                  onPressed: it.enabled ? () => widget.onChosen(it.value) : null,
                  builder: (context, s) => DecoratedBox(
                    decoration: BoxDecoration(
                      color: s.hovered || s.focused ? c.hover : null,
                      borderRadius: BorderRadius.circular(AxRadius.key),
                    ),
                    child: Padding(
                      padding: EdgeInsets.symmetric(horizontal: 8, vertical: t.density.pad / 2 + 1),
                      child: Row(
                        children: [
                          SizedBox(
                            width: 18,
                            child: it.value == widget.value
                                ? AxGlyph(AxIcon.check, size: 12, color: c.liveText)
                                : it.icon != null
                                ? AxGlyph(it.icon!, size: 13)
                                : null,
                          ),
                          Flexible(
                            child: Text(
                              it.label,
                              style: t.type.ui.ink(it.enabled ? c.ink1 : c.idle),
                            ),
                          ),
                          if ((it.enabled ? it.detail : it.why) case final d?) ...[
                            const SizedBox(width: 14),
                            const Spacer(),
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

/// A two-line selector key: what is being chosen, and the choice.
class AxSelect<T> extends StatelessWidget {
  const AxSelect({
    required this.label,
    required this.items,
    required this.value,
    required this.onChanged,
    super.key,
  });
  final String label;
  final List<AxMenuItem<T>> items;
  final T value;
  final ValueChanged<T> onChanged;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final chosen = items.where((e) => e.value == value).firstOrNull;
    return AxPopover(
      anchor: (context, open, toggle) => Pressable(
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
          child: SizedBox(
            height: t.density.control + 6,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 9),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(label, style: t.type.caption.ink(c.ink3)),
                      Text(
                        chosen?.label ?? 'None',
                        style: t.type.labelStrong.copyWith(height: 1.2),
                      ),
                    ],
                  ),
                  const SizedBox(width: 8),
                  AxGlyph(AxIcon.down, size: 9),
                ],
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
}
