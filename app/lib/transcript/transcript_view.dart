import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../host/proto.dart';
import '../host/session_model.dart';
import '../patterns/highlight.dart';
import '../patterns/transcript_parts.dart';
import '../ui/controls.dart';
import '../ui/editable.dart';
import '../ui/icons.dart';
import '../ui/press.dart';
import '../ui/tokens.dart';
import 'inline.dart';
import 'markdown.dart';
import 'safe_text.dart';

/// A row: one closed block of an item, or what is open of it.
class TranscriptRow {
  TranscriptRow(this.item, this.index, this.ordinal, {this.open = false});
  final ItemModel item;
  final int index;
  final bool open;

  /// Its place in the transcript. Rows are only inserted near the end, so few ever change it.
  int ordinal;
}

/// The rows of one agent's transcript. Growing costs what grew.
class TranscriptRows {
  TranscriptRows(this.agent) {
    sync();
  }
  final AgentModel agent;
  final List<TranscriptRow> rows = [];
  int _items = 0;

  /// The items that can still change, in order, with what is known of them.
  final List<_Live> _live = [];

  /// Adds what is new. True if any row was added, removed or replaced.
  bool sync() {
    var changed = false;
    for (final l in _live) {
      changed = _grow(l) || changed;
    }
    _live.removeWhere((l) => !l.item.open && l.done);
    while (_items < agent.items.length) {
      final l = _Live(agent.items[_items++], rows.length);
      _live.add(l);
      _grow(l);
      changed = true;
      if (!l.item.open) {
        l.done = true;
      }
    }
    _live.removeWhere((l) => !l.item.open && l.done);
    return changed;
  }

  bool _grow(_Live l) {
    final i = l.item;
    final wants = i.rich ? i.doc.closed.length : 0;
    final open = !i.rich || i.open;
    final replaced = i.rich && i.doc.revision != l.revision;
    if (!replaced && wants == l.closed && open == l.opened) {
      if (!i.open) l.done = true;
      return false;
    }
    // Items are contiguous: this one's rows start where its first row is.
    final at = l.first;
    final had = l.closed + (l.opened ? 1 : 0);
    final keep = replaced ? 0 : l.closed;
    final fresh = [
      for (var k = keep; k < wants; k++) TranscriptRow(i, k, 0),
      if (open) TranscriptRow(i, wants, 0, open: true),
    ];
    rows.replaceRange(at + keep, at + had, fresh);
    for (var k = at + keep; k < rows.length; k++) {
      rows[k].ordinal = k;
    }
    final shift = fresh.length - (had - keep);
    if (shift != 0) {
      // Items stand in order, so everything after this one moves, also what has no rows yet.
      for (var k = _live.indexOf(l) + 1; k < _live.length; k++) {
        _live[k].first += shift;
      }
    }
    l
      ..closed = wants
      ..opened = open
      ..revision = i.doc.revision
      ..done = !i.open;
    return true;
  }
}

class _Live {
  _Live(this.item, this.first);
  final ItemModel item;
  int first;
  int closed = 0, revision = 0;
  bool opened = false, done = false;
}

/// An agent's transcript. Only what is visible exists as widgets; closed blocks never rebuild; the
/// block being written rebuilds alone, at most once per frame. What is being read stays where it is;
/// at the end, the view follows.
class TranscriptView extends StatefulWidget {
  const TranscriptView(this.agent, {super.key, this.onLink, this.controller});
  final AgentModel agent;
  final void Function(String url)? onLink;
  final ScrollController? controller;
  @override
  State<TranscriptView> createState() => TranscriptViewState();
}

class TranscriptViewState extends State<TranscriptView> {
  static const _center = ValueKey('now'), _ahead = 80;
  ScrollController? _own;
  ScrollController get scroll => widget.controller ?? (_own ??= ScrollController());
  late TranscriptRows _rows;

  /// Rows before this one lie above the scroll view's origin: what happens after them cannot move
  /// them, and what is above is built only when someone goes there.
  int _pivot = 0;
  final _listening = <ItemModel>{};
  final _focus = FocusNode(debugLabel: 'transcript');
  bool _atEnd = true;

