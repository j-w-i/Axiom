import 'package:flutter/semantics.dart';
import 'package:flutter/widgets.dart';

import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/marks.dart';
import '../ui/press.dart';
import '../ui/status.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';
import 'models.dart';

/// The ground of a row in the sidebar.
Widget _ground(
  AxColors c, {
  required bool selected,
  required bool hovered,
  required Widget child,
  bool key = false,
}) => selected || key
    ? Surface(selected ? c.slab : c.ghost, radius: AxRadius.key, child: child)
    : Ground(hovered ? c.hover : null, child: child);

/// Somewhere to go: what it is and the keys that go there.
class NavRow extends StatelessWidget {
  const NavRow(
    this.icon,
    this.label, {
    required this.onPressed,
    super.key,
    this.keys,
    this.selected = false,
    this.act = false,
  });

  /// It begins something, and is a key at rest too.
  final bool act;
  final AxIcon icon;
  final String label;
  final String? keys;
  final bool selected;
  final VoidCallback onPressed;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Pressable(
      label: label,
      hint: keys,
      selected: selected ? true : null,
      onPressed: onPressed,
      builder: (context, s) => _ground(
        c,
        selected: selected,
        hovered: s.hovered,
        key: act,
        child: ConstrainedBox(
          constraints: BoxConstraints(minHeight: t.density.row + (act ? 4 : 0)),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 10),
            child: Row(
              children: [
                AxGlyph(icon, size: 17, color: selected || s.hovered || act ? c.ink1 : c.ink2),
                const SizedBox(width: 10),
                Expanded(
                  child: Text(
                    label,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: (act ? t.type.uiMedium : t.type.ui).ink(
                      selected || s.hovered || act ? c.ink1 : c.ink2,
                    ),
                  ),
                ),
                // Said to whoever hears it, and shown to whoever is here.
                if (keys case final k? when s.hovered || s.focused)
                  Text(k, style: t.type.small.ink(c.ink3)),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// What the rows under it have in common: a folder, or a state.
class SideHeading extends StatefulWidget {
  const SideHeading(this.label, {super.key, this.spoken, this.trailing, this.act});
  final String label;
  final String? spoken;

  /// Always there; only when the hand or the keyboard is.
  final Widget? trailing, act;
  @override
  State<SideHeading> createState() => _SideHeadingState();
}

class _SideHeadingState extends State<SideHeading> {
  bool _over = false, _within = false;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final spoken = widget.spoken;
    return MouseRegion(
      onEnter: (_) => setState(() => _over = true),
      onExit: (_) => setState(() => _over = false),
      child: Padding(
        padding: const EdgeInsets.fromLTRB(10, 18, 2, 3),
        child: ConstrainedBox(
          constraints: const BoxConstraints(minHeight: 26),
          child: Row(
            children: [
              Expanded(
                child: Semantics(
                  header: true,
                  label: spoken,
                  excludeSemantics: spoken != null,
                  child: Text(
                    widget.label,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: t.type.smallStrong.ink(c.ink4),
                  ),
                ),
              ),
              if (widget.act case final a?)
                Focus(
                  canRequestFocus: false,
                  skipTraversal: true,
                  onFocusChange: (v) => setState(() => _within = v),
                  child: Opacity(
                    opacity: _over || _within ? 1 : 0,
                    alwaysIncludeSemantics: true,
                    child: a,
                  ),
                ),
              ?widget.trailing,
            ],
          ),
        ),
      ),
    );
  }
}

/// The open session, where it stands among the others: its name, and its agents under it.
class SessionPanel extends StatelessWidget {
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

  /// The agent that is shown, if one is.
  final String? selected;
  final ValueChanged<String> onSelect;
  final VoidCallback? onAddAgent;
  final ValueChanged<String>? onPause, onStop;

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), s = session;
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: 'Session ${s.title}',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          ConstrainedBox(
            constraints: BoxConstraints(minHeight: t.density.row),
            child: Padding(
              padding: const EdgeInsets.only(left: 10, right: 2),
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
                  if (onAddAgent case final f?)
                    AxButton.icon(
                      AxIcon.plus,
                      label: 'Add an agent to ${s.title}',
                      tone: AxTone.quiet,
                      size: 28,
                      onPressed: f,
                    ),
                ],
              ),
            ),
          ),
          for (final a in s.agents)
            AgentRow(
              a,
              selected: a.id == selected,
              onSelect: () => onSelect(a.id),
              onPause: onPause == null ? null : () => onPause!(a.id),
              onStop: onStop == null ? null : () => onStop!(a.id),
            ),
          const SizedBox(height: 8),
        ],
      ),
    );
  }
}

