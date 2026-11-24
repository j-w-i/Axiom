import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';

import '../host/proto.dart';
import '../host/session_model.dart';
import '../patterns/highlight.dart';
import '../patterns/transcript_parts.dart';
import '../ui/icons.dart';
import '../ui/tokens.dart';
import 'inline.dart';
import 'markdown.dart';

/// An agent's transcript. Only what is visible exists as widgets; closed blocks never rebuild; the
/// block being written rebuilds alone, at most once per frame.
class TranscriptView extends StatefulWidget {
  const TranscriptView(this.agent, {super.key, this.onLink, this.controller});
  final AgentModel agent;
  final void Function(String url)? onLink;
  final ScrollController? controller;
  @override
  State<TranscriptView> createState() => _TranscriptViewState();
}

/// A row: one closed block of an item, or everything from [index] on while the item is open.
@immutable
class _Row {
  const _Row(this.item, this.index, {this.open = false});
  final ItemModel item;
  final int index;
  final bool open;
  @override
  bool operator ==(Object other) =>
      other is _Row && identical(other.item, item) && other.index == index && other.open == open;
  @override
  int get hashCode => Object.hash(identityHashCode(item), index, open);
}

class _TranscriptViewState extends State<TranscriptView> {
  late final ScrollController _scroll = widget.controller ?? ScrollController();
  final _rows = <_Row>[];
  int _items = 0;

  @override
  void initState() {
    super.initState();
    widget.agent.addListener(_changed);
    _grow();
  }

  @override
  void didUpdateWidget(TranscriptView old) {
    super.didUpdateWidget(old);
    if (identical(old.agent, widget.agent)) return;
    old.agent.removeListener(_changed);
    _last?.removeListener(_changed);
    _last = null;
    _rows.clear();
    _items = _settled = 0;
    _opened = false;
    widget.agent.addListener(_changed);
    _grow();
    if (_scroll.hasClients) _scroll.jumpTo(0);
  }

  @override
  void dispose() {
    widget.agent.removeListener(_changed);
    _last?.removeListener(_changed);
    if (widget.controller == null) _scroll.dispose();
    super.dispose();
  }

  /// The items still open: their rows can grow.
  ItemModel? _last;

  void _changed() {
    final before = _rows.length;
    _grow();
    // Growing text does not rebuild the list; only new rows do.
    if (_rows.length != before) setState(() {});
  }

  /// Adds rows for what is new. Costs what is new: earlier rows are never looked at again. An item
  /// that another one followed keeps its open row to the end, whatever it still receives.
  void _grow() {
    final items = widget.agent.items;
    while (_items < items.length) {
      final i = items[_items];
      if (_opened) _rows.removeLast();
      final n = i.rich ? i.doc.closed.length : 0;
      for (; _settled < n; _settled++) {
        _rows.add(_Row(i, _settled));
      }
      _opened = !i.rich || i.open;
      if (_opened) _rows.add(_Row(i, _settled, open: true));
      if (_items + 1 == items.length) {
        if (!identical(_last, i)) {
          _last?.removeListener(_changed);
          _last = i..addListener(_changed);
        }
        return;
      }
      _items++;
      _settled = 0;
      _opened = false;
    }
  }

  int _settled = 0;
  bool _opened = false;

  @override
  Widget build(BuildContext context) {
    final n = _rows.length;
    return Align(
      alignment: Alignment.topCenter,
      child: CustomScrollView(
        controller: _scroll,
        reverse: true,
        shrinkWrap: n < 40,
        scrollCacheExtent: const ScrollCacheExtent.pixels(400),
        slivers: [
          SliverPadding(
            padding: const EdgeInsets.fromLTRB(24, 16, 24, 20),
            sliver: SliverList.builder(
              itemCount: n,
              // Reversed: row 0 is the newest, so growth at the end keeps the place being read.
              itemBuilder: (context, i) {
                final r = _rows[n - 1 - i];
                return KeyedSubtree(
                  key: ValueKey(r),
                  child: _RowView(r, onLink: widget.onLink),
                );
              },
              findChildIndexCallback: (k) {
                final r = (k as ValueKey<_Row>).value;
                // Rows move only near the end.
                for (var i = n - 1; i >= 0; i--) {
                  if (_rows[i] == r) return n - 1 - i;
                }
                return null;
              },
            ),
          ),
        ],
      ),
    );
  }
}

