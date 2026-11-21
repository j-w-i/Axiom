// The Visual Inspector (M5-UI-001; ADR-0019, ADR-0020 §8), in the side bar beside the editor:
// the folder's preview (start, open in the browser, stop), inspect mode, and the element the
// person selected in the page: its hierarchy, source (exact locations open at line and column;
// guesses say so), computed properties, "Ask AI about this element", and the latest
// verification an agent ran.

import 'dart:async';

import 'package:flutter/widgets.dart';

import '../app/app_state.dart';
import '../app/open_link.dart';
import '../daemon/protocol.g.dart';
import '../state/inspector.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/theme.dart';
import '../ui/tokens.dart';

/// Lines of source attached with "Ask AI about this element" (from the element's line).
const askContextLines = 20;

class InspectorSide extends StatelessWidget {
  const InspectorSide({super.key});

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final s = app.inspector;
    return ListenableBuilder(
      listenable: Listenable.merge([s, app.workspace]),
      builder: (context, _) {
        final t = context.axiom;
        if (!app.workspace.isOpen) {
          return Padding(
            padding: const EdgeInsets.all(Metrics.s5),
            child: Text('Open a folder to preview and inspect its app.', style: t.type.small),
          );
        }
        final shown = s.shown;
        return Semantics(
          container: true,
          explicitChildNodes: true,
          label: 'Inspector',
          child: ListView(
            padding: const EdgeInsets.fromLTRB(Metrics.s5, Metrics.s2, Metrics.s5, Metrics.s5),
            children: [
              _PreviewSection(store: s),
              if (s.running) ...[
                const SizedBox(height: Metrics.s4),
                AxToggle(
                  label: 'Inspect mode',
                  value: s.inspectMode,
                  onChanged: s.connected ? (v) => unawaited(s.setInspectMode(v)) : null,
                ),
                const SizedBox(height: Metrics.s2),
                Text(
                  s.inspectMode
                      ? 'Point at the page to highlight an element; click to select it.'
                      : 'Turn on to select an element in the page.',
                  style: t.type.tiny,
                ),
                if (s.inspectMode && s.hovered != null)
                  Padding(
                    padding: const EdgeInsets.only(top: Metrics.s2),
                    child: Semantics(
                      liveRegion: true,
                      child: Text(
                        'Under the pointer: ${nodeLabel(s.hovered!)}',
                        style: t.type.small,
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                  ),
              ],
              if (shown != null) ...[
                const SizedBox(height: Metrics.s5),
                _Selection(store: s, node: shown),
              ],
              if (s.report != null) ...[
                const SizedBox(height: Metrics.s5),
                _Report(report: s.report!.report),
              ],
            ],
          ),
        );
      },
    );
  }
}

class _Heading extends StatelessWidget {
  const _Heading(this.text);
  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(bottom: Metrics.s2),
    child: Semantics(header: true, child: Text(text, style: context.axiom.type.label)),
  );
}

class _PreviewSection extends StatelessWidget {
  const _PreviewSection({required this.store});
  final InspectorStore store;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final s = store;
    final p = s.preview;
    final children = <Widget>[const _Heading('Preview')];
    if (s.starting) {
      children.add(const Spinner(label: 'Starting the preview'));
    } else if (p == null || p.state == PreviewState.failed) {
      children.addAll([
        Text(
          'Runs the app’s dev server with Axiom’s inspector. The page opens in your browser.',
          style: t.type.small,
        ),
        const SizedBox(height: Metrics.s3),
        Align(
          alignment: Alignment.centerLeft,
          child: AxButton(
            label: 'Start preview',
            icon: AxiomIcons.play,
            onPressed: () => unawaited(s.start()),
          ),
        ),
        if (p?.message != null) ...[
          const SizedBox(height: Metrics.s3),
          Semantics(
            liveRegion: true,
            child: Text(p!.message!, style: t.type.small.copyWith(color: c.dangerText)),
          ),
          if (p.output != null && p.output!.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(top: Metrics.s2),
              child: Text(p.output!, style: t.type.tiny, maxLines: 8),
            ),
        ],
      ]);
    } else {
      final url = p.url;
      children.addAll([
        Row(
          children: [
            StatusDot(s.connected ? DotState.ok : DotState.warn),
            const SizedBox(width: Metrics.s3),
            Expanded(
              child: Semantics(
                liveRegion: true,
                child: Text(
                  s.connected ? 'Page connected' : 'Waiting for the page: open it in a browser',
                  style: t.type.small,
                ),
              ),
            ),
          ],
        ),
        if (url != null) ...[
          const SizedBox(height: Metrics.s2),
          Text(url, style: t.type.tiny, overflow: TextOverflow.ellipsis),
        ],
        const SizedBox(height: Metrics.s3),
        Row(
          children: [
            if (url != null)
              AxButton(
                label: 'Open in browser',
                icon: AxiomIcons.link,
                onPressed: () => unawaited(openExternal(url)),
              ),
            const Spacer(),
            AxIconButton(
              icon: AxiomIcons.stop,
              label: 'Stop preview',
              onPressed: () => unawaited(s.stop()),
            ),
          ],
        ),
      ]);
    }
    if (s.error.isNotEmpty) {
      children.addAll([
        const SizedBox(height: Metrics.s3),
        Semantics(
          liveRegion: true,
          child: Text(s.error, style: t.type.small.copyWith(color: c.dangerText)),
        ),
      ]);
    }
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: children);
  }
}

String _round(double v) => v == v.roundToDouble() ? '${v.round()}' : v.toStringAsFixed(1);

class _Selection extends StatelessWidget {
  const _Selection({required this.store, required this.node});
  final InspectorStore store;
  final UiNode node;

