import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../ui/commands.dart';
import '../ui/controls.dart';
import '../transcript/safe_text.dart';
import '../ui/editable.dart';
import '../ui/icons.dart';
import '../ui/press.dart';
import '../ui/surface.dart';
import '../ui/status.dart';
import '../ui/tokens.dart';
import 'highlight.dart';
import 'models.dart';

/// One line of work: what was done, to what, with what result. [detail] is mono because it is
/// always a literal: a path, a pattern or a command.
class WorkLine extends StatelessWidget {
  const WorkLine({
    required this.icon,
    required this.name,
    required this.detail,
    super.key,
    this.result = '',
    this.failed = false,
    this.more,
  });
  final AxIcon icon;
  final String name, detail, result;
  final bool failed;

  /// Whether more is shown below; null if there is nothing more.
  final bool? more;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Semantics(
      label: '$name $detail $result',
      excludeSemantics: true,
      child: ConstrainedBox(
        constraints: const BoxConstraints(minHeight: 26),
        child: LayoutBuilder(
          builder: (context, box) => Row(
            children: [
              SizedBox(
                width: 26,
                child: Align(
                  alignment: Alignment.centerLeft,
                  child: AxGlyph(icon, color: failed ? c.faultText : c.ink3),
                ),
              ),
              ConstrainedBox(
                constraints: BoxConstraints(maxWidth: math.min(190, box.maxWidth * .34)),
                child: Text(
                  name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: t.type.label.ink(c.ink3),
                ),
              ),
              const CopyGap(' '),
              const SizedBox(width: 8),
              // What came of it stands by what was done.
              Flexible(
                child: Text(
                  detail,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: t.type.code.copyWith(height: 1.4, color: c.ink2),
                ),
              ),
              CopyGap(result.isEmpty ? '' : '  '),
              if (result.isNotEmpty) const SizedBox(width: 10),
              ConstrainedBox(
                constraints: BoxConstraints(maxWidth: math.min(240, box.maxWidth * .34)),
                child: Text.rich(
                  TextSpan(children: _tinted(result, c)),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: t.type.label.ink(failed ? c.faultText : c.ink3),
                ),
              ),
              if (more case final m?) ...[
                const SizedBox(width: 7),
                AxGlyph(m ? AxIcon.down : AxIcon.right, color: c.ink3),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

/// `+7 −2` in the colours of what they count.
List<InlineSpan> _tinted(String s, AxColors c) {
  final out = <InlineSpan>[];
  var p = 0;
  for (final m in RegExp(r'(?<!\S)([+−-])\d+(?!\S)').allMatches(s)) {
    if (m.start > p) out.add(TextSpan(text: s.substring(p, m.start)));
    out.add(
      TextSpan(
        text: m[0],
        style: TextStyle(color: m[1] == '+' ? c.doneText : c.faultText),
      ),
    );
    p = m.end;
  }
  if (p < s.length) out.add(TextSpan(text: s.substring(p)));
  return out;
}

class DiffBlock extends StatelessWidget {
  const DiffBlock({required this.path, required this.lines, super.key});
  final String path;
  final List<DiffLine> lines;
  @override
  Widget build(BuildContext context) {
    final add = lines.where((l) => l.kind == DiffKind.add).length;
    final rem = lines.where((l) => l.kind == DiffKind.remove).length;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        WorkLine(
          icon: AxIcon.edit,
          name: 'Edit',
          detail: path,
          result: [if (add > 0) '+$add', if (rem > 0) '−$rem'].join('  '),
        ),
        const SizedBox(height: 4),
        DiffLines(lines),
      ],
    );
  }
}

/// The lines of one file's change.
class DiffLines extends StatelessWidget {
  const DiffLines(this.lines, {super.key, this.onLine, this.at, this.note});
  final List<DiffLine> lines;

  /// One of them was pressed.
  final ValueChanged<DiffLine>? onLine;

  /// What stands under one of them.
  final DiffLine? at;
  final Widget? note;

  Widget _line(AxTheme t, DiffLine l) {
    final c = t.colors;
    final line = ColoredBox(
      color: switch (l.kind) {
        DiffKind.add => c.diffAdd,
        DiffKind.remove => c.diffRemove,
        DiffKind.same => const Color(0x00000000),
      },
      child: Row(
        children: [
          SizedBox(
            width: 34,
            child: Text(
              '${l.number}',
              textAlign: TextAlign.right,
              style: t.type.codeSmall.ink(c.ink3),
            ),
          ),
          SizedBox(
            width: 22,
            child: Text(
              switch (l.kind) {
                DiffKind.add => '+',
                DiffKind.remove => '−',
                DiffKind.same => '',
              },
              textAlign: TextAlign.center,
              style: t.type.codeSmall.ink(l.kind == DiffKind.add ? c.doneText : c.faultText),
            ),
          ),
          Expanded(
            child: Text.rich(
              highlight(literal(l.text), t.type.codeSmall, c),
              style: t.type.codeSmall,
              maxLines: 1,
              softWrap: false,
              overflow: TextOverflow.fade,
            ),
          ),
        ],
      ),
    );
    if (onLine == null) return line;
    // The keyboard's way is the file's own button: hundreds of stops would be no way at all.
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        excludeFromSemantics: true,
        onTap: () => onLine!(l),
        child: line,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return ClipPath(
      clipper: const ContinuousClip(AxRadius.block),
      child: ColoredBox(
        color: c.block.fill,
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: 6),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (note != null && at == null) note!,
              for (final l in lines) ...[_line(t, l), if (note != null && identical(l, at)) note!],
            ],
          ),
        ),
      ),
    );
  }
}

/// One file of a diff: its first lines, how many were added and removed in all, and how many lines
/// are not here.
class FileDiff {
  FileDiff(this.path);
  final String path;
  final lines = <DiffLine>[];
  int added = 0, removed = 0, left = 0;
}

/// A unified diff, file by file. How long a part is says itself, so that a removed line that begins
/// with dashes is not taken for another file.
/// Lines beyond [most] of one file, or [atAll] of all, are only counted.
List<FileDiff> readDiffs(String text, {int most = 400, int atAll = 20000}) {
  final files = <FileDiff>[];
  FileDiff? f;
  String? from;
  var was = 0, now = 0, olds = 0, news = 0, shown = 0;
  for (final l in text.split('\n')) {
    if (olds > 0 || news > 0) {
      final kind = switch (l.isEmpty ? ' ' : l[0]) {
        '+' => DiffKind.add,
        '-' => DiffKind.remove,
        ' ' => DiffKind.same,
        _ => null,
      };
      // "\ No newline at end of file".
      if (kind == null || f == null) continue;
      if (f.lines.length >= most || shown >= atAll) {
        f.left++;
      } else {
        shown++;
        f.lines.add(
          DiffLine(
            kind,
            kind == DiffKind.remove ? was : now,
            l.isEmpty ? '' : cutTo(l.substring(1), 1000),
          ),
        );
      }
      if (kind != DiffKind.add) {
        was++;
        olds--;
      }
      if (kind != DiffKind.remove) {
        now++;
        news--;
      }
      if (kind == DiffKind.add) f.added++;
      if (kind == DiffKind.remove) f.removed++;
      continue;
    }
    if (l.startsWith('--- ')) {
      from = l.substring(4);
    } else if (l.startsWith('+++ ') && from != null) {
      final to = l.substring(4);
      final gone = to == '/dev/null' || to.startsWith('(');
      // Git ends a name that has spaces in it with a tab.
      final name = gone ? from : to;
      f = FileDiff(name.endsWith('\t') ? name.substring(0, name.length - 1) : name);
      files.add(f);
      from = null;
    } else if (RegExp(r'^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@').firstMatch(l) case final m?) {
      was = int.parse(m[1]!);
      olds = int.parse(m[2] ?? '1');
      now = int.parse(m[3]!);
      news = int.parse(m[4] ?? '1');
    }
  }
  return files;
}

/// A permission request. The subject is exact, whole and spelled out: what cannot be seen is escaped,
/// nothing is cut off, and if it is too long to show at once, saying yes waits until it has been
/// scrolled through (SI-15).
class RequestBar extends StatefulWidget {
  const RequestBar(
    this.request, {
    required this.onOnce,
    required this.onAlways,
    required this.onDeny,
    super.key,
    this.room = maxHeight,
    this.onDenyWith,
    this.waiting = 0,
  });