class _RowView extends StatelessWidget {
  const _RowView(this.row, {this.onLink});
  final _Row row;
  final void Function(String url)? onLink;
  @override
  Widget build(BuildContext context) {
    final i = row.item;
    if (!i.rich) {
      return RepaintBoundary(
        child: ListenableBuilder(listenable: i, builder: (_, _) => ToolRow(i)),
      );
    }
    if (!row.open) {
      return RepaintBoundary(
        child: BlockView(
          i.doc.closed[row.index],
          voice: i.kind,
          first: row.index == 0,
          onLink: onLink,
        ),
      );
    }
    return RepaintBoundary(
      child: ListenableBuilder(
        listenable: i,
        builder: (_, _) {
          final d = i.doc;
          final blocks = [
            for (var k = row.index; k < d.closed.length; k++) d.closed[k],
            ?d.beforeTail,
            ?d.tail,
          ];
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              for (final (k, b) in blocks.indexed)
                BlockView(b, voice: i.kind, first: row.index + k == 0, onLink: onLink),
            ],
          );
        },
      ),
    );
  }
}

/// Reading width: about 78 characters of prose.
const _measure = 640.0;

class BlockView extends StatefulWidget {
  const BlockView(this.block, {required this.voice, super.key, this.first = false, this.onLink});
  final Block block;
  final ItemKind voice;
  final bool first;
  final void Function(String url)? onLink;
  @override
  State<BlockView> createState() => _BlockViewState();
}

class _BlockViewState extends State<BlockView> {
  final _recognizers = <GestureRecognizer>[];
  void _clear() {
    for (final r in _recognizers) {
      r.dispose();
    }
    _recognizers.clear();
  }

  @override
  void dispose() {
    _clear();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    _clear();
    final t = Ax.of(context), c = t.colors, b = widget.block;
    final ink = switch (widget.voice) {
      ItemKind.prompt => c.voice,
      ItemKind.reasoning => c.ink3,
      _ => c.ink1,
    };
    final base = t.type.prose.ink(ink);
    final code = t.type.code.copyWith(height: base.height, color: c.ink2, fontSize: 12.5);
    Widget text(TextStyle s) => ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: _measure),
      child: Text.rich(
        inlineSpan(b.text, s, code, c, onLink: widget.onLink, recognizers: _recognizers),
      ),
    );
    final child = switch (b.kind) {
      BlockKind.paragraph => text(base),
      BlockKind.heading => text(
        (b.level == 1
                ? t.type.heading
                : b.level == 2
                ? t.type.title
                : t.type.uiStrong)
            .ink(ink),
      ),
      BlockKind.item => Padding(
        padding: EdgeInsets.only(left: b.level * 18.0),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            SizedBox(width: 22, child: Text(b.marker, style: base.ink(c.ink4))),
            Flexible(child: text(base)),
          ],
        ),
      ),
      BlockKind.quote => DecoratedBox(
        decoration: BoxDecoration(
          border: Border(left: BorderSide(color: c.ink4, width: 2)),
        ),
        child: Padding(padding: const EdgeInsets.only(left: 14), child: text(base.ink(c.ink3))),
      ),
      BlockKind.rule => Padding(
        padding: const EdgeInsets.symmetric(vertical: 8),
        child: ColoredBox(color: c.bayRule, child: const SizedBox(height: 1)),
      ),
      BlockKind.code => CodeBlock(b),
      BlockKind.table => TableBlock(b.text),
    };
    return Padding(
      padding: EdgeInsets.only(
        top: b.continues
            ? 0
            : b.kind == BlockKind.heading
            ? 18
            : widget.first
            ? 16
            : b.kind == BlockKind.item
            ? 5
            : 11,
      ),
      // Left-aligned at its own width; code and tables take the row.
      child: b.kind == BlockKind.code || b.kind == BlockKind.table || b.kind == BlockKind.rule
          ? child
          : Align(alignment: Alignment.topLeft, heightFactor: 1, child: child),
    );
  }
}