/// One agent: whose it is, who, how it stands, and what it does. Its keys appear when the hand is
/// here.
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
    final live = a.state == AgentState.running || a.state == AgentState.asking;
    final now = a.recent.lastOrNull;
    final pause =
        widget.onPause != null && (a.state == AgentState.running || a.state == AgentState.paused);
    final stop = widget.onStop != null && live;
    final pauseWord = '${a.state == AgentState.paused ? 'Resume' : 'Pause'} ${a.name}';
    final keys = (_hover || _within) && (pause || stop);
    // What it does is said while that matters: at rest its name and model are enough.
    final says =
        now != null && (live || a.state == AgentState.failed || a.state == AgentState.paused);
    final does = TextSpan(
      text: says ? now.text : null,
      style: !says
          ? null
          : now.failed || a.state == AgentState.failed
          ? TextStyle(color: c.faultText)
          : a.state == AgentState.asking
          ? TextStyle(color: c.attentionText)
          : null,
    );
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
                onPressed: widget.onSelect,
                actions: {
                  if (pause) CustomSemanticsAction(label: pauseWord): widget.onPause!,
                  if (stop) CustomSemanticsAction(label: 'Stop ${a.name}'): widget.onStop!,
                },
                builder: (context, s) => _ground(
                  c,
                  selected: widget.selected,
                  hovered: s.hovered,
                  child: Padding(
                    padding: EdgeInsets.fromLTRB(10, 7, keys ? (pause && stop ? 66 : 38) : 10, 7),
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        _Mark(
                          a.mark == null
                              ? AxGlyph(AxIcon.comment, size: 15, color: c.ink3)
                              : AxServiceMark(a.mark!, size: 16),
                          t.type.uiMedium,
                        ),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.stretch,
                            children: [
                              Row(
                                children: [
                                  Expanded(
                                    child: Text.rich(
                                      TextSpan(
                                        children: [
                                          TextSpan(text: a.name),
                                          if (a.model.isNotEmpty)
                                            TextSpan(
                                              text: '   ${a.model}',
                                              style: t.type.small.ink(c.ink3),
                                            ),
                                        ],
                                      ),
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: t.type.uiMedium,
                                    ),
                                  ),
                                  if (!keys) ...[const SizedBox(width: 6), AxLamp(a.state)],
                                ],
                              ),
                              if (says)
                                Text.rich(
                                  does,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: t.type.small.ink(c.ink3),
                                ),
                            ],
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
              if (keys)
                Positioned(
                  right: 6,
                  child: Row(
                    children: [
                      if (pause)
                        AxButton.icon(
                          a.state == AgentState.paused ? AxIcon.play : AxIcon.pause,
                          label: pauseWord,
                          tone: AxTone.quiet,
                          size: 28,
                          onPressed: widget.onPause,
                        ),
                      if (pause && stop) const SizedBox(width: 2),
                      if (stop)
                        AxButton.icon(
                          AxIcon.stop,
                          label: 'Stop ${a.name}',
                          tone: AxTone.quiet,
                          size: 28,
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

/// A mark in a column of its own, centred on a line of [style] at any text size.
class _Mark extends StatelessWidget {
  const _Mark(this.child, this.style);
  final Widget child;
  final TextStyle style;
  @override
  Widget build(BuildContext context) => SizedBox(
    width: 26,
    height: MediaQuery.textScalerOf(context).scale(style.fontSize!) * style.height!,
    child: Align(
      alignment: Alignment.centerLeft,
      child: SizedBox(width: 16, child: Center(child: child)),
    ),
  );
}

/// What the session means to do: how far it is, its parts, how they stand and whose they are.
(String, Widget?, List<Widget>) planGroup(BuildContext context, List<TaskView> tasks) {
  final t = Ax.of(context);
  final done = tasks.where((k) => k.state == TaskState.done).length;
  return (
    'Plan',
    Text('$done of ${tasks.length}', style: t.type.small.ink(t.colors.ink3)),
    [for (final k in tasks) TaskRow(k)],
  );
}

/// What a part of the panel beside the conversation is.
class PanelHeading extends StatelessWidget {
  const PanelHeading(this.label, this.trailing, {super.key});
  final String label;
  final Widget? trailing;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(14, 12, 14, 8),
      child: Row(
        children: [
          Expanded(
            child: Semantics(
              header: true,
              child: Text(label, style: t.type.smallStrong.ink(t.colors.ink3)),
            ),
          ),
          ?trailing,
        ],
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
      TaskState.done => (const AxLamp(AgentState.done), 'done'),
      TaskState.active => (const AxLamp(AgentState.running, turns: false), 'active'),
      TaskState.asking => (const AxLamp(AgentState.asking), 'asking'),
      TaskState.todo => (const AxLamp(AgentState.waiting), 'to do'),
      TaskState.blocked => (AxGlyph(AxIcon.warn, size: 13, color: c.attentionText), 'blocked'),
    };
    return Semantics(
      container: true,
      label: '${task.title}, $word${task.owner == null ? '' : ', ${task.owner}'}',
      excludeSemantics: true,
      child: ConstrainedBox(
        constraints: const BoxConstraints(minHeight: 32),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14),
          child: Row(
            children: [
              _Mark(mark, t.type.ui),
              Expanded(
                child: Text(
                  task.title,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: switch (task.state) {
                    TaskState.active || TaskState.asking => t.type.uiMedium,
                    TaskState.done => t.type.ui.ink(c.ink3),
                    _ => t.type.ui.ink(c.ink2),
                  },
                ),
              ),
              if (task.owner != null && task.state != TaskState.done) ...[
                const SizedBox(width: 8),
                Text(task.owner!, style: t.type.small.ink(c.ink3)),
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
      builder: (context, s) => _ground(
        c,
        selected: false,
        hovered: s.hovered,
        child: ConstrainedBox(
          constraints: BoxConstraints(minHeight: t.density.row),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 10),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    session.title,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: t.type.ui.ink(s.hovered ? c.ink1 : c.ink2),
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
    this.state = AgentState.asking,
  });
  final AgentState state;
  final String agent, what, subject, session, age;
  final VoidCallback onOpen;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Pressable(
      label: subject.isEmpty ? '$agent $what, in $session' : '$agent $what: $subject, in $session',
      hint: age,
      onPressed: onOpen,
      builder: (context, s) => _ground(
        c,
        selected: false,
        hovered: s.hovered,
        child: Padding(
          padding: const EdgeInsets.fromLTRB(10, 7, 10, 8),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              _Mark(AxLamp(state), t.type.uiMedium),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      '$agent $what',
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                      style: t.type.uiMedium,
                    ),
                    if (subject.isNotEmpty)
                      Text(
                        subject,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: state == AgentState.asking
                            ? t.type.codeSmall.ink(c.ink2)
                            : t.type.small.ink(c.ink2),
                      ),
                    Row(
                      children: [
                        Expanded(
                          child: Text(
                            session,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: t.type.small.ink(c.ink3),
                          ),
                        ),
                        if (age.isNotEmpty) ...[
                          const SizedBox(width: 8),
                          Text(age, style: t.type.small.ink(c.ink4)),
                        ],
                      ],
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

/// A part beside the conversation, with the groove that parts them. Each group is one raised surface.
class SidePanel extends StatelessWidget {
  const SidePanel({required this.groups, super.key});

  /// What each is called, what stands at the end of its name, and its rows.
  final List<(String, Widget?, List<Widget>)> groups;
  static const width = 312.0;
  @override
  Widget build(BuildContext context) => SizedBox(
    width: width,
    child: ListView(
      padding: const EdgeInsets.fromLTRB(4, 0, 18, 20),
      children: [
        for (final (name, trailing, rows) in groups) ...[
          PanelHeading(name, trailing),
          Surface(
            Ax.of(context).colors.plate,
            radius: AxRadius.card,
            child: Padding(
              padding: const EdgeInsets.symmetric(vertical: 5),
              child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: rows),
            ),
          ),
          const SizedBox(height: 8),
        ],
      ],
    ),
  );
}

/// A file that has been changed, and by how much.
class ChangedRow extends StatelessWidget {
  const ChangedRow(
    this.path, {
    required this.added,
    required this.removed,
    required this.onOpen,
    super.key,
  });
  final String path;
  final int added, removed;
  final VoidCallback onOpen;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 5),
      child: Pressable(
        label: '$path, $added added, $removed removed',
        onPressed: onOpen,
        builder: (context, s) => _ground(
          c,
          selected: false,
          hovered: s.hovered,
          child: ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 32),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 7),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      path,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: t.type.codeSmall.ink(s.hovered ? c.ink1 : c.ink2),
                    ),
                  ),
                  const SizedBox(width: 8),
                  ChangedBy(added, removed),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// `+7 −2` in the colours of what they count; what is nothing is not said.
class ChangedBy extends StatelessWidget {
  const ChangedBy(this.added, this.removed, {super.key});
  final int added, removed;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (added > 0) Text('+$added', style: t.type.codeSmall.ink(c.doneText)),
        if (added > 0 && removed > 0) const SizedBox(width: 7),
        if (removed > 0) Text('−$removed', style: t.type.codeSmall.ink(c.faultText)),
      ],
    );
  }
}