  void _open(BuildContext context, SourceCandidate c) {
    final app = AppScope.of(context);
    unawaited(
      app.documents.open(c.location.file, line: c.location.line, column: c.location.column ?? 1),
    );
  }

  void _ask(BuildContext context) {
    final app = AppScope.of(context);
    final best = node.sources?.where((c) => c.generated != true).firstOrNull;
    final what = nodeLabel(node);
    final key = node.stableKey == null ? '' : ', key ${node.stableKey}';
    final where = best == null
        ? ''
        : isExact(best)
        ? ' at ${locationLabel(best.location)}'
        : ' (source uncertain: ${locationLabel(best.location)})';
    final seed = 'About the selected element, $what$key$where: ';
    if (best != null) {
      final l = best.location;
      app.agent.attach(
        Attachment(path: l.file, lines: [l.line, l.line + askContextLines - 1]),
        '$what · ${l.file}:${l.line}',
        seed: seed,
      );
    } else {
      app.agent.seed(seed);
    }
    app.showDock?.call();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final s = store;
    final b = node.bounds;
    final chain = s.selection;
    final sources = node.sources ?? const <SourceCandidate>[];
    final props = (s.details?.properties ?? const <String, Object?>{}).entries.toList();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const _Heading('Selected element'),
        Semantics(
          liveRegion: true,
          child: Text(nodeLabel(node), style: t.type.body, overflow: TextOverflow.ellipsis),
        ),
        if (b != null)
          Text(
            '${_round(b.x)}, ${_round(b.y)} · ${_round(b.width)} × ${_round(b.height)} px',
            style: t.type.tiny,
          ),
        const SizedBox(height: Metrics.s3),
        Align(
          alignment: Alignment.centerLeft,
          child: AxButton(
            label: 'Ask AI about this element',
            icon: AxiomIcons.chat,
            onPressed: () => _ask(context),
          ),
        ),
        const SizedBox(height: Metrics.s4),
        const _Heading('Source'),
        if (sources.isEmpty) Text('No source location is known.', style: t.type.small),
        for (final c in sources)
          AxListRow(
            label: 'Open ${locationLabel(c.location)}${isExact(c) ? '' : ', uncertain'}',
            hint: isExact(c) ? null : candidateLabel(c),
            height: 28,
            onPressed: () => _open(context, c),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: Metrics.s2),
              child: Row(
                children: [
                  StatusDot(isExact(c) ? DotState.ok : DotState.warn),
                  const SizedBox(width: Metrics.s3),
                  Expanded(
                    child: Text(
                      candidateLabel(c),
                      style: t.type.small,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                ],
              ),
            ),
          ),
        if (chain.length > 1) ...[
          const SizedBox(height: Metrics.s4),
          const _Heading('Hierarchy'),
          for (var i = chain.length - 1; i >= 0; i--)
            AxListRow(
              label: i == 0 ? '${nodeLabel(chain[i])}, selected' : 'Select ${nodeLabel(chain[i])}',
              selected: i == 0,
              height: 24,
              onPressed: i == 0 ? null : () => unawaited(s.select(i)),
              child: Padding(
                padding: EdgeInsets.only(left: Metrics.s2 + (chain.length - 1 - i) * 10.0),
                child: Align(
                  alignment: Alignment.centerLeft,
                  child: Text(
                    nodeLabel(chain[i]),
                    style: t.type.small,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
              ),
            ),
        ],
        if (props.isNotEmpty) ...[
          const SizedBox(height: Metrics.s4),
          const _Heading('Properties'),
          for (final e in props)
            Semantics(
              label: '${e.key}: ${propertyText(e.value)}',
              child: ExcludeSemantics(
                child: Padding(
                  padding: const EdgeInsets.symmetric(vertical: 1),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Expanded(
                        flex: 5,
                        child: Text(e.key, style: t.type.tiny, overflow: TextOverflow.ellipsis),
                      ),
                      const SizedBox(width: Metrics.s2),
                      Expanded(
                        flex: 4,
                        child: Text(
                          propertyText(e.value),
                          style: t.type.small,
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
        ],
      ],
    );
  }
}

class _Report extends StatelessWidget {
  const _Report({required this.report});
  final VerificationReport report;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final v = report.verdict;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const _Heading('Last verification'),
        Semantics(
          liveRegion: true,
          label: 'Verification: ${verdictLabel(v)}',
          child: ExcludeSemantics(
            child: Row(
              children: [
                StatusDot(switch (v) {
                  Verdict.verified => DotState.ok,
                  Verdict.failed => DotState.error,
                  Verdict.inconclusive => DotState.warn,
                }),
                const SizedBox(width: Metrics.s3),
                Text(verdictLabel(v), style: t.type.body),
              ],
            ),
          ),
        ),
        const SizedBox(height: Metrics.s2),
        for (final c in report.checks)
          Semantics(
            label:
                '${c.skipped == true
                    ? 'Skipped'
                    : c.passed
                    ? 'Passed'
                    : 'Failed'}: ${c.summary}',
            child: ExcludeSemantics(
              child: Padding(
                padding: const EdgeInsets.symmetric(vertical: 2),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Padding(
                      padding: const EdgeInsets.only(top: 4),
                      child: StatusDot(
                        c.skipped == true
                            ? DotState.off
                            : c.passed
                            ? DotState.ok
                            : DotState.error,
                        size: 6,
                      ),
                    ),
                    const SizedBox(width: Metrics.s3),
                    Expanded(child: Text(c.summary, style: t.type.small)),
                  ],
                ),
              ),
            ),
          ),
      ],
    );
  }
}