  @override
  void initState() {
    super.initState();
    _open();
    scroll.addListener(_scrolled);
  }

  void _open() {
    _rows = TranscriptRows(widget.agent);
    widget.agent.addListener(_changed);
    _listen();
    _pivot = _rows.rows.length > _ahead ? _rows.rows.length - _ahead : 0;
    _atEnd = true;
    WidgetsBinding.instance.addPostFrameCallback((_) => _toEnd());
  }

  void _close(AgentModel a) {
    a.removeListener(_changed);
    for (final i in _listening) {
      i.removeListener(_changed);
    }
    _listening.clear();
  }

  @override
  void didUpdateWidget(TranscriptView oldWidget) {
    super.didUpdateWidget(oldWidget);
    final old = oldWidget;
    if (old.controller != widget.controller) {
      (old.controller ?? _own)?.removeListener(_scrolled);
      scroll.addListener(_scrolled);
    }
    if (!identical(old.agent, widget.agent)) {
      _close(old.agent);
      _open();
    }
  }

  @override
  void dispose() {
    _close(widget.agent);
    scroll.removeListener(_scrolled);
    _own?.dispose();
    _focus.dispose();
    super.dispose();
  }

  /// Follows the items that can still change.
  void _listen() {
    final open = {
      for (final i in widget.agent.items.reversed.take(64))
        if (i.open) i,
    };
    for (final i in _listening.difference(open)) {
      i.removeListener(_changed);
    }
    for (final i in open.difference(_listening)) {
      i.addListener(_changed);
    }
    _listening
      ..clear()
      ..addAll(open);
  }

  void _changed() {
    if (_rows.sync()) setState(() {});
    _listen();
  }

  void _scrolled() {
    final p = scroll.position;
    final end = p.pixels >= p.maxScrollExtent - 4;
    if (end != _atEnd) setState(() => _atEnd = end);
  }

