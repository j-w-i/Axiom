import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import '../ui/controls.dart';
import '../ui/marks.dart';
import '../ui/status.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';

/// The bar over a display: the agent in a section of its own, its views beside it.
class AgentBar extends StatelessWidget {
  const AgentBar({
    required this.name,
    required this.state,
    super.key,
    this.mark,
    this.tabs = const [],
    this.index = 0,
    this.onTab,
    this.trailing,
  });
  final String name;
  final AgentState state;
  final ServiceMark? mark;
  final List<(String, String?)> tabs;
  final int index;
  final ValueChanged<int>? onTab;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, box) {
      final t = Ax.of(context), c = t.colors;
      final words = MediaQuery.textScalerOf(context).scale(13) * 1.35;
      final wide = box.maxWidth.isFinite ? math.min(260.0, box.maxWidth * .42) : 260.0;
      final views = tabs.isEmpty
          ? const SizedBox.shrink()
          : SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              child: AxTabs(tabs: tabs, index: index, onChanged: onTab ?? (_) {}),
            );
      return SizedBox(
        height: math.max(40, math.max(words, t.density.control.toDouble()) + 12),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            ConstrainedBox(
              constraints: BoxConstraints(minWidth: math.min(132, wide), maxWidth: wide),
              child: DecoratedBox(
                decoration: BoxDecoration(
                  color: c.plate.bottom,
                  borderRadius: const BorderRadius.only(topLeft: Radius.circular(AxRadius.bay - 1)),
                ),
                child: Padding(
                  padding: const EdgeInsets.fromLTRB(24, 0, 18, 0),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      if (mark case final m?) ...[
                        AxServiceMark(m, size: 14, labelled: true),
                        const SizedBox(width: 9),
                      ],
                      Flexible(
                        child: Text(
                          name,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: t.type.uiStrong,
                        ),
                      ),
                      const SizedBox(width: 10),
                      AxLamp(state),
                    ],
                  ),
                ),
              ),
            ),
            const Seam(vertical: true),
            const SizedBox(width: 6),
            Expanded(
              child: Padding(
                padding: EdgeInsets.only(right: trailing == null ? 6 : 24),
                child: Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    Flexible(flex: 3, child: views),
                    if (trailing case final w?) ...[
                      const SizedBox(width: 12),
                      Flexible(flex: 2, child: w),
                    ],
                  ],
                ),
              ),
            ),
          ],
        ),
      );
    },
  );
}