  /// How many others wait after it. Not part of the request: another count is the same question.
  final int waiting;

  /// No, and why.
  final ValueChanged<String>? onDenyWith;
  final RequestView request;
  final VoidCallback onOnce, onAlways, onDeny;

  /// How high its words may be, if that is less than [maxHeight].
  final double room;
  static const maxHeight = 220.0;
  @override
  State<RequestBar> createState() => _RequestBarState();
}

class _RequestBarState extends State<RequestBar> {
  final _scroll = ScrollController();
  final _focus = FocusNode(debugLabel: 'request');
  final _why = TextEditingController(), _whyFocus = FocusNode(debugLabel: 'why not');
  bool _focused = false, _says = false;

  /// Null until laid out: not knowing is not having seen.
  bool? _seen;

  /// A press meant for the question before this one is not an answer to it.
  bool _settled = false;
  Timer? _settling;
  void _settle() {
    _settled = false;
    _settling?.cancel();
    _settling = Timer(AxMotion.settle, () {
      if (mounted) setState(() => _settled = true);
    });
  }

  @override
  void initState() {
    super.initState();
    _scroll.addListener(_look);
    _settle();
  }

  @override
  void didUpdateWidget(RequestBar old) {
    super.didUpdateWidget(old);
    // Any difference makes it another request, and nothing about it has been seen.
    if (old.request != widget.request) {
      _seen = null;
      _settle();
      _says = false;
      _why.clear();
      if (_scroll.hasClients) _scroll.jumpTo(0);
    }
  }

