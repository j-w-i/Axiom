import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import '../ui/controls.dart';
import '../ui/marks.dart';
import '../ui/status.dart';
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
      final words = MediaQuery.textScalerOf(context).scale(13) * 1.4;
      final wide = box.maxWidth.isFinite ? math.min(340.0, box.maxWidth * .42) : 340.0;
      final views = tabs.isEmpty
          ? const SizedBox.shrink()
          : SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              child: AxTabs(tabs: tabs, index: index, onChanged: onTab ?? (_) {}),
            );
      return SizedBox(
        height: math.max(42, math.max(words, t.density.control.toDouble()) + 14),
        child: Row(
          children: [
            ConstrainedBox(
              constraints: BoxConstraints(maxWidth: wide),
              child: Padding(
                padding: const EdgeInsets.only(left: 20, right: 14),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    if (mark case final m?) ...[
                      AxServiceMark(m, size: 15, labelled: true),
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
                    if (box.maxWidth >= 560) ...[
                      const SizedBox(width: 6),
                      // The lamp says it to whoever hears it.
                      Flexible(
                        child: ExcludeSemantics(
                          child: Text(
                            state.said,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: t.type.label.ink(c.ink3),
                          ),
                        ),
                      ),
                    ],
                  ],
                ),
              ),
            ),
            Expanded(
              child: Padding(
                padding: EdgeInsets.only(right: trailing == null ? 6 : 12),
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
