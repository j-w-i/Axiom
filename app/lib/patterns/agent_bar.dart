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

  static const height = 40.0;

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final hue = mark?.color(c);
    return IntrinsicHeight(
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ConstrainedBox(
            constraints: const BoxConstraints(minHeight: height, minWidth: 132, maxWidth: 260),
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
          if (tabs.isNotEmpty)
            AxTabs(tabs: tabs, index: index, hue: hue, onChanged: onTab ?? (_) {}),
          Expanded(
            child: Align(
              alignment: Alignment.centerRight,
              child: Padding(padding: const EdgeInsets.only(right: 24), child: trailing),
            ),
          ),
        ],
      ),
    );
  }
}
