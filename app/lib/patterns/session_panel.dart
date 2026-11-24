import 'package:flutter/widgets.dart';

import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/press.dart';
import '../ui/spring.dart';
import '../ui/status.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';
import 'models.dart';

/// The open session: its agents and its plan, in one panel, because they are its own.
class SessionPanel extends StatefulWidget {
  const SessionPanel({
    required this.session,
    required this.selected,
    required this.onSelect,
    super.key,
    this.onAddAgent,
    this.onPause,
    this.onStop,
  });
  final SessionView session;
  final String? selected;
  final ValueChanged<String> onSelect;
  final VoidCallback? onAddAgent;
  final ValueChanged<String>? onPause, onStop;
  @override
  State<SessionPanel> createState() => _SessionPanelState();
}

class _SessionPanelState extends State<SessionPanel> {
  bool _plan = true;

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors, s = widget.session;
    final done = s.tasks.where((k) => k.state == TaskState.done).length;
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: 'Session ${s.title}',
      child: Surface(
        c.plate,
        radius: AxRadius.bay,
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: 5),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(12, 4, 6, 4),
                child: Row(
                  children: [
                    Expanded(
                      child: Text(
                        s.title,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: t.type.uiStrong,
                      ),
                    ),
                    Text(s.age, style: t.type.small.ink(c.ink3)),
                    const SizedBox(width: 4),
                    if (widget.onAddAgent != null)
                      AxButton.icon(
                        AxIcon.plus,
                        label: 'Add an agent to ${s.title}',
                        tone: AxTone.quiet,
                        size: 22,
                        onPressed: widget.onAddAgent,
                      ),
                  ],
                ),
              ),
              for (final a in s.agents)
                AgentRow(
                  a,
                  selected: a.id == widget.selected,
                  onSelect: () => widget.onSelect(a.id),
                  onPause: widget.onPause == null ? null : () => widget.onPause!(a.id),
                  onStop: widget.onStop == null ? null : () => widget.onStop!(a.id),
                ),
              if (s.tasks.isNotEmpty) ...[
                const Padding(padding: EdgeInsets.fromLTRB(12, 6, 12, 2), child: Seam()),
                Pressable(
                  label: 'Plan, $done of ${s.tasks.length} done',
                  expanded: _plan,
                  radius: 0,
                  onPressed: () => setState(() => _plan = !_plan),
                  builder: (context, st) => Padding(
                    padding: const EdgeInsets.fromLTRB(12, 4, 12, 4),
                    child: Row(
                      children: [
                        SizedBox(
                          width: 22,
                          child: Align(
                            alignment: Alignment.centerLeft,
                            child: SpringTo(
                              target: _plan ? 1 : 0,
                              spring: AxMotion.snap,
                              builder: (_, v, child) =>
                                  Transform.rotate(angle: v * 1.5708, child: child),
                              child: AxGlyph(
                                AxIcon.right,
                                size: 10,
                                color: st.hovered ? c.ink2 : c.ink4,
                              ),
                            ),
                          ),
                        ),
                        Expanded(child: Text('Plan', style: t.type.label.ink(c.ink2))),
                        Text('$done of ${s.tasks.length}', style: t.type.small.ink(c.ink3)),
                      ],
                    ),
                  ),
                ),
                if (_plan)
                  for (final k in s.tasks) TaskRow(k),
                const SizedBox(height: 3),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

/// One agent: who, on what, doing what. Its keys appear when the hand is here.
class AgentRow extends StatefulWidget {
  const AgentRow(
    this.agent, {
    required this.onSelect,
    super.key,
    this.selected = false,
    this.onPause,
    this.onStop,
  });
  final AgentView agent;
  final bool selected;
  final VoidCallback onSelect;
  final VoidCallback? onPause, onStop;
  @override
  State<AgentRow> createState() => _AgentRowState();
}

class _AgentRowState extends State<AgentRow> {
  bool _hover = false;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors, a = widget.agent;
    final live = a.state == AgentState.running || a.state == AgentState.asking;
    final now = a.recent.lastOrNull;
    final keys = _hover && (live || a.state == AgentState.paused);
    return RepaintBoundary(
      child: MouseRegion(
        onEnter: (_) => setState(() => _hover = true),
        onExit: (_) => setState(() => _hover = false),
        child: Stack(
          alignment: Alignment.centerRight,
          children: [
            Pressable(
              label: '${a.name}, ${a.state.word}',
              hint: '${a.model}, ${a.harness}, ${a.connection}. ${now?.text ?? ''}',
              selected: widget.selected,
              radius: 0,
              onPressed: widget.onSelect,
              builder: (context, s) => DecoratedBox(
                decoration: BoxDecoration(
                  gradient: widget.selected
                      ? LinearGradient(
                          begin: Alignment.topCenter,
                          end: Alignment.bottomCenter,
                          colors: [c.rowSelectedTop, c.rowSelectedBottom],
                        )
                      : null,
                  color: !widget.selected && s.hovered ? c.hover : null,
                ),
                child: Stack(
                  children: [
                    if (widget.selected)
                      Positioned(
                        left: 0,
                        top: 7,
                        bottom: 7,
                        child: ColoredBox(
                          color: a.state == AgentState.asking ? c.attention : c.live,
                          child: const SizedBox(width: 2),
                        ),
                      ),
                    Padding(
                      padding: const EdgeInsets.fromLTRB(12, 6, 12, 7),
                      child: Row(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          SizedBox(
                            width: 22,
                            height: 18,
                            child: Align(
                              alignment: Alignment.centerLeft,
                              child: AxLamp(a.state, width: 12),
                            ),
                          ),
                          Expanded(
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Row(
                                  children: [
                                    Text(a.name, style: t.type.ui.ink(c.ink1)),
                                    const SizedBox(width: 8),
                                    Expanded(
                                      child: Text(
                                        a.model,
                                        maxLines: 1,
                                        overflow: TextOverflow.ellipsis,
                                        style: t.type.label.ink(c.ink4),
                                      ),
                                    ),
                                    if (!keys) Text(a.elapsed, style: t.type.small.ink(c.ink4)),
                                  ],
                                ),
                                if (now != null)
                                  Text(
                                    now.text,
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: t.type.label.ink(
                                      a.state == AgentState.asking
                                          ? c.attentionText
                                          : now.failed
                                          ? c.faultText
                                          : c.ink3,
                                    ),
                                  ),
                              ],
                            ),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
            ),
            if (keys)
              Positioned(
                right: 8,
                child: Row(
                  children: [
                    if (widget.onPause != null && a.state != AgentState.asking)
                      AxButton.icon(
                        a.state == AgentState.paused ? AxIcon.play : AxIcon.pause,
                        label: '${a.state == AgentState.paused ? 'Resume' : 'Pause'} ${a.name}',
                        size: 22,
                        onPressed: widget.onPause,
                      ),
                    const SizedBox(width: 3),
                    if (widget.onStop != null && live)
                      AxButton.icon(
                        AxIcon.stop,
                        label: 'Stop ${a.name}',
                        size: 22,
                        onPressed: widget.onStop,
                      ),
                  ],
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class TaskRow extends StatelessWidget {
  const TaskRow(this.task, {super.key});
  final TaskView task;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final (mark, word) = switch (task.state) {
      TaskState.done => (AxGlyph(AxIcon.check, size: 12, color: c.doneText), 'done'),
      TaskState.active => (const AxLamp(AgentState.running, width: 9, breathe: false), 'active'),
      TaskState.todo => (const AxLamp(AgentState.waiting, width: 9), 'to do'),
      TaskState.blocked => (AxGlyph(AxIcon.warn, size: 12, color: c.attentionText), 'blocked'),
    };
    return Semantics(
      label: '${task.title}, $word${task.owner == null ? '' : ', ${task.owner}'}',
      excludeSemantics: true,
      child: SizedBox(
        height: 25,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12),
          child: Row(
            children: [
              SizedBox(
                width: 22,
                child: Align(alignment: Alignment.centerLeft, child: mark),
              ),
              Expanded(
                child: Text(
                  task.title,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: t.type.label.ink(task.state == TaskState.active ? c.ink1 : c.ink3),
                ),
              ),
              if (task.owner != null && task.state != TaskState.done) ...[
                const SizedBox(width: 8),
                Text(task.owner!, style: t.type.small.ink(c.ink4)),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

/// A session that is not open: its name, its age, and what its agents are doing, as lamps.
class SessionRow extends StatelessWidget {
  const SessionRow(this.session, {required this.onOpen, super.key});
  final SessionView session;
  final VoidCallback onOpen;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final busy = session.agents
        .where((a) => a.state == AgentState.running || a.state == AgentState.asking)
        .toList();
    return Pressable(
      label: session.title,
      hint: busy.isEmpty ? session.age : '${busy.length} at work',
      onPressed: onOpen,
      builder: (context, s) => DecoratedBox(
        decoration: BoxDecoration(
          color: s.hovered ? c.hover : null,
          borderRadius: BorderRadius.circular(AxRadius.key),
        ),
        child: SizedBox(
          height: t.density.row + 2,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    session.title,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: t.type.ui.ink(c.ink2),
                  ),
                ),
                for (final a in busy.take(4)) ...[
                  const SizedBox(width: 3),
                  AxLamp(a.state, width: 8),
                ],
                const SizedBox(width: 8),
                Text(session.age, style: t.type.small.ink(c.ink4)),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