  void _look() {
    if (!mounted || !_scroll.hasClients || _seen == true) return;
    final p = _scroll.position;
    final seen = p.pixels >= p.maxScrollExtent - 1;
    if (seen != _seen) setState(() => _seen = seen);
  }

  void _by(double viewports) {
    final p = _scroll.position;
    _scroll.jumpTo(
      (p.pixels + viewports * p.viewportDimension).clamp(p.minScrollExtent, p.maxScrollExtent),
    );
  }

  @override
  void dispose() {
    _settling?.cancel();
    _scroll.dispose();
    _focus.dispose();
    _why.dispose();
    _whyFocus.dispose();
    super.dispose();
  }

  void _say(bool on) {
    setState(() => _says = on);
    if (on) _whyFocus.requestFocus();
  }

  void _denyWith() {
    // While an input method is composing, Enter is its own.
    final composing = _why.value.composing;
    if (composing.isValid && !composing.isCollapsed) return;
    final why = _why.text.trim();
    why.isEmpty ? widget.onDeny() : widget.onDenyWith!(why);
  }

  @override
  Widget build(BuildContext context) {
    WidgetsBinding.instance.addPostFrameCallback((_) => _look());
    final t = Ax.of(context), c = t.colors, r = widget.request;
    final ok = _seen ?? false;
    final agent = oneLine(literal(r.agent), 60);
    final words = ConstrainedBox(
      constraints: BoxConstraints(maxHeight: math.min(RequestBar.maxHeight, widget.room)),
      child: NotificationListener<ScrollMetricsNotification>(
        onNotification: (_) {
          WidgetsBinding.instance.addPostFrameCallback((_) => _look());
          return false;
        },
        child: CallbackShortcuts(
          bindings: {
            const SingleActivator(LogicalKeyboardKey.arrowDown): () => _by(.2),
            const SingleActivator(LogicalKeyboardKey.arrowUp): () => _by(-.2),
            const SingleActivator(LogicalKeyboardKey.pageDown): () => _by(.9),
            const SingleActivator(LogicalKeyboardKey.pageUp): () => _by(-.9),
            const SingleActivator(LogicalKeyboardKey.space): () => _by(.9),
            const SingleActivator(LogicalKeyboardKey.end): () => _by(1e6),
            const SingleActivator(LogicalKeyboardKey.home): () => _by(-1e6),
          },
          child: Focus(
            focusNode: _focus,
            // A stop on the way to the keys only if there is something to scroll.
            canRequestFocus: !ok || _focused,
            onFocusChange: (v) => setState(() => _focused = v),
            child: CustomPaint(
              foregroundPainter: _focused ? FocusRing(c.focus, 0) : null,
              child: SingleChildScrollView(
                controller: _scroll,
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Row(
                      children: [
                        const AxLamp(AgentState.asking),
                        const SizedBox(width: 10),
                        Expanded(child: Text('$agent ${literal(r.what)}', style: t.type.uiStrong)),
                        if (widget.waiting > 0) ...[
                          const SizedBox(width: 12),
                          Text('${widget.waiting} more waiting', style: t.type.small.ink(c.ink3)),
                        ],
                      ],
                    ),
                    const SizedBox(height: 9),
                    Surface(
                      c.block,
                      radius: AxRadius.key,
                      child: SizedBox(
                        width: double.infinity,
                        child: Padding(
                          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
                          child: Text(literal(r.subject), style: t.type.code),
                        ),
                      ),
                    ),
                    const SizedBox(height: 8),
                    Text(literal(r.scope), style: t.type.label.ink(c.ink3)),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
    final body = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        words,
        if (_seen == false) ...[
          const SizedBox(height: 5),
          Text('There is more. Scroll to the end to allow this.', style: t.type.label.ink(c.ink2)),
        ],
        if (_says) ...[
          const SizedBox(height: 8),
          CallbackShortcuts(
            bindings: {
              const SingleActivator(LogicalKeyboardKey.enter): _denyWith,
              const SingleActivator(LogicalKeyboardKey.escape): () => _say(false),
            },
            child: AxField(
              label: 'Reason',
              controller: _why,
              focusNode: _whyFocus,
              placeholder: 'What it should do instead',
            ),
          ),
        ],
      ],
    );
    Widget group(List<Widget> all) => Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        for (final (i, k) in all.indexed) ...[
          if (i > 0) const SizedBox(width: 6),
          Flexible(child: k),
        ],
      ],
    );
    // If they break, then between no and yes: the main answer is never left alone.
    final keys = Wrap(
      spacing: 8,
      runSpacing: 6,
      alignment: WrapAlignment.end,
      children: [
        group([
          if (widget.onDenyWith != null)
            _says
                ? AxButton(label: 'Back', tone: AxTone.quiet, onPressed: () => _say(false))
                : AxButton(
                    label: 'Deny with reason',
                    tone: AxTone.quiet,
                    onPressed: () => _say(true),
                  ),
          AxButton(label: 'Deny', onPressed: _says ? _denyWith : widget.onDeny),
        ]),
        if (!_says)
          group([
            if (r.always case final a?)
              AxButton(label: oneLine(literal(a), 80), onPressed: ok ? widget.onAlways : null),
            AxButton(
              label: 'Allow once',
              tone: AxTone.primary,
              onPressed: ok ? widget.onOnce : null,
            ),
          ]),
      ],
    );
    return OnCommand(
      Command.allow,
      // Only what has been seen to its end, and been there long enough to be meant.
      ok && _settled && !_says ? widget.onOnce : null,
      child: OnCommand(
        Command.deny,
        _settled ? (_says ? _denyWith : widget.onDeny) : null,
        child: Semantics(
          container: true,
          liveRegion: true,
          label: 'Permission request from $agent',
          explicitChildNodes: true,
          child: Settle(
            stamp: r,
            fresh: true,
            child: Surface(
              c.plate,
              radius: AxRadius.card,
              child: Padding(
                padding: const EdgeInsets.all(AxSpace.card),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    body,
                    const SizedBox(height: 10),
                    Align(alignment: Alignment.centerRight, child: keys),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
