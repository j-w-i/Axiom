import 'package:flutter/semantics.dart';
import 'package:flutter/widgets.dart';

import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/marks.dart';
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
          padding: const EdgeInsets.fromLTRB(1, 1, 1, 5),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: [
              ConstrainedBox(
                constraints: const BoxConstraints(minHeight: 36),
                child: Padding(
                  padding: const EdgeInsets.only(left: 11, right: 3),
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
                      if (widget.onAddAgent != null) ...[
                        const SizedBox(width: 2),
                        AxButton.icon(
                          AxIcon.plus,
                          label: 'Add an agent to ${s.title}',
                          tone: AxTone.quiet,
                          size: 22,
                          onPressed: widget.onAddAgent,
                        ),
                      ] else
                        const SizedBox(width: 5),
                    ],
                  ),
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
                const Padding(padding: EdgeInsets.fromLTRB(11, 6, 11, 2), child: Seam()),
                Pressable(
                  label: 'Plan, $done of ${s.tasks.length} done',
                  expanded: _plan,
                  radius: 0,
                  onPressed: () => setState(() => _plan = !_plan),
                  builder: (context, st) => Padding(
                    padding: const EdgeInsets.fromLTRB(11, 4, 11, 4),
                    child: Row(
                      children: [
                        SizedBox(
                          width: 22,
                          child: Align(
                            alignment: const Alignment(-.82, 0),
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
  bool _hover = false, _within = false;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors, a = widget.agent;
    final hue = a.mark?.color(c);
    final live = a.state == AgentState.running || a.state == AgentState.asking;
    final now = a.recent.lastOrNull;
    final pause =
        widget.onPause != null && (a.state == AgentState.running || a.state == AgentState.paused);
    final stop = widget.onStop != null && live;
    final pauseWord = '${a.state == AgentState.paused ? 'Resume' : 'Pause'} ${a.name}';
    final keys = (_hover || _within) && (pause || stop);
    return RepaintBoundary(
      child: MouseRegion(
        onEnter: (_) => setState(() => _hover = true),
        onExit: (_) => setState(() => _hover = false),
        child: Focus(
          canRequestFocus: false,
          skipTraversal: true,
          onFocusChange: (v) => setState(() => _within = v),
          child: Stack(
            alignment: Alignment.centerRight,
            children: [
              Pressable(
                label: '${a.name}, ${a.state.word}',
                hint: [
                  for (final x in [a.model, a.harness, a.connection, now?.text ?? ''])
                    if (x.isNotEmpty) x,
                ].join(', '),
                selected: widget.selected,
                radius: 0,
                onPressed: widget.onSelect,
                actions: {
                  if (pause) CustomSemanticsAction(label: pauseWord): widget.onPause!,
                  if (stop) CustomSemanticsAction(label: 'Stop ${a.name}'): widget.onStop!,
                },
                builder: (context, s) => DecoratedBox(
                  decoration: BoxDecoration(
                    gradient: widget.selected
                        ? LinearGradient(
                            begin: Alignment.topCenter,
                            end: Alignment.bottomCenter,
                            colors: [
                              for (final k in [c.rowSelectedTop, c.rowSelectedBottom])
                                c.selected(k, hue),
                            ],
                          )
                        : null,
                    color: !widget.selected && s.hovered ? c.tinted(hue) : null,
                  ),
                  child: Stack(
                    children: [
                      Padding(
                        padding: const EdgeInsets.fromLTRB(11, 6, 11, 7),
                        child: Row(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            _Mark(AxLamp(a.state), t.type.ui),
                            Expanded(
                              child: Padding(
                                padding: EdgeInsets.only(
                                  right: keys ? (pause && stop ? 59 : 32) : 0,
                                ),
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    Row(
                                      crossAxisAlignment: CrossAxisAlignment.baseline,
                                      textBaseline: TextBaseline.alphabetic,
                                      children: [
                                        Expanded(
                                          child: Text.rich(
                                            TextSpan(
                                              children: [
                                                TextSpan(text: a.name),
                                                const TextSpan(text: '   '),
                                                if (a.mark case final m?)
                                                  WidgetSpan(
                                                    alignment: PlaceholderAlignment.middle,
                                                    child: Padding(
                                                      padding: const EdgeInsets.only(right: 5),
                                                      child: AxServiceMark(m, size: 11),
                                                    ),
                                                  ),
                                                TextSpan(
                                                  text: a.model,
                                                  style: t.type.label.ink(
                                                    widget.selected ? c.ink3 : c.ink4,
                                                  ),
                                                ),
                                              ],
                                            ),
                                            maxLines: 1,
                                            overflow: TextOverflow.ellipsis,
                                            style: t.type.ui,
                                          ),
                                        ),
                                        const SizedBox(width: 8),
                                        if (!keys)
                                          Text(
                                            a.elapsed,
                                            style: t.type.small.ink(
                                              widget.selected ? c.ink3 : c.ink4,
                                            ),
                                          ),
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
                  right: 11,
                  child: Row(
                    children: [
                      if (pause)
                        AxButton.icon(
                          a.state == AgentState.paused ? AxIcon.play : AxIcon.pause,
                          label: pauseWord,
                          size: 22,
                          onPressed: widget.onPause,
                        ),
                      if (pause && stop) const SizedBox(width: 3),
                      if (stop)
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
      ),
    );
  }
}

/// A mark in the first column, centred on a line of [style] at any text size.
class _Mark extends StatelessWidget {
  const _Mark(this.child, this.style);
  final Widget child;
  final TextStyle style;
  @override
  Widget build(BuildContext context) => SizedBox(
    width: 22,
    height: MediaQuery.textScalerOf(context).scale(style.fontSize!) * style.height!,
    child: Align(
      alignment: Alignment.centerLeft,
      child: SizedBox(width: 12, child: Center(child: child)),
    ),
  );
}

class TaskRow extends StatelessWidget {
  const TaskRow(this.task, {super.key});
  final TaskView task;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final (mark, word) = switch (task.state) {
      TaskState.done => (AxGlyph(AxIcon.check, size: 12, color: c.doneText), 'done'),
      TaskState.active => (const AxLamp(AgentState.running, turns: false), 'active'),
      TaskState.asking => (const AxLamp(AgentState.asking), 'asking'),
      TaskState.todo => (const AxLamp(AgentState.waiting), 'to do'),
      TaskState.blocked => (AxGlyph(AxIcon.warn, size: 12, color: c.attentionText), 'blocked'),
    };
    return Semantics(
      label: '${task.title}, $word${task.owner == null ? '' : ', ${task.owner}'}',
      excludeSemantics: true,
      child: ConstrainedBox(
        constraints: const BoxConstraints(minHeight: 25),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 11),
          child: Row(
            children: [
              _Mark(mark, t.type.label),
              Expanded(
                child: Text(
                  task.title,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: t.type.label.ink(switch (task.state) {
                    TaskState.active => c.ink1,
                    TaskState.asking => c.attentionText,
                    _ => c.ink3,
                  }),
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
      hint: busy.isNotEmpty
          ? '${busy.length} at work'
          : session.working
          ? 'at work'
          : session.age,
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
                for (final a in busy.take(4)) ...[const SizedBox(width: 4), AxLamp(a.state)],
                if (busy.isEmpty && session.working) ...[
                  const SizedBox(width: 4),
                  const AxLamp(AgentState.running),
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

/// One thing that waits for a person: who asks what, and where. It is answered there.
class NeedsRow extends StatelessWidget {
  const NeedsRow({
    required this.agent,
    required this.what,
    required this.subject,
    required this.session,
    required this.age,
    required this.onOpen,
    super.key,
  });
  final String agent, what, subject, session, age;
  final VoidCallback onOpen;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Pressable(
      label: '$agent $what: $subject, in $session',
      hint: age,
      onPressed: onOpen,
      builder: (context, s) => DecoratedBox(
        decoration: BoxDecoration(
          color: s.hovered ? c.hover : null,
          borderRadius: BorderRadius.circular(AxRadius.key),
        ),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(12, 5, 12, 6),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Padding(padding: EdgeInsets.only(top: 3), child: AxLamp(AgentState.asking)),
              const SizedBox(width: 9),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      '$agent $what',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: t.type.ui.ink(c.ink1),
                    ),
                    Text(
                      subject,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: t.type.codeSmall.ink(c.ink2),
                    ),
                    Text(
                      session,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: t.type.small.ink(c.ink3),
                    ),
                  ],
                ),
              ),
              const SizedBox(width: 8),
              Text(age, style: t.type.small.ink(c.ink3)),
            ],
          ),
        ),
      ),
    );
  }
}
