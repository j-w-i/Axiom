// Tabs with the ink underline (ADR-0020 §4): the selected tab gets a short underline that
// springs (about 3 % overshoot) to its new place. Arrow keys move between tabs.

import 'dart:ui' show SemanticsRole;

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
  });

  /// The tab list's accessible name.
  final String label;
  final List<TabItem> tabs;
  final String? selected;
  final ValueChanged<String> onSelect;
  final double height;

  @override
  State<AxTabs> createState() => _AxTabsState();
}

class _AxTabsState extends State<AxTabs> with TickerProviderStateMixin {
  final _keys = <String, GlobalKey>{};
  late final AnimationController _x = AnimationController.unbounded(vsync: this);
  late final AnimationController _w = AnimationController.unbounded(vsync: this);
  final _rowKey = GlobalKey();
  bool _placed = false;

  GlobalKey _key(String id) => _keys.putIfAbsent(id, GlobalKey.new);

  @override
  void didUpdateWidget(AxTabs old) {
    super.didUpdateWidget(old);
    WidgetsBinding.instance.addPostFrameCallback(
      (_) => _place(animate: old.selected != widget.selected),
    );
  }

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _place(animate: false));
  }

  void _place({required bool animate}) {
    if (!mounted || widget.selected == null) return;
    final box = _keys[widget.selected]?.currentContext?.findRenderObject() as RenderBox?;
    final row = _rowKey.currentContext?.findRenderObject() as RenderBox?;
    if (box == null || row == null || !box.attached) return;
    final left = box.localToGlobal(Offset.zero, ancestor: row).dx;
    // Tauri's view tabs: the underline spans the whole selected tab.
    final w = box.size.width;
    final x = left + (box.size.width - w) / 2;
    final m = context.axiom.motion;
    if (!animate || !m.animates || !_placed) {
      _x.value = x;
      _w.value = w;
    } else {
      _x.animateWith(SpringSimulation(m.settle, _x.value, x, _x.velocity));
      _w.animateWith(SpringSimulation(m.settle, _w.value, w, _w.velocity));
    }
    setState(() => _placed = true);
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

  @override
  void dispose() {
    _x.dispose();
    _w.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    return Shortcuts(
      shortcuts: const {
        SingleActivator(LogicalKeyboardKey.arrowRight): _StepIntent(1),
        SingleActivator(LogicalKeyboardKey.arrowLeft): _StepIntent(-1),
      },
      child: Actions(
        actions: {_StepIntent: CallbackAction<_StepIntent>(onInvoke: (i) => _step(i.delta))},
        child: Semantics(
          role: SemanticsRole.tabBar,
          label: widget.label,
          container: true,
          child: SizedBox(
            height: widget.height,
            child: Stack(
              key: _rowKey,
              children: [
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
                              height: widget.height,
                              child: Padding(
                                padding: const EdgeInsets.symmetric(horizontal: Metrics.s4),
                                child: Row(
                                  mainAxisSize: MainAxisSize.min,
                                  children: [
                                    Text(
                                      tab.label,
                                      style: t.type.small.copyWith(
                                        color: on || s.hovered ? c.ink : c.ink2,
                                        fontWeight: FontWeight.w600,
                                      ),
                                    ),
                                    if (tab.trailing != null) ...[
                                      const SizedBox(width: 6),
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
                if (_placed && widget.selected != null)
                  AnimatedBuilder(
                    animation: Listenable.merge([_x, _w]),
                    builder: (context, _) => Positioned(
                      left: _x.value,
                      bottom: 0,
                      width: _w.value.clamp(0, double.infinity),
                      height: Metrics.underlineHeight,
                      child: IgnorePointer(
                        child: DecoratedBox(decoration: BoxDecoration(color: c.accent)),
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

class _StepIntent extends Intent {
  const _StepIntent(this.delta);
  final int delta;
}