  void _toEnd() {
    if (!mounted || !scroll.hasClients) return;
    scroll.jumpTo(scroll.position.maxScrollExtent);
    // The far end's extent is an estimate until it has been laid out.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && scroll.hasClients && _atEnd) scroll.jumpTo(scroll.position.maxScrollExtent);
    });
  }

  /// Both ends are reached by moving the origin, not by laying out everything between.
  void toStart() {
    setState(() {
      _pivot = 0;
      _atEnd = false;
    });
    scroll.jumpTo(0);
  }

  void toEnd() {
    setState(() {
      final n = _rows.rows.length;
      _pivot = n > _ahead ? n - _ahead : 0;
      _atEnd = true;
    });
    WidgetsBinding.instance.addPostFrameCallback((_) => _toEnd());
  }

  void _by(double viewports) {
    final p = scroll.position;
    final to = (p.pixels + viewports * p.viewportDimension).clamp(
      p.minScrollExtent,
      p.maxScrollExtent,
    );
    scroll.jumpTo(to);
  }

  @override
  Widget build(BuildContext context) {
    final rows = _rows.rows, n = rows.length;
    if (_pivot > n) _pivot = n;
    Widget row(int k) => KeyedSubtree(
      key: ObjectKey(rows[k]),
      child: _RowView(rows, k, onLink: widget.onLink),
    );
    int? find(Key k, int from, int step) {
      final r = (k as ObjectKey).value! as TranscriptRow;
      // A row that was replaced is not found at its place.
      if (r.ordinal >= n || !identical(rows[r.ordinal], r)) return null;
      final i = (r.ordinal - from) * step;
      return i < 0 ? null : i;
    }

    final a = widget.agent;
    const side = EdgeInsets.symmetric(horizontal: 24);
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.pageDown): () => _by(.9),
        const SingleActivator(LogicalKeyboardKey.pageUp): () => _by(-.9),
        const SingleActivator(LogicalKeyboardKey.space): () => _by(.9),
        const SingleActivator(LogicalKeyboardKey.space, shift: true): () => _by(-.9),
        const SingleActivator(LogicalKeyboardKey.arrowDown): () => _by(.08),
        const SingleActivator(LogicalKeyboardKey.arrowUp): () => _by(-.08),
        const SingleActivator(LogicalKeyboardKey.home): toStart,
        const SingleActivator(LogicalKeyboardKey.end): toEnd,
        const SingleActivator(LogicalKeyboardKey.arrowUp, meta: true): toStart,
        const SingleActivator(LogicalKeyboardKey.arrowDown, meta: true): toEnd,
      },
      child: Semantics(
        container: true,
        explicitChildNodes: true,
        label: 'Transcript of ${a.name}',
        child: Stack(
          children: [
            Positioned.fill(
              child: SelectableRegion(
                focusNode: _focus,
                selectionControls: emptyTextSelectionControls,
                contextMenuBuilder: (context, s) =>
                    AxContextMenu(anchors: s.contextMenuAnchors, items: s.contextMenuButtonItems),
                child: CustomScrollView(
                  controller: scroll,
                  center: _center,
                  physics: _Follow(() => _atEnd),
                  scrollCacheExtent: const ScrollCacheExtent.pixels(400),
                  slivers: [
                    SliverPadding(
                      padding: side,
                      sliver: SliverList.builder(
                        itemCount: _pivot,
                        itemBuilder: (_, i) => row(_pivot - 1 - i),
                        findChildIndexCallback: (k) => find(k, _pivot - 1, -1),
                      ),
                    ),
                    SliverPadding(
                      key: _center,
                      padding: side,
                      sliver: SliverList.builder(
                        itemCount: n - _pivot,
                        itemBuilder: (_, i) => row(_pivot + i),
                        findChildIndexCallback: (k) => find(k, _pivot, 1),
                      ),
                    ),
                    SliverPadding(
                      padding: const EdgeInsets.fromLTRB(24, 0, 24, 20),
                      sliver: SliverToBoxAdapter(
                        child: ListenableBuilder(
                          listenable: a,
                          builder: (context, _) => _Ending(a),
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
            if (!_atEnd)
              Positioned(
                right: 20,
                bottom: 14,
                child: AxButton(label: 'Latest', icon: AxIcon.down, onPressed: toEnd),
              ),
          ],
        ),
      ),
    );
  }
}

/// Keeps to the end when at the end; otherwise leaves the position alone.
class _Follow extends ClampingScrollPhysics {
  const _Follow(this.atEnd, {super.parent});
  final bool Function() atEnd;
  @override
  _Follow applyTo(ScrollPhysics? ancestor) => _Follow(atEnd, parent: buildParent(ancestor));
  @override
  double adjustPositionForNewDimensions({
    required ScrollMetrics oldPosition,
    required ScrollMetrics newPosition,
    required bool isScrolling,
    required double velocity,
  }) => !isScrolling && atEnd()
      ? newPosition.maxScrollExtent
      : super.adjustPositionForNewDimensions(
          oldPosition: oldPosition,
          newPosition: newPosition,
          isScrolling: isScrolling,
          velocity: velocity,
        );
}

/// How the run ended, if not well.
class _Ending extends StatelessWidget {
  const _Ending(this.agent);
  final AgentModel agent;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final words = switch (agent.ended) {
      Outcome.failed => agent.error ?? 'The run failed.',
      Outcome.interrupted => 'Interrupted: the host stopped while this ran.',
      Outcome.cancelled => 'Stopped.',
      _ => null,
    };
    if (words == null) return const SizedBox.shrink();
    final failed = agent.ended == Outcome.failed;
    return Padding(
      padding: const EdgeInsets.only(top: 14),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 22,
            height: 18,
            child: Align(
              alignment: Alignment.centerLeft,
              child: AxGlyph(AxIcon.warn, size: 13, color: failed ? c.faultText : c.ink3),
            ),
          ),
          Expanded(child: Text(words, style: t.type.ui.ink(failed ? c.faultText : c.ink3))),
        ],
      ),
    );
  }
}

/// Reading width of prose, and of everything that stands with it.
const _measure = 640.0, _wide = 780.0;

