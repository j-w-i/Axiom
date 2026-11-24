import 'package:flutter/gestures.dart';
import 'package:flutter/rendering.dart';
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

/// A row: one closed block, or the end of a passage: its last blocks and the work that followed it.
class TranscriptRow {
  TranscriptRow.block(ItemModel this.item, this.index)
    : work = const [],
      from = 0,
      to = 0,
      isEnd = false;
  TranscriptRow.end(this.item, this.index, this.work, this.from, this.to) : isEnd = true;

  /// The passage; null for work that nothing was said before.
  final ItemModel? item;

  /// Which block; for an end, the first block it shows.
  final int index;

  /// The work, from [from] to [to].
  final List<ItemModel> work;
  final int from;
  int to;
  final bool isEnd;

  /// Its place in the transcript. Rows are only inserted near the end, so few ever change it.
  int ordinal = 0;
}

/// A passage and the work up to the next one.
class _Group {
  _Group(this.item, this.first);
  final ItemModel? item;
  final List<ItemModel> work = [];
  int first;
  List<TranscriptRow> rows = [];
  (int, int, bool, int) seen = (-1, -1, false, -1);
  bool get open => (item?.open ?? false) || work.any((w) => w.open);
}

/// The rows of one agent's transcript. Growing costs what grew.
class TranscriptRows {
  TranscriptRows(this.agent) {
    sync();
  }
  final AgentModel agent;
  final List<TranscriptRow> rows = [];
  int _items = 0;
  final List<_Group> _live = [];

  /// How many blocks stand beside the work they led to, and how much work stands in one row.
  static const beside = 6, together = 30;

  /// The items whose rows may still change.
  Set<ItemModel> get live => {
    for (final g in _live) ...[
      if (g.item case final i? when i.open) i,
      ...g.work.where((w) => w.open),
    ],
  };

  /// Adds what is new. True if anything must be built again.
  bool sync() {
    while (_items < agent.items.length) {
      final i = agent.items[_items++];
      if (i.rich || _live.isEmpty) {
        // Settle what stands before, so that the new group knows where it begins.
        for (final g in _live) {
          _grow(g);
        }
        _live.add(_Group(i.rich ? i : null, rows.length));
      }
      if (!i.rich) _live.last.work.add(i);
    }
    var changed = false;
    for (final g in _live) {
      changed = _grow(g) || changed;
    }
    // The last group can still be given work.
    if (_live.length > 1) {
      final last = _live.last;
      _live.removeWhere((g) => !identical(g, last) && !g.open);
    }
    final dirty = _dirty();
    return changed || dirty;
  }

  bool _was = false;
  bool _dirty() {
    final d = _was;
    _was = false;
    return d;
  }

  bool _grow(_Group g) {
    final i = g.item;
    final closed = i?.doc.closed.length ?? 0, open = i?.open ?? false;
    final sig = (closed, i?.doc.revision ?? 0, open, g.work.length);
    if (sig == g.seen) return false;
    final replaced = g.seen.$2 != sig.$2;
    g.seen = sig;
    final keep = g.work.isEmpty ? closed : (closed > beside ? closed - beside : 0);
    final old = g.rows;
    final next = <TranscriptRow>[];
    for (var k = 0; k < keep; k++) {
      final same = !replaced && k < old.length && !old[k].isEnd && old[k].index == k;
      next.add(same ? old[k] : TranscriptRow.block(i!, k));
    }
    if (open || g.work.isNotEmpty) {
      for (var f = 0; f == 0 || f < g.work.length; f += together) {
        final to = f + together < g.work.length ? f + together : g.work.length;
        final at = f == 0 ? keep : closed;
        final o = old
            .where((r) => r.isEnd && r.from == f && r.index == at && identical(r.work, g.work))
            .firstOrNull;
        if (o != null && !replaced) {
          next.add(o..to = to);
        } else {
          next.add(TranscriptRow.end(i, at, g.work, f, to));
        }
      }
    }
    var same = 0;
    while (same < old.length && same < next.length && identical(old[same], next[same])) {
      same++;
    }
    rows.replaceRange(g.first + same, g.first + old.length, next.sublist(same));
    for (var k = g.first + same; k < rows.length; k++) {
      rows[k].ordinal = k;
    }
    final shift = next.length - old.length;
    if (shift != 0) {
      for (var k = _live.indexOf(g) + 1; k < _live.length; k++) {
        _live[k].first += shift;
      }
    }
    g.rows = next;
    _was = true;
    return true;
  }
}

