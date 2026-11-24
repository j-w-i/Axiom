import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

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
        constraints: const BoxConstraints(minHeight: 24),
        child: Row(
          children: [
            SizedBox(
              width: 22,
              child: Align(
                alignment: Alignment.centerLeft,
                child: AxGlyph(
                  icon,
                  size: 13,
                  color: failed ? c.faultText : icon.kind(c) ?? c.ink4,
                ),
              ),
            ),
            ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 120),
              child: Text(
                name,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: t.type.label.ink(c.ink2),
              ),
            ),
            const CopyGap(' '),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                detail,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: t.type.codeSmall.ink(c.ink3),
              ),
            ),
            CopyGap(result.isEmpty ? '' : '  '),
            const SizedBox(width: 12),
            ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 200),
              child: Text.rich(
                TextSpan(children: _tinted(result, c)),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: t.type.small.ink(failed ? c.faultText : c.ink3),
              ),
            ),
            if (more case final m?) ...[
              const SizedBox(width: 7),
              AxGlyph(m ? AxIcon.down : AxIcon.right, size: 9, color: c.ink4),
            ],
          ],
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
    final t = Ax.of(context), c = t.colors;
    final add = lines.where((l) => l.kind == DiffKind.add).length;
    final rem = lines.where((l) => l.kind == DiffKind.remove).length;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        WorkLine(icon: AxIcon.edit, name: 'Edit', detail: path, result: '+$add  −$rem'),
        const SizedBox(height: 4),
        ClipRRect(
          borderRadius: BorderRadius.circular(AxRadius.key),
          child: ColoredBox(
            color: c.block.bottom,
            child: Padding(
              padding: const EdgeInsets.symmetric(vertical: 6),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  for (final l in lines)
                    ColoredBox(
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
                              style: t.type.codeSmall.ink(c.ink4),
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
                              style: t.type.codeSmall.ink(
                                l.kind == DiffKind.add ? c.doneText : c.faultText,
                              ),
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
                    ),
                ],
              ),
            ),
          ),
        ),
      ],
    );
  }
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
  });
  final RequestView request;
  final VoidCallback onOnce, onAlways, onDeny;
  static const maxHeight = 220.0;
  @override
  State<RequestBar> createState() => _RequestBarState();
}

class _RequestBarState extends State<RequestBar> {
  final _scroll = ScrollController();
  final _focus = FocusNode(debugLabel: 'request');
  bool _focused = false;

  /// Null until laid out: not knowing is not having seen.
  bool? _seen;

  @override
  void initState() {
    super.initState();
    _scroll.addListener(_look);
  }

  @override
  void didUpdateWidget(RequestBar old) {
    super.didUpdateWidget(old);
    // Any difference makes it another request, and nothing about it has been seen.
    if (old.request != widget.request) {
      _seen = null;
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
    _scroll.dispose();
    _focus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    WidgetsBinding.instance.addPostFrameCallback((_) => _look());
    final t = Ax.of(context), c = t.colors, r = widget.request;
    final ok = _seen ?? false;
    final agent = oneLine(literal(r.agent), 60);
    final words = ConstrainedBox(
      constraints: const BoxConstraints(maxHeight: RequestBar.maxHeight),
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
                        const SizedBox(width: 9),
                        Expanded(child: Text('$agent ${literal(r.what)}', style: t.type.uiStrong)),
                      ],
                    ),
                    const SizedBox(height: 3),
                    Text(literal(r.subject), style: t.type.code.ink(c.attentionText)),
                    const SizedBox(height: 3),
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
      ],
    );
    final keys = Wrap(
      spacing: 6,
      runSpacing: 6,
      alignment: WrapAlignment.end,
      children: [
        AxButton(label: 'Deny', tone: AxTone.quiet, onPressed: widget.onDeny),
        AxButton(label: oneLine(literal(r.always), 80), onPressed: ok ? widget.onAlways : null),
        AxButton(label: 'Allow once', tone: AxTone.primary, onPressed: ok ? widget.onOnce : null),
      ],
    );
    return Semantics(
      container: true,
      liveRegion: true,
      label: 'Permission request from $agent',
      explicitChildNodes: true,
      child: Surface(
        c.plate,
        child: Stack(
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(12, 9, 12, 10),
              child: LayoutBuilder(
                builder: (context, box) => box.maxWidth < 620
                    ? Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          body,
                          const SizedBox(height: 8),
                          Align(alignment: Alignment.centerRight, child: keys),
                        ],
                      )
                    : Row(
                        children: [
                          Expanded(child: body),
                          const SizedBox(width: 16),
                          ConstrainedBox(
                            constraints: BoxConstraints(maxWidth: box.maxWidth * .5),
                            child: keys,
                          ),
                        ],
                      ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
