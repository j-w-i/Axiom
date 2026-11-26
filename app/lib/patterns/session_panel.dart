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
          constraints: BoxConstraints(minHeight: t.density.row + (act ? 2 : 0)),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 10),
            child: Row(
              children: [
                AxGlyph(
                  icon,
                  color: selected || s.hovered || act ? c.ink1 : c.ink2,
                  filled: selected,
                ),
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
  const SideHeading(
    this.label, {
    super.key,
    this.spoken,
    this.trailing,
    this.act,
    this.first = false,
  });

  /// Nothing stands over it.
  final bool first;
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
        padding: EdgeInsets.fromLTRB(AxSpace.row, widget.first ? 2 : 14, 2, 2),
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
                  // Its own keys stand over it: the row is still under the pointer.
                  hovered: s.hovered || _hover,
                  child: Padding(
                    padding: EdgeInsets.fromLTRB(10, 5, keys ? (pause && stop ? 66 : 38) : 10, 6),
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        _Mark(
                          a.mark == null
                              ? AxGlyph(AxIcon.comment, color: c.ink3)
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
                                  // Kept while the keys stand in its place, so that it turns on.
                                  Visibility(
                                    visible: !keys,
                                    maintainState: true,
                                    maintainAnimation: true,
                                    child: Padding(
                                      padding: const EdgeInsets.only(left: 6),
                                      child: AxLamp(a.state),
                                    ),
                                  ),
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
class PlanView extends StatelessWidget {
  const PlanView(this.tasks, {super.key, this.folded = false, this.onFold});
  final bool folded;
  final VoidCallback? onFold;
  final List<TaskView> tasks;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context);
    final done = tasks.where((k) => k.state == TaskState.done).length;
    return PanelGroup(
      'Plan',
      Text('$done of ${tasks.length}', style: t.type.small.ink(t.colors.ink3)),
      [for (final k in tasks) TaskRow(k)],
      folded: folded,
      onFold: onFold,
    );
  }
}

/// What a part of the panel beside the conversation is.
class PanelHeading extends StatelessWidget {
  const PanelHeading(
    this.label,
    this.trailing, {
    super.key,
    this.acts = false,
    this.folded = false,
    this.onFold,
  });

  /// What it is over is put away under it.
  final bool folded;
  final VoidCallback? onFold;

  /// It ends in a key, whose sign, not its ground, stands in line.
  final bool acts;
  final String label;
  final Widget? trailing;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    Widget words({bool hovered = false}) => Row(
      children: [
        Flexible(
          child: Text(
            label,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: t.type.smallStrong.ink(hovered ? c.ink1 : c.ink3),
          ),
        ),
        if (onFold != null) ...[
          const SizedBox(width: 5),
          // What is put away is always seen to be; that the rest can be, under the hand.
          if (folded || hovered) AxGlyph(folded ? AxIcon.right : AxIcon.down, color: c.ink3),
        ],
      ],
    );
    return Padding(
      padding: EdgeInsets.fromLTRB(AxSpace.card, 4, acts ? 9 : AxSpace.card, 4),
      child: ConstrainedBox(
        constraints: const BoxConstraints(minHeight: 30),
        child: Row(
          children: [
            Expanded(
              child: Semantics(
                header: true,
                child: onFold == null
                    ? words()
                    : Pressable(
                        label: label,
                        expanded: !folded,
                        onPressed: onFold,
                        builder: (context, s) => ConstrainedBox(
                          constraints: const BoxConstraints(minHeight: 26),
                          child: words(hovered: s.hovered || s.focused),
                        ),
                      ),
              ),
            ),
            ?trailing,
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
      TaskState.done => (const AxLamp(AgentState.done), 'done'),
      TaskState.active => (const AxLamp(AgentState.running, turns: false), 'active'),
      TaskState.asking => (const AxLamp(AgentState.asking), 'asking'),
      TaskState.todo => (const AxLamp(AgentState.waiting), 'to do'),
      TaskState.blocked => (AxGlyph(AxIcon.warn, color: c.attentionText), 'blocked'),
    };
    return Semantics(
      container: true,
      label: '${task.title}, $word${task.owner == null ? '' : ', ${task.owner}'}',
      excludeSemantics: true,
      child: ConstrainedBox(
        constraints: const BoxConstraints(minHeight: 29),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: AxSpace.card),
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
          padding: const EdgeInsets.fromLTRB(10, 5, 10, 6),
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

/// Rows of one kind on one raised surface, and what they are.
class PanelGroup extends StatelessWidget {
  const PanelGroup(
    this.name,
    this.trailing,
    this.rows, {
    super.key,
    this.acts = false,
    this.folded = false,
    this.onFold,
  });
  final bool acts, folded;
  final VoidCallback? onFold;
  final String name;
  final Widget? trailing;
  final List<Widget> rows;
  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      PanelHeading(name, trailing, acts: acts, folded: folded, onFold: onFold),
      if (!folded)
        Surface(
          Ax.of(context).colors.plate,
          radius: AxRadius.card,
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 4),
            child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: rows),
          ),
        ),
      const SizedBox(height: 6),
    ],
  );
}

/// A file that has been changed, and by how much. Its change is opened under it, or not.
class ChangedRow extends StatelessWidget {
  const ChangedRow(
    this.path, {
    required this.added,
    required this.removed,
    required this.open,
    required this.onPressed,
    super.key,
    this.what = 'changed',
    this.tag,
    this.quiet = false,
  });
  final String path, what;
  final int added, removed;
  final bool open;

  /// Null if there is nothing of it to be read.
  final VoidCallback? onPressed;

  /// What it is, in a word, where that is more than how it has changed.
  final String? tag;
  final bool quiet;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final dir = path.substring(0, path.lastIndexOf('/') + 1), name = path.substring(dir.length);
    return Pressable(
      label: '$path, $what, $added added, $removed removed',
      expanded: open,
      onPressed: onPressed,
      builder: (_, s) => Padding(
        padding: const EdgeInsets.fromLTRB(6, 7, 6, 7),
        child: Row(
          children: [
            SizedBox(
              width: 16,
              child: Center(
                child: AxGlyph(
                  open ? AxIcon.down : AxIcon.right,
                  color: s.hovered ? c.ink1 : c.ink3,
                ),
              ),
            ),
            const SizedBox(width: 9),
            Expanded(
              // Its own name is the last to be cut.
              child: Text.rich(
                TextSpan(
                  text: name,
                  children: [
                    if (dir.isNotEmpty)
                      TextSpan(
                        text: '  ${dir.substring(0, dir.length - 1)}',
                        style: t.type.codeSmall.ink(c.ink3),
                      ),
                  ],
                ),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: t.type.code.ink(quiet ? c.ink3 : c.ink1),
              ),
            ),
            if (tag case final w?) ...[
              const SizedBox(width: 8),
              Surface(
                c.ghost,
                radius: AxRadius.tick,
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 3),
                  child: Text(w, style: t.type.caption.ink(c.ink3)),
                ),
              ),
            ],
            const SizedBox(width: 10),
            ChangedBy(added, removed),
            if (onPressed == null) Text('No preview', style: t.type.small.ink(c.ink3)),
          ],
        ),
      ),
    );
  }
}

/// Something at work in the session that can be brought into sight.
class PanelRow extends StatelessWidget {
  const PanelRow(
    this.icon,
    this.label, {
    required this.onPressed,
    super.key,
    this.state,
    this.open = false,
  });
  final AxIcon icon;
  final String label;

  /// In a word.
  final String? state;
  final bool open;
  final VoidCallback onPressed;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 4),
      child: Pressable(
        label: state == null ? label : '$label, $state',
        toggled: open,
        onPressed: onPressed,
        builder: (context, s) => Ground(
          open
              ? c.selected
              : s.hovered
              ? c.hover
              : null,
          child: ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 30),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: AxSpace.row),
              child: Row(
                children: [
                  AxGlyph(icon, color: open || s.hovered ? c.ink1 : c.ink3, filled: open),
                  const SizedBox(width: 9),
                  Expanded(
                    child: Text(
                      label,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: t.type.code.ink(open ? c.ink1 : c.ink2),
                    ),
                  ),
                  if (state case final w?) ...[
                    const SizedBox(width: 8),
                    Text(w, style: t.type.small.ink(c.ink3)),
                  ],
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