class _RowView extends StatelessWidget {
  const _RowView(this.rows, this.at, {this.onLink});
  final List<TranscriptRow> rows;
  final int at;
  final void Function(String url)? onLink;
  @override
  Widget build(BuildContext context) {
    final r = rows[at], i = r.item;
    final before = at > 0 ? rows[at - 1] : null;
    final first = at == 0;
    final Widget child;
    if (!i.rich) {
      final follows = before != null && !before.item.rich;
      child = Padding(
        padding: EdgeInsets.only(
          top: first
              ? 16
              : follows
              ? 2
              : 10,
        ),
        child: ListenableBuilder(listenable: i, builder: (_, _) => ToolRow(i)),
      );
    } else if (!r.open) {
      final blocks = i.doc.closed;
      child = BlockView(
        blocks[r.index],
        voice: i.kind,
        opens: r.index == 0,
        afterTool: r.index == 0 && before != null && !before.item.rich,
        goesOn: r.index + 1 < blocks.length && blocks[r.index + 1].continues,
        onLink: onLink,
      );
    } else {
      child = ListenableBuilder(
        listenable: i,
        builder: (_, _) {
          final tail = i.doc.tail;
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              for (final (k, b) in tail.indexed)
                BlockView(
                  b,
                  voice: i.kind,
                  opens: r.index + k == 0,
                  afterTool: r.index + k == 0 && before != null && !before.item.rich,
                  goesOn: k + 1 < tail.length && tail[k + 1].continues,
                  onLink: onLink,
                ),
            ],
          );
        },
      );
    }
    return RepaintBoundary(child: child);
  }
}

class BlockView extends StatefulWidget {
  const BlockView(
    this.block, {
    required this.voice,
    super.key,
    this.opens = false,
    this.afterTool = false,
    this.goesOn = false,
    this.onLink,
  });
  final Block block;
  final ItemKind voice;

  /// First of its item; after a run of tools; continued by the next block.
  final bool opens, afterTool, goesOn;
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
      ItemKind.reasoning => c.ink3,
      _ => c.ink1,
    };
    final base = t.type.prose.ink(ink);
    final code = t.type.code.copyWith(height: base.height, color: c.ink2, fontSize: 12.5);
    Widget text(TextStyle s, {bool header = false}) {
      final w = ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: _measure),
        child: Text.rich(
          inlineSpan(b.text, s, code, c, onLink: widget.onLink, recognizers: _recognizers),
          style: s,
        ),
      );
      return header ? Semantics(header: true, child: w) : w;
    }

    Widget left(Widget w) => Align(alignment: Alignment.topLeft, heightFactor: 1, child: w);
    var child = switch (b.kind) {
      BlockKind.paragraph => left(text(base)),
      BlockKind.heading => left(
        text(
          (b.level == 1
                  ? t.type.heading
                  : b.level == 2
                  ? t.type.title
                  : t.type.uiStrong)
              .ink(ink),
          header: true,
        ),
      ),
      BlockKind.item => Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(width: 22, child: _Marker(b.marker, base)),
          Expanded(child: left(text(base))),
        ],
      ),
      BlockKind.quote => left(
        DecoratedBox(
          decoration: BoxDecoration(
            border: Border(left: BorderSide(color: c.ink4, width: 2)),
          ),
          child: Padding(padding: const EdgeInsets.only(left: 14), child: text(base.ink(c.ink3))),
        ),
      ),
      BlockKind.rule => Padding(
        padding: const EdgeInsets.symmetric(vertical: 8),
        child: ColoredBox(color: c.bayRule, child: const SizedBox(height: 1)),
      ),
      BlockKind.code => CodeBlock(b, goesOn: widget.goesOn),
      BlockKind.table => TableBlock(b.text),
    };
    // An item's marker stands in the column of its own depth; what belongs to it stands after.
    final indent = (b.kind == BlockKind.item ? b.depth - 1 : b.depth) * 22.0;
    if (indent > 0) {
      child = Padding(
        padding: EdgeInsets.only(left: indent),
        child: child,
      );
    }
    if (widget.voice == ItemKind.prompt) child = _Voice(child, opens: widget.opens);
    final top = b.continues
        ? 0.0
        : widget.opens
        ? (widget.afterTool ? 14.0 : 18.0)
        : b.kind == BlockKind.heading
        ? 20.0
        : b.kind == BlockKind.item
        ? 6.0
        : 11.0;
    child = Padding(
      padding: EdgeInsets.only(top: widget.voice == ItemKind.prompt && !widget.opens ? 0 : top),
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: _wide),
        child: child,
      ),
    );
    child = Align(alignment: Alignment.topLeft, heightFactor: 1, child: child);
    if (!widget.opens) return child;
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: switch (widget.voice) {
        ItemKind.prompt => 'Request',
        ItemKind.reasoning => 'Reasoning',
        _ => 'Answer',
      },
      child: child,
    );
  }
}

