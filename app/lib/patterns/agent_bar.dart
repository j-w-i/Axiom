import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import '../ui/controls.dart';
import '../ui/marks.dart';
import '../ui/status.dart';
import '../ui/tokens.dart';

/// What stands over a conversation: the agent, the session it is in, and its views beside it.
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
    this.lead,
    this.where,
    this.inset = 20,
    this.height = 50,
  });
  final String name;
  final AgentState state;
  final ServiceMark? mark;
  final List<(String, String?)> tabs;
  final int index;
  final ValueChanged<int>? onTab;
  final Widget? trailing;

  /// Before the agent; after its name, where it is or what has failed.
  final Widget? lead, where;
  final double inset, height;

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, box) {
      final t = Ax.of(context);
      final words = MediaQuery.textScalerOf(context).scale(13) * 1.4;
      final wide = box.maxWidth.isFinite
          ? math.min(460.0, box.maxWidth * (where == null ? .42 : .46))
          : 460.0;
      final views = tabs.isEmpty
          ? const SizedBox.shrink()
          : SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              child: AxTabs(tabs: tabs, index: index, onChanged: onTab ?? (_) {}),
            );
      return SizedBox(
        height: math.max(height, math.max(words, t.density.control.toDouble()) + 16),
        child: Row(
          children: [
            ConstrainedBox(
              constraints: BoxConstraints(maxWidth: wide),
              child: Padding(
                padding: EdgeInsets.only(left: inset, right: 14),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    ?lead,
                    if (mark case final m?) ...[
                      AxServiceMark(m, size: 17, labelled: true),
                      const SizedBox(width: 10),
                    ],
                    Flexible(
                      child: Text(
                        name,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: t.type.uiStrong.copyWith(fontSize: 14),
                      ),
                    ),
                    const SizedBox(width: 10),
                    AxLamp(state),
                    if (where case final w?) ...[
                      const SizedBox(width: 12),
                      Flexible(flex: 2, child: w),
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
                    Flexible(flex: 2, child: views),
                    if (trailing case final w?) ...[
                      const SizedBox(width: 12),
                      Flexible(flex: 3, child: w),
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
