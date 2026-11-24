import 'package:flutter/widgets.dart';

import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/press.dart';
import '../ui/status.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';
import 'models.dart';

/// Every agent of the session, side by side on one panel, divided by seams.
class AgentDeck extends StatelessWidget {
  const AgentDeck({
    required this.agents,
    required this.selected,
    required this.onSelect,
    super.key,
    this.onPause,
    this.onStop,
    this.onAnswer,
    this.minStrip = 176,
  });
  final List<AgentView> agents;
  final String? selected;
  final ValueChanged<String> onSelect;
  final ValueChanged<String>? onPause, onStop, onAnswer;
  final double minStrip;

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, box) {
      final fit = agents.isEmpty ? 0.0 : box.maxWidth / agents.length;
      final w = fit < minStrip ? minStrip : fit;
      return SizedBox(
        height: 132,
        child: Surface(
          Ax.of(context).colors.plate,
          radius: AxRadius.bay,
          child: ClipRRect(
            borderRadius: BorderRadius.circular(AxRadius.bay - 1),
            child: ListView.builder(
              scrollDirection: Axis.horizontal,
              itemExtent: w,
              itemCount: agents.length,
              itemBuilder: (context, i) => Row(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  if (i > 0) const Seam(vertical: true),
                  Expanded(
                    child: AgentStrip(
                      agents[i],
                      selected: agents[i].id == selected,
                      onSelect: () => onSelect(agents[i].id),
                      onPause: onPause == null ? null : () => onPause!(agents[i].id),
                      onStop: onStop == null ? null : () => onStop!(agents[i].id),
                      onAnswer: onAnswer == null ? null : () => onAnswer!(agents[i].id),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      );
    },
  );
}

class AgentStrip extends StatefulWidget {
  const AgentStrip(
    this.agent, {
    required this.onSelect,
    super.key,
    this.selected = false,
    this.onPause,
    this.onStop,
    this.onAnswer,
  });
  final AgentView agent;
  final bool selected;
  final VoidCallback onSelect;
  final VoidCallback? onPause, onStop, onAnswer;
  @override
  State<AgentStrip> createState() => _AgentStripState();
}

class _AgentStripState extends State<AgentStrip> {
  bool _hover = false;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors, a = widget.agent;
    final live = a.state == AgentState.running || a.state == AgentState.asking;
    final recent = a.recent.length > 2 ? a.recent.sublist(a.recent.length - 2) : a.recent;
    final controls = _hover || widget.selected;
    return RepaintBoundary(
      child: MouseRegion(
        onEnter: (_) => setState(() => _hover = true),
        onExit: (_) => setState(() => _hover = false),
        child: Stack(
          children: [
            Positioned.fill(
              child: Pressable(
                label: '${a.name}, ${a.state.word}',
                hint: '${a.model}, ${a.harness}, ${a.connection}, ${a.elapsed}',
                selected: widget.selected,
                radius: 0,
                onPressed: widget.onSelect,
                builder: (context, s) => DecoratedBox(
                  decoration: BoxDecoration(
                    color: !widget.selected && (s.hovered || s.pressed) ? c.hover : null,
                    gradient: widget.selected
                        ? LinearGradient(
                            begin: Alignment.topCenter,
                            end: Alignment.bottomCenter,
                            colors: [c.plateSelected.top, c.plateSelected.bottom],
                          )
                        : null,
                    border: Border(
                      top: BorderSide(
                        width: 2,
                        color: widget.selected
                            ? (a.state == AgentState.asking ? c.attention : c.live)
                            : const Color(0x00000000),
                      ),
                    ),
                  ),
                  child: Padding(
                    padding: const EdgeInsets.fromLTRB(12, 8, 12, 10),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        SizedBox(
                          height: 22,
                          child: Row(
                            children: [
                              AxLamp(a.state),
                              const SizedBox(width: 8),
                              Expanded(
                                child: Text(
                                  a.name,
                                  style: t.type.uiStrong,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                ),
                              ),
                              if (!controls) Text(a.elapsed, style: t.type.small.ink(c.ink3)),
                            ],
                          ),
                        ),
                        Text(
                          '${a.model}  ${a.harness}',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: t.type.label.ink(c.ink3),
                        ),
                        const SizedBox(height: 8),
                        for (final (i, r) in recent.indexed)
                          SizedBox(
                            height: 19,
                            child: Row(
                              children: [
                                AxGlyph(
                                  r.icon,
                                  size: 12,
                                  color: r.failed
                                      ? c.faultText
                                      : i == recent.length - 1 && live
                                      ? a.state.text(c)
                                      : c.ink4,
                                ),
                                const SizedBox(width: 8),
                                Expanded(
                                  child: Text(
                                    r.detail == null ? r.text : '${r.text}  ${r.detail}',
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: t.type.label.ink(
                                      r.failed
                                          ? c.faultText
                                          : i == recent.length - 1 && live
                                          ? c.ink1
                                          : c.ink3,
                                    ),
                                  ),
                                ),
                              ],
                            ),
                          ),
                        const Spacer(),
                        Row(
                          children: [
                            Expanded(
                              child: AxMeter(
                                label: 'Context used',
                                value: a.context,
                                provenance: a.contextProvenance,
                                thickness: 3,
                              ),
                            ),
                            const SizedBox(width: 8),
                            Text(
                              a.context == null
                                  ? 'context unknown'
                                  : '${(a.context! * 100).round()}%',
                              style: t.type.small.ink(c.ink3),
                            ),
                          ],
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
            if (controls)
              Positioned(
                right: 8,
                top: 8,
                child: Row(
                  children: [
                    if (a.state == AgentState.asking && widget.onAnswer != null)
                      AxButton(
                        label: 'Answer',
                        tone: AxTone.answer,
                        size: 22,
                        onPressed: widget.onAnswer,
                      )
                    else if (widget.onPause != null &&
                        (a.state == AgentState.running || a.state == AgentState.paused))
                      AxButton.icon(
                        a.state == AgentState.running ? AxIcon.pause : AxIcon.play,
                        label: '${a.state == AgentState.running ? 'Pause' : 'Resume'} ${a.name}',
                        tone: AxTone.quiet,
                        size: 22,
                        onPressed: widget.onPause,
                      ),
                    if (widget.onStop != null && live)
                      AxButton.icon(
                        AxIcon.stop,
                        label: 'Stop ${a.name}',
                        tone: AxTone.quiet,
                        size: 22,
                        onPressed: widget.onStop,
                      ),
                  ],
                ),
              )
            else if (a.state == AgentState.asking && widget.onAnswer != null)
              const SizedBox.shrink(),
          ],
        ),
      ),
    );
  }
}