/// What was asked: a rule in the margin, so it is found when scrolling back. The words themselves
/// are as strong as any others.
class _Voice extends StatelessWidget {
  const _Voice(this.child, {required this.opens});
  final Widget child;
  final bool opens;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return DecoratedBox(
      decoration: BoxDecoration(
        border: Border(left: BorderSide(color: c.idle, width: 2)),
      ),
      child: Padding(
        padding: EdgeInsets.only(left: 12, top: opens ? 0 : 11),
        child: child,
      ),
    );
  }
}

class _Marker extends StatelessWidget {
  const _Marker(this.marker, this.style);
  final String marker;
  final TextStyle style;
  @override
  Widget build(BuildContext context) {
    final c = Ax.of(context).colors;
    if (marker == 'todo' || marker == 'done') {
      return Semantics(
        label: marker == 'done' ? 'done' : 'to do',
        child: SizedBox(
          height: MediaQuery.textScalerOf(context).scale(style.fontSize!) * style.height!,
          child: Align(
            alignment: Alignment.centerLeft,
            child: marker == 'done'
                ? AxGlyph(AxIcon.check, size: 13, color: c.doneText)
                : DecoratedBox(
                    decoration: BoxDecoration(
                      border: Border.all(color: c.idle, width: 1.3),
                      borderRadius: BorderRadius.circular(2.5),
                    ),
                    child: const SizedBox.square(dimension: 11),
                  ),
          ),
        ),
      );
    }
    return Padding(
      padding: const EdgeInsets.only(right: 6),
      child: Text(
        marker,
        textAlign: marker == '•' ? TextAlign.left : TextAlign.right,
        softWrap: false,
        overflow: TextOverflow.visible,
        style: style.ink(c.ink4),
      ),
    );
  }
}

class CodeBlock extends StatefulWidget {
  const CodeBlock(this.block, {super.key, this.goesOn = false});
  final Block block;
  final bool goesOn;
  static const maxColumns = 2000;
  @override
  State<CodeBlock> createState() => _CodeBlockState();
}

class _CodeBlockState extends State<CodeBlock> {
  bool _over = false, _copied = false;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors, b = widget.block;
    final style = t.type.code;
    final lines = literal(b.text).split('\n');
    const r = Radius.circular(AxRadius.key);
    return MouseRegion(
      onEnter: (_) => setState(() => _over = true),
      onExit: (_) => setState(() => _over = _copied = false),
      child: Stack(
        children: [
          ClipRRect(
            borderRadius: BorderRadius.vertical(
              top: b.continues ? Radius.zero : r,
              bottom: widget.goesOn ? Radius.zero : r,
            ),
            child: ColoredBox(
              color: c.block.bottom,
              child: SizedBox(
                width: double.infinity,
                child: SingleChildScrollView(
                  scrollDirection: Axis.horizontal,
                  padding: EdgeInsets.fromLTRB(
                    14,
                    b.continues ? 0 : 10,
                    14,
                    widget.goesOn ? 0 : 10,
                  ),
                  child: Text.rich(
                    TextSpan(
                      children: [
                        for (final (i, l) in lines.indexed) ...[
                          if (i > 0) const TextSpan(text: '\n'),
                          highlight(
                            l.length > CodeBlock.maxColumns
                                ? '${l.substring(0, CodeBlock.maxColumns)} … '
                                      '${l.length - CodeBlock.maxColumns} more'
                                : l,
                            style,
                            c,
                          ),
                        ],
                      ],
                    ),
                    style: style,
                    softWrap: false,
                  ),
                ),
              ),
            ),
          ),
          if (!b.continues)
            Positioned(
              right: 5,
              top: 5,
              child: Opacity(
                opacity: _over ? 1 : 0,
                child: AxButton(
                  label: _copied ? 'Copied' : 'Copy',
                  size: 22,
                  onPressed: () async {
                    // What was written, not what is shown.
                    await Clipboard.setData(ClipboardData(text: b.text));
                    if (mounted) setState(() => _copied = true);
                  },
                ),
              ),
            ),
        ],
      ),
    );
  }
}