class CodeBlock extends StatelessWidget {
  const CodeBlock(this.block, {super.key});
  final Block block;
  static const maxColumns = 2000;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final style = t.type.code;
    final lines = visible(block.text).split('\n');
    return ClipRRect(
      borderRadius: BorderRadius.circular(AxRadius.key),
      child: ColoredBox(
        color: c.block.bottom,
        child: SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          padding: EdgeInsets.fromLTRB(14, block.continues ? 0 : 10, 14, 10),
          child: Text.rich(
            TextSpan(
              children: [
                for (final (i, l) in lines.indexed) ...[
                  if (i > 0) const TextSpan(text: '\n'),
                  highlight(
                    l.length > maxColumns
                        ? '${l.substring(0, maxColumns)} … ${l.length - maxColumns} more'
                        : l,
                    style,
                    c,
                  ),
                ],
              ],
              style: style,
            ),
            softWrap: false,
          ),
        ),
      ),
    );
  }
}

class TableBlock extends StatelessWidget {
  const TableBlock(this.source, {super.key});
  final String source;
  static const maxColumns = 12;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    List<String> cells(String l) {
      final x = l.trim().replaceFirst(RegExp(r'^\|'), '').replaceFirst(RegExp(r'(?<!\\)\|$'), '');
      return x
          .split(RegExp(r'(?<!\\)\|'))
          .map((e) => e.trim().replaceAll(r'\|', '|'))
          .take(maxColumns)
          .toList();
    }

    final raw = source.split('\n').map(cells).toList();
    final head = raw.length > 1 && raw[1].every((x) => RegExp(r'^:?-+:?$').hasMatch(x));
    final rows = [
      for (final (i, r) in raw.indexed)
        if (!(head && i == 1)) r,
    ];
    final n = rows.fold(0, (m, r) => r.length > m ? r.length : m);
    if (n == 0) return const SizedBox.shrink();
    return SingleChildScrollView(
      scrollDirection: Axis.horizontal,
      child: Table(
        defaultColumnWidth: const IntrinsicColumnWidth(),
        children: [
          for (final (i, r) in rows.indexed)
            TableRow(
              decoration: BoxDecoration(
                border: Border(bottom: BorderSide(color: c.bayRule)),
              ),
              children: [
                for (var k = 0; k < n; k++)
                  Padding(
                    padding: const EdgeInsets.fromLTRB(0, 5, 22, 5),
                    child: ConstrainedBox(
                      constraints: const BoxConstraints(maxWidth: 340),
                      child: Text.rich(
                        inlineSpan(
                          k < r.length ? r[k] : '',
                          head && i == 0 ? t.type.labelStrong : t.type.ui.ink(c.ink2),
                          t.type.codeSmall,
                          c,
                        ),
                      ),
                    ),
                  ),
              ],
            ),
        ],
      ),
    );
  }
}

/// A tool call, as one line. Its output is shown on request, not by default.
class ToolRow extends StatelessWidget {
  const ToolRow(this.item, {super.key});
  final ItemModel item;

  static AxIcon iconOf(String name) => switch (name) {
    'read' => AxIcon.file,
    'edit' || 'write' || 'patch' => AxIcon.edit,
    'search' || 'glob' || 'list' => AxIcon.search,
    'shell' || 'test' => AxIcon.terminal,
    'fetch' => AxIcon.web,
    'delegate' => AxIcon.delegate,
    _ => AxIcon.more,
  };

  @override
  Widget build(BuildContext context) {
    final args = item.args;
    final detail = args is Map
        ? '${args['path'] ?? args['command'] ?? args['pattern'] ?? args.values.firstOrNull ?? ''}'
        : '${args ?? item.plain}';
    final result = item.result;
    final summary = result is Map ? '${result['summary'] ?? ''}' : '';
    final failed = item.outcome == Outcome.failed;
    final name = item.kind == ItemKind.notice ? 'Notice' : item.name;
    return Padding(
      padding: const EdgeInsets.only(top: 3),
      child: WorkLine(
        icon: iconOf(item.name),
        name: name.isEmpty ? 'tool' : name[0].toUpperCase() + name.substring(1),
        detail: visible(detail.split('\n').first),
        result: item.open
            ? 'running'
            : switch (item.outcome!) {
                Outcome.cancelled => 'cancelled',
                Outcome.interrupted => 'interrupted',
                Outcome.failed => summary.isEmpty ? 'failed' : visible(summary),
                _ => visible(summary),
              },
        failed: failed,
      ),
    );
  }
}
