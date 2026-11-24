// Tabs (ADR-0024): the selected tab sits on a fill that glides to its new place (critically damped,
// Standard motion only). Arrow keys move between tabs; a strip with
// more tabs than room scrolls (wheel too) and keeps the selected tab in view.

import 'dart:math' as math;
import 'dart:ui' show SemanticsRole;

import 'package:flutter/gestures.dart';
import 'package:flutter/physics.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'pressable.dart';
import 'theme.dart';
import 'tokens.dart';

final class TabItem {
  const TabItem({required this.id, required this.label, this.hint, this.trailing});
  final String id;
  final String label;
  final String? hint;
  final Widget? trailing;
}

class AxTabs extends StatefulWidget {
  const AxTabs({
    super.key,
    required this.label,
    required this.tabs,
    required this.selected,
    required this.onSelect,
    this.height = Metrics.tabHeight,
    this.maxLabelWidth = 200,
  });

  /// The tab list's accessible name.
  final String label;
  final List<TabItem> tabs;
  final String? selected;
  final ValueChanged<String> onSelect;

  /// At text scale 1 (grows with the system's text scale).
  final double height;

  /// A longer label is ellipsised (its full text stays the tab's name and hint).
  final double maxLabelWidth;

  @override
  State<AxTabs> createState() => _AxTabsState();
}

class _AxTabsState extends State<AxTabs> with TickerProviderStateMixin {
  final _keys = <String, GlobalKey>{};
  late final AnimationController _x = AnimationController.unbounded(vsync: this);
  late final AnimationController _w = AnimationController.unbounded(vsync: this);
  final _rowKey = GlobalKey();
  final _scroll = ScrollController();
  bool _placed = false;

  GlobalKey _key(String id) => _keys.putIfAbsent(id, GlobalKey.new);