class TableBlock extends StatelessWidget {
  const TableBlock(this.source, {super.key});
  final String source;
  static const maxColumns = 12, maxCell = 1000;

  static List<String> cells(String l) {
    var x = l.trim();
    if (x.startsWith('|')) x = x.substring(1);
    final out = <String>[];
    final b = StringBuffer();
    for (var i = 0; i < x.length; i++) {
      if (x[i] == r'\' && i + 1 < x.length && x[i + 1] == '|') {
        b.write('|');
        i++;
      } else if (x[i] == '|') {
        out.add(b.toString().trim());
        b.clear();
      } else {
        b.write(x[i]);
      }
    }
    if (b.toString().trim().isNotEmpty || !x.endsWith('|')) out.add(b.toString().trim());
    return [
      for (final e in out.take(maxColumns)) e.length > maxCell ? '${e.substring(0, maxCell)}…' : e,
    ];
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final raw = source.split('\n').map(cells).toList();
    final head = raw.length > 1 && raw[1].every((x) => x.replaceAll(RegExp('[:-]'), '').isEmpty);
    final align = [
      if (head)
        for (final x in raw[1])
          x.endsWith(':')
              ? (x.startsWith(':') ? TextAlign.center : TextAlign.right)
              : TextAlign.left,
    ];
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
                        style: head && i == 0 ? t.type.labelStrong : t.type.ui,
                        textAlign: k < align.length ? align[k] : TextAlign.left,
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

/// A tool call, as one line; its output opens under it.
class ToolRow extends StatefulWidget {
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
  State<ToolRow> createState() => _ToolRowState();
}

class _ToolRowState extends State<ToolRow> {
  bool _shown = false;
  static const maxOutput = 20000;

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors, item = widget.item;
    final args = item.args;
    final notice = item.kind == ItemKind.notice;
    if (notice) {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 3),
        child: Text(visible(item.plain), style: t.type.ui.ink(c.ink3)),
      );
    }
    final detail = args is Map
        ? '${args['path'] ?? args['command'] ?? args['pattern'] ?? args.values.firstOrNull ?? ''}'
        : '${args ?? ''}';
    final result = item.result;
    final summary = result is Map ? oneLine('${result['summary'] ?? ''}', 160) : '';
    final name = item.name.isEmpty ? 'tool' : item.name;
    final output = item.plain;
    final line = WorkLine(
      icon: ToolRow.iconOf(item.name),
      name: name[0].toUpperCase() + name.substring(1),
      detail: oneLine(literal(detail), 400),
      result: item.open
          ? 'running'
          : switch (item.outcome!) {
              Outcome.cancelled => 'cancelled',
              Outcome.interrupted => 'interrupted',
              Outcome.failed => summary.isEmpty ? 'failed' : summary,
              _ => summary,
            },
      failed: item.outcome == Outcome.failed,
      more: output.isEmpty ? null : _shown,
    );
    return Align(
      alignment: Alignment.topLeft,
      heightFactor: 1,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: _wide),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (output.isEmpty)
              line
            else
              Pressable(
                label: '$name $detail',
                hint: _shown ? 'Hide output' : 'Show output',
                expanded: _shown,
                radius: 0,
                onPressed: () => setState(() => _shown = !_shown),
                builder: (_, s) =>
                    ColoredBox(color: s.hovered ? c.hover : c.bay.bottom, child: line),
              ),
            if (_shown)
              Padding(
                padding: const EdgeInsets.fromLTRB(22, 4, 0, 6),
                child: CodeBlock(
                  Block(
                    BlockKind.code,
                    output.length > maxOutput
                        ? '${output.substring(output.length - maxOutput)}\n'
                        : output,
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}