/// An agent's transcript. Only what is visible exists as widgets; closed blocks never rebuild; the
/// block being written rebuilds alone, at most once per frame. What is being read stays where it is;
/// at the end, the view follows.
class TranscriptView extends StatefulWidget {
  const TranscriptView(this.agent, {super.key, this.onLink});
  final AgentModel agent;
  final void Function(String url)? onLink;
  @override
  State<TranscriptView> createState() => TranscriptViewState();
}

class TranscriptViewState extends State<TranscriptView> {
  /// Changes when the origin is moved outright, so that nothing laid out from the old one is kept.
  int _epoch = 0;
  Key get _center => ValueKey(('now', _epoch));
  late final ScrollController scroll = _EndController(() => _atEnd);
  late TranscriptRows _rows;

  /// Rows before this one lie above the scroll view's origin: what happens after them cannot move
  /// them, and what is above is built only when someone goes there.
  int _pivot = 0;

  /// How many rows surely fill the viewport.
  int _ahead = 80;
  double _width = 0;
  final _listening = <ItemModel>{};
  final _focus = FocusNode(debugLabel: 'transcript');
  bool _atEnd = true;

  /// What has been opened by hand, kept here so that a row can be rebuilt without losing it.
  final _opened = <ItemModel>{};

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
    _atEnd = true;
    _opened.clear();
    _pivot = _fromEnd();
  }

  int _fromEnd() {
    final n = _rows.rows.length;
    return n > _ahead ? n - _ahead : 0;
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
    if (!identical(oldWidget.agent, widget.agent)) {
      _close(oldWidget.agent);
      _open();
    }
  }

  @override
  void dispose() {
    _close(widget.agent);
    scroll
      ..removeListener(_scrolled)
      ..dispose();
    _focus.dispose();
    super.dispose();
  }

  /// Follows every item that can still change, wherever it stands.
  void _listen() {
    final open = _rows.live;
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
    final held = _atEnd ? null : _firstVisible();
    if (_rows.sync()) {
      _hold(held);
      setState(() {});
    }
    _listen();
  }

  /// The first row in view and where its top is, from the last layout.
  (TranscriptRow, double)? _firstVisible() {
    final view = context.findRenderObject();
    if (view is! RenderBox || !view.hasSize) return null;
    (TranscriptRow, double)? best;
    void visit(RenderObject o) {
      if (o is RenderMetaData && o.metaData is TranscriptRow) {
        if (!o.hasSize || !o.attached) return;
        final y = o.localToGlobal(Offset.zero, ancestor: view).dy;
        if (y + o.size.height > 0 && y < view.size.height && (best == null || y < best!.$2)) {
          best = (o.metaData as TranscriptRow, y);
        }
        return;
      }
      o.visitChildrenForSemantics(visit);
    }

    view.visitChildrenForSemantics(visit);
    return best;
  }

  /// Puts [held] back where it was: it becomes the origin, so nothing above can push it.
  void _hold((TranscriptRow, double)? held) {
    if (held == null || !scroll.hasClients) return;
    final (row, y) = held;
    final rows = _rows.rows;
    var at = row.ordinal;
    if (at >= rows.length || !identical(rows[at], row)) {
      // It was replaced; its successor stands at the same place of the same item.
      at = rows.indexWhere(
        (r) =>
            identical(r.item, row.item) &&
            (r.index == row.index || (r.isEnd && r.index <= row.index) || r.index > row.index),
      );
      if (at < 0) return;
    }
    _pivot = at;
    scroll.position.correctPixels(-y);
  }

  void _scrolled() {
    final p = scroll.position;
    final end = p.pixels >= p.maxScrollExtent - 4;
    if (end != _atEnd) setState(() => _atEnd = end);
  }

  /// Both ends are reached by moving the origin, not by laying out everything between.
  void toStart() => setState(() {
    _epoch++;
    _pivot = 0;
    _atEnd = false;
    scroll.position.correctPixels(0);
  });

  void toEnd() => setState(() {
    _epoch++;
    _pivot = _fromEnd();
    _atEnd = true;
  });

  void _by(double viewports) {
    final p = scroll.position;
    scroll.jumpTo(
      (p.pixels + viewports * p.viewportDimension).clamp(p.minScrollExtent, p.maxScrollExtent),
    );
  }

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, box) {
      final ahead = box.maxHeight.isFinite ? (box.maxHeight / 18).ceil() + 24 : 80;
      if (ahead > _ahead) {
        _ahead = ahead;
        if (_atEnd) _pivot = _fromEnd();
      }
      if (box.maxWidth != _width) {
        if (_width > 0 && !_atEnd) _hold(_firstVisible());
        _width = box.maxWidth;
      }
      return _view(context);
    },
  );

  Widget _view(BuildContext context) {
    final rows = _rows.rows, n = rows.length;
    if (_pivot > n) _pivot = n;
    Widget row(int k) => KeyedSubtree(
      key: ObjectKey(rows[k]),
      child: MetaData(
        metaData: rows[k],
        child: _RowView(
          rows,
          k,
          wide: _width >= 860,
          onLink: widget.onLink,
          opened: _opened.contains,
          onToggle: (i) => setState(() {
            if (!_opened.remove(i)) _opened.add(i);
          }),
        ),
      ),
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
                selectionControls: axSelectionControls,
                contextMenuBuilder: (context, s) =>
                    AxContextMenu(anchors: s.contextMenuAnchors, items: s.contextMenuButtonItems),
                child: CustomScrollView(
                  controller: scroll,
                  center: _center,
                  physics: const ClampingScrollPhysics(),
                  scrollCacheExtent: const ScrollCacheExtent.pixels(400),
                  slivers: [
                    SliverPadding(
                      key: ValueKey(('before', _epoch)),
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

/// Keeps to the end while [atEnd] says so, within the layout that moved it: no frame shows the place
/// before. A person's own scrolling is never overruled.
class _EndController extends ScrollController {
  _EndController(this.atEnd);
  final bool Function() atEnd;
  @override
  ScrollPosition createScrollPosition(
    ScrollPhysics physics,
    ScrollContext context,
    ScrollPosition? oldPosition,
  ) => _EndPosition(atEnd, physics: physics, context: context, oldPosition: oldPosition);
}

class _EndPosition extends ScrollPositionWithSingleContext {
  _EndPosition(this.atEnd, {required super.physics, required super.context, super.oldPosition});
  final bool Function() atEnd;
  @override
  bool applyContentDimensions(double minScrollExtent, double maxScrollExtent) {
    final follow = atEnd() && !isScrollingNotifier.value;
    if (follow && (!hasPixels || pixels != maxScrollExtent)) {
      correctPixels(maxScrollExtent);
      super.applyContentDimensions(minScrollExtent, maxScrollExtent);
      return false;
    }
    return super.applyContentDimensions(minScrollExtent, maxScrollExtent);
  }
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
  const _RowView(
    this.rows,
    this.at, {
    required this.wide,
    required this.opened,
    required this.onToggle,
    this.onLink,
  });
  final List<TranscriptRow> rows;
  final int at;
  final bool wide;
  final bool Function(ItemModel) opened;
  final void Function(ItemModel) onToggle;
  final void Function(String url)? onLink;

  /// Prose and work side by side, at the proportions of the accepted design.
  static Widget columns(Widget left, Widget right) => Row(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Expanded(flex: 54, child: _Unit(left)),
      const SizedBox(width: 44),
      Expanded(flex: 46, child: _Unit(right)),
    ],
  );

  @override
  Widget build(BuildContext context) {
    final r = rows[at], i = r.item;
    final before = at > 0 ? rows[at - 1] : null;
    final afterWork = before != null && before.work.isNotEmpty;
    if (i != null && !r.isEnd) {
      final blocks = i.doc.closed;
      final w = BlockView(
        blocks[r.index],
        voice: i.kind,
        opens: r.index == 0,
        afterTool: r.index == 0 && afterWork && !wide,
        goesOn: r.index + 1 < blocks.length && blocks[r.index + 1].continues,
        closes: r.index + 1 == blocks.length && i.doc.tail.isEmpty,
        whole: () => wholeCode(blocks, r.index),
        onLink: onLink,
      );
      return RepaintBoundary(child: wide ? columns(w, const SizedBox.shrink()) : w);
    }
    Widget prose() {
      if (i == null) return const SizedBox(width: double.infinity);
      Widget build(BuildContext _, [Widget? _]) {
        final d = i.doc;
        final tail = d.tail;
        final all = [for (var k = r.index; k < d.closed.length; k++) d.closed[k], ...tail];
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            for (final (k, b) in all.indexed)
              BlockView(
                b,
                voice: i.kind,
                opens: r.index + k == 0,
                afterTool: r.index + k == 0 && afterWork && !wide,
                goesOn: k + 1 < all.length && all[k + 1].continues,
                closes: k + 1 == all.length,
                whole: () => wholeCode([...d.closed, ...tail], r.index + k),
                onLink: onLink,
              ),
          ],
        );
      }

      return i.open ? ListenableBuilder(listenable: i, builder: build) : build(context);
    }

    if (r.work.isEmpty) {
      final w = prose();
      return RepaintBoundary(child: wide ? columns(w, const SizedBox.shrink()) : w);
    }
    final first = r.from == 0;
    // The first line of the work stands on the first line of the words.
    final top = wide
        ? (!first
              ? 2.0
              : i == null || r.index == 0
              ? 17.0
              : 10.0)
        : at == 0
        ? 16.0
        : first
        ? 10.0
        : 2.0;
    final work = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (var k = r.from; k < r.to; k++)
          Padding(
            padding: EdgeInsets.only(top: k == r.from ? top : 2),
            child: RepaintBoundary(
              child: ListenableBuilder(
                key: ObjectKey(r.work[k]),
                listenable: r.work[k],
                builder: (_, _) => ToolRow(
                  r.work[k],
                  opened: opened(r.work[k]),
                  onToggle: () => onToggle(r.work[k]),
                ),
              ),
            ),
          ),
      ],
    );
    return RepaintBoundary(
      child: wide
          ? columns(first ? prose() : const SizedBox.shrink(), work)
          : Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [if (first) prose(), work],
            ),
    );
  }
}