  @override
  void didUpdateWidget(AxTabs old) {
    super.didUpdateWidget(old);
    final ids = {for (final t in widget.tabs) t.id};
    _keys.removeWhere((id, _) => !ids.contains(id));
    WidgetsBinding.instance.addPostFrameCallback(
      (_) => _place(animate: old.selected != widget.selected),
    );
  }

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _place(animate: false));
  }

  /// Moves the fill under the selected tab and scrolls that tab into view. Only the first
  /// placement rebuilds (the fill appears); later ones drive the animation controllers, so
  /// a parent's rebuild builds the tabs once, not twice (AUD-072).
  void _place({required bool animate}) {
    if (!mounted || widget.selected == null) return;
    final box = _keys[widget.selected]?.currentContext?.findRenderObject() as RenderBox?;
    final row = _rowKey.currentContext?.findRenderObject() as RenderBox?;
    if (box == null || row == null || !box.attached || !box.hasSize) return;
    final x = box.localToGlobal(Offset.zero, ancestor: row).dx;
    final w = box.size.width;
    final m = context.axiom.motion;
    if (!animate || !m.glides || !_placed) {
      _x.value = x;
      _w.value = w;
    } else {
      _x.animateWith(SpringSimulation(m.settle, _x.value, x, _x.velocity));
      _w.animateWith(SpringSimulation(m.settle, _w.value, w, _w.velocity));
    }
    if (_scroll.hasClients) {
      final p = _scroll.position;
      final view = p.viewportDimension;
      double? to;
      if (x < p.pixels) {
        to = x;
      } else if (x + w > p.pixels + view) {
        to = x + w - view;
      }
      if (to != null) _scroll.jumpTo(to.clamp(p.minScrollExtent, p.maxScrollExtent));
    }
    if (!_placed) setState(() => _placed = true);
  }

  void _step(int d) {
    final ids = widget.tabs.map((t) => t.id).toList();
    if (ids.isEmpty) return;
    final i = ids.indexOf(widget.selected ?? '');
    final next = ids[((i < 0 ? 0 : i) + d + ids.length) % ids.length];
    widget.onSelect(next);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      final ctx = _keys[next]?.currentContext;
      if (ctx != null) Focus.maybeOf(ctx)?.requestFocus();
    });
  }

  /// A mouse wheel scrolls the strip sideways (a vertical wheel cannot scroll a horizontal
  /// Scrollable by itself).
  void _wheel(PointerSignalEvent e) {
    if (e is! PointerScrollEvent || !_scroll.hasClients) return;
    final p = _scroll.position;
    final delta = e.scrollDelta.dx != 0 ? e.scrollDelta.dx : e.scrollDelta.dy;
    if (p.maxScrollExtent <= 0 || delta == 0) return;
    GestureBinding.instance.pointerSignalResolver.register(e, (_) {
      _scroll.jumpTo((p.pixels + delta).clamp(p.minScrollExtent, p.maxScrollExtent));
    });
  }

  @override
  void dispose() {
    _x.dispose();
    _w.dispose();
    _scroll.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final height = context.scaled(widget.height, fontSize: 12, lineHeight: 16);
    final strip = Semantics(
      role: SemanticsRole.tabBar,
      label: widget.label,
      container: true,
      child: Stack(
        key: _rowKey,
        children: [
          // The fill under the selected tab: repainted as it glides, never laid out
          // (motion-system.md §3 rule 1).
          if (_placed && widget.selected != null)
            Positioned.fill(
              child: IgnorePointer(
                child: CustomPaint(painter: _TabFill(_x, _w, c.selected, Metrics.radiusControl)),
              ),
            ),
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              for (final tab in widget.tabs)
                Pressable(
                  key: _key(tab.id),
                  label: tab.label,
                  hint: tab.hint,
                  role: PressRole.tab,
                  selected: tab.id == widget.selected,
                  onPressed: () => widget.onSelect(tab.id),
                  builder: (context, s) {
                    final on = tab.id == widget.selected;
                    return FocusRing(
                      visible: s.focused,
                      child: SizedBox(
                        height: height,
                        child: Padding(
                          padding: const EdgeInsets.symmetric(horizontal: Metrics.s4 + Metrics.s1),
                          child: Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              ConstrainedBox(
                                constraints: BoxConstraints(maxWidth: widget.maxLabelWidth),
                                child: Text(
                                  tab.label,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: t.type.small.copyWith(
                                    color: on || s.hovered ? c.ink : c.ink2,
                                    fontWeight: FontWeight.w500,
                                  ),
                                ),
                              ),
                              if (tab.trailing != null) ...[
                                const SizedBox(width: Metrics.s3),
                                tab.trailing!,
                              ],
                            ],
                          ),
                        ),
                      ),
                    );
                  },
                ),
            ],
          ),
        ],
      ),
    );
    return Shortcuts(
      shortcuts: const {
        SingleActivator(LogicalKeyboardKey.arrowRight): _StepIntent(1),
        SingleActivator(LogicalKeyboardKey.arrowLeft): _StepIntent(-1),
      },
      child: Actions(
        actions: {_StepIntent: CallbackAction<_StepIntent>(onInvoke: (i) => _step(i.delta))},
        child: SizedBox(
          height: height,
          // Too many tabs (or a long label) for the room: the strip scrolls, and the selected
          // tab is kept in view (AUD-059).
          child: LayoutBuilder(
            builder: (context, box) => box.maxWidth.isFinite
                ? Listener(
                    onPointerSignal: _wheel,
                    child: SingleChildScrollView(
                      controller: _scroll,
                      scrollDirection: Axis.horizontal,
                      child: strip,
                    ),
                  )
                : strip,
          ),
        ),
      ),
    );
  }
}

class _TabFill extends CustomPainter {
  _TabFill(this.x, this.w, this.color, this.radius) : super(repaint: Listenable.merge([x, w]));
  final Animation<double> x, w;
  final Color color;
  final double radius;

  @override
  void paint(Canvas canvas, Size size) {
    // At most 24 px tall, so a tall strip keeps the same key as a short one.
    final h = math.min(size.height - 6, 24.0);
    canvas.drawRRect(
      RRect.fromRectAndRadius(
        Rect.fromLTWH(x.value + 1, (size.height - h) / 2, math.max(0, w.value - 2), h),
        Radius.circular(radius),
      ),
      Paint()..color = color,
    );
  }

  @override
  bool shouldRepaint(_TabFill o) => o.color != color || o.x != x || o.w != w;
}

class _StepIntent extends Intent {
  const _StepIntent(this.delta);
  final int delta;
}
