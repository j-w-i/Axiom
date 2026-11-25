import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import '../ui/marks.dart';
import '../ui/status.dart';
import '../ui/tokens.dart';

/// What stands over a conversation: the agent, and at the other end what can be done here.
class AgentBar extends StatelessWidget {
  const AgentBar({
    required this.name,
    required this.state,
    super.key,
    this.mark,
    this.trailing,
    this.lead,
    this.where,
    this.inset = 20,
    this.height = 50,
  });
  final String name;
  final AgentState state;
  final ServiceMark? mark;
  final Widget? trailing;

  /// Before the agent; after its name, where it is.
  final Widget? lead, where;
  final double inset, height;

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context);
    final words = MediaQuery.textScalerOf(context).scale(13) * 1.4;
    return LayoutBuilder(
      builder: (context, box) => SizedBox(
        height: math.max(height, math.max(words, t.density.control) + 16),
        child: Padding(
          padding: EdgeInsets.only(left: inset, right: 12),
          child: Row(
            children: [
              Expanded(
                child: Row(
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
                      const SizedBox(width: 14),
                      Flexible(flex: 2, child: w),
                    ],
                  ],
                ),
              ),
              if (trailing case final w?) ...[
                const SizedBox(width: 12),
                // As wide as it is; the name gives way first, down to a part of the bar.
                ConstrainedBox(
                  constraints: BoxConstraints(
                    maxWidth: box.maxWidth.isFinite ? box.maxWidth * .6 : double.infinity,
                  ),
                  child: w,
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}