/// Copied as a whole before what stands beside it.
class _Unit extends StatefulWidget {
  const _Unit(this.child);
  final Widget child;
  @override
  State<_Unit> createState() => _UnitState();
}

class _UnitState extends State<_Unit> {
  final _delegate = StaticSelectionContainerDelegate();
  @override
  void dispose() {
    _delegate.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) =>
      SelectionContainer(delegate: _delegate, child: widget.child);
}

/// All of a code block that was cut into parts, from any of them.
String wholeCode(List<Block> blocks, int at) {
  var from = at;
  while (from > 0 && blocks[from].continues) {
    from--;
  }
  var to = at + 1;
  while (to < blocks.length && blocks[to].continues) {
    to++;
  }
  return blocks.sublist(from, to).map((b) => b.text).join('\n');
}

class BlockView extends StatefulWidget {
  const BlockView(
    this.block, {
    required this.voice,
    super.key,
    this.opens = false,
    this.afterTool = false,
    this.goesOn = false,
    this.closes = false,
    this.whole,
    this.onLink,
  });
  final String Function()? whole;
  final Block block;
  final ItemKind voice;

  /// First of its item; after a run of tools; continued by the next block; last of its item.
  final bool opens, afterTool, goesOn, closes;
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
    final code = t.type.code.copyWith(height: base.height, color: c.codeInline, fontSize: 12.5);
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
          _Marker(b.marker, base),
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
      BlockKind.code => CodeBlock(b, goesOn: widget.goesOn, whole: widget.whole),
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
    final top = b.continues
        ? 0.0
        : widget.opens
        ? (widget.afterTool ? 14.0 : 18.0)
        : b.kind == BlockKind.heading
        ? 20.0
        : b.kind == BlockKind.item
        ? 6.0
        : 11.0;
    final asked = widget.voice == ItemKind.prompt;
    if (asked) child = _Voice(child, opens: widget.opens, closes: widget.closes, gap: top);
    child = Padding(
      // The tile begins above its words, which stay on the line any others would.
      padding: EdgeInsets.only(
        top: !asked
            ? top
            : widget.opens
            ? top - _Voice.pad
            : 0,
      ),
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: _wide),
        child: child,
      ),
    );
    child = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Align(alignment: Alignment.topLeft, heightFactor: 1, child: child),
        // At the right, so that it is read after the words and not among them.
        SizedBox(
          width: double.infinity,
          child: Align(
            alignment: Alignment.topRight,
            heightFactor: 1,
            child: CopyGap(widget.goesOn || b.kind == BlockKind.item ? '\n' : '\n\n'),
          ),
        ),
      ],
    );
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

