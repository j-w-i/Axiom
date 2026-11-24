import 'package:flutter/widgets.dart';

import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/status.dart';
import '../ui/surface.dart';
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
  });
  final AxIcon icon;
  final String name, detail, result;
  final bool failed;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Semantics(
      label: '$name $detail $result',
      excludeSemantics: true,
      child: SizedBox(
        height: 24,
        child: Row(
          children: [
            AxGlyph(icon, size: 13, color: failed ? c.faultText : c.ink4),
            const SizedBox(width: 9),
            Text(name, style: t.type.label.ink(c.ink2)),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                detail,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: t.type.codeSmall.ink(c.ink3),
              ),
            ),
            const SizedBox(width: 8),
            Text(result, style: t.type.small.ink(failed ? c.faultText : c.ink3)),
          ],
        ),
      ),
    );
  }
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
                              highlight(l.text, t.type.codeSmall, c),
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

/// A permission request. The subject is exact; the scope says what saying yes allows.
class RequestBar extends StatelessWidget {
  const RequestBar(
    this.request, {
    required this.onOnce,
    required this.onAlways,
    required this.onDeny,
    super.key,
  });
  final RequestView request;
  final VoidCallback onOnce, onAlways, onDeny;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final words = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Text.rich(
          TextSpan(
            children: [
              TextSpan(text: '${request.agent} ${request.what}', style: t.type.uiStrong),
              TextSpan(text: '   ${request.scope}', style: t.type.label.ink(c.ink3)),
            ],
          ),
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
        ),
        const SizedBox(height: 2),
        Text(
          request.subject,
          maxLines: 2,
          overflow: TextOverflow.ellipsis,
          style: t.type.code.ink(c.attentionText),
        ),
      ],
    );
    final keys = Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        AxButton(label: 'Deny', tone: AxTone.quiet, onPressed: onDeny),
        const SizedBox(width: 4),
        AxButton(label: request.always, onPressed: onAlways),
        const SizedBox(width: 6),
        AxButton(label: 'Allow once', tone: AxTone.answer, onPressed: onOnce),
      ],
    );
    return Semantics(
      container: true,
      liveRegion: true,
      label: 'Permission request from ${request.agent}',
      explicitChildNodes: true,
      child: Surface(
        c.request,
        child: Padding(
          padding: const EdgeInsets.fromLTRB(14, 9, 10, 10),
          child: LayoutBuilder(
            builder: (context, box) => box.maxWidth < 620
                ? Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      words,
                      const SizedBox(height: 8),
                      Align(alignment: Alignment.centerRight, child: keys),
                    ],
                  )
                : Row(
                    children: [
                      const AxLamp(AgentState.asking),
                      const SizedBox(width: 12),
                      Expanded(child: words),
                      const SizedBox(width: 12),
                      keys,
                    ],
                  ),
          ),
        ),
      ),
    );
  }
}