/// What was asked stands on a tile of its own, so it is found when scrolling back. The words
/// themselves are as strong as any others.
class _Voice extends StatelessWidget {
  const _Voice(this.child, {required this.opens, required this.closes, required this.gap});
  final Widget child;
  final bool opens, closes;
  final double gap;
  static const pad = 8.0;
  @override
  Widget build(BuildContext context) {
    final c = Ax.of(context).colors;
    const r = Radius.circular(AxRadius.plate);
    return DecoratedBox(
      decoration: BoxDecoration(
        color: c.block.top,
        borderRadius: BorderRadius.vertical(
          top: opens ? r : Radius.zero,
          bottom: closes ? r : Radius.zero,
        ),
      ),
      child: SizedBox(
        width: double.infinity,
        child: Padding(
          padding: EdgeInsets.fromLTRB(12, opens ? pad : gap, 12, closes ? pad : 0),
          child: child,
        ),
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
          width: 22,
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
    final bullet = marker == '•';
    // Wide enough for any number, with the same gap before the words.
    return ConstrainedBox(
      constraints: BoxConstraints(minWidth: bullet ? 22 : 30),
      child: Text(
        '$marker ',
        softWrap: false,
        textAlign: bullet ? TextAlign.left : TextAlign.right,
        style: style.copyWith(color: c.ink4, wordSpacing: bullet ? 0 : 4),
      ),
    );
  }
}

class CodeBlock extends StatefulWidget {
  const CodeBlock(this.block, {super.key, this.goesOn = false, this.whole});
  final Block block;
  final bool goesOn;

  /// What Copy copies, if it is more than is shown here.
  final String Function()? whole;
  static const maxColumns = 2000;
  @override
  State<CodeBlock> createState() => _CodeBlockState();
}

class _CodeBlockState extends State<CodeBlock> {
  bool _over = false, _within = false, _copied = false;

  Future<void> _copy() async {
    // What was written, all of it, not what is shown.
    await Clipboard.setData(ClipboardData(text: widget.whole?.call() ?? widget.block.text));
    if (mounted) setState(() => _copied = true);
  }

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
                                ? '${cutTo(l, CodeBlock.maxColumns)} … '
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
              child: SelectionContainer.disabled(
                child: Focus(
                  canRequestFocus: false,
                  skipTraversal: true,
                  onFocusChange: (v) => setState(() => _within = v),
                  child: Opacity(
                    opacity: _over || _within ? 1 : 0,
                    child: AxButton(label: _copied ? 'Copied' : 'Copy', size: 22, onPressed: _copy),
                  ),
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
    return [for (final e in out.take(maxColumns)) e.length > maxCell ? '${cutTo(e, maxCell)}…' : e];
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
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Flexible(
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
                          CopyGap(k + 1 < n ? '\t' : '\n'),
                        ],
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
class ToolRow extends StatelessWidget {
  const ToolRow(this.item, {super.key, this.opened = false, this.onToggle});
  final ItemModel item;
  final bool opened;
  final VoidCallback? onToggle;
  static const maxOutput = 20000;

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
    final t = Ax.of(context), c = t.colors;
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
      more: output.isEmpty ? null : opened,
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
                hint: opened ? 'Hide output' : 'Show output',
                expanded: opened,
                radius: 0,
                onPressed: onToggle ?? () {},
                builder: (_, s) =>
                    ColoredBox(color: s.hovered ? c.hover : c.bay.bottom, child: line),
              ),
            if (opened && output.isNotEmpty)
              Padding(
                padding: const EdgeInsets.fromLTRB(22, 4, 0, 6),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    if (output.length > maxOutput)
                      Padding(
                        padding: const EdgeInsets.only(bottom: 4),
                        child: Text(
                          'The last part of ${output.length} characters. Copy takes all of them.',
                          style: t.type.small.ink(c.ink3),
                        ),
                      ),
                    CodeBlock(
                      Block(BlockKind.code, cutFrom(output, maxOutput)),
                      whole: () => output,
                    ),
                  ],
                ),
              ),
            const CopyGap('\n'),
          ],
        ),
      ),
    );
  }
}
