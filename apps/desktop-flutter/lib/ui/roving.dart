// One Tab stop per list (AUD-021): Tab lands on the list, which hands focus to its current row;
// the list's own arrow keys move between rows; Tab and Shift+Tab leave the list. Rows are out of
// the Tab order (`skipTraversal`), so a list of 3 000 rows costs one Tab press, not 3 000.

import 'package:flutter/widgets.dart';

/// Wraps a list whose rows are out of the Tab order. When Tab (or Shift+Tab) reaches the list,
/// [onEnter] moves focus to the row that should have it (the selected one, else the first),
/// scrolling it into view first when needed. While a row has focus the list itself is not a
/// Tab stop, so Shift+Tab from a row leaves the list instead of coming back to it.
class RovingFocus extends StatefulWidget {
  const RovingFocus({
    super.key,
    required this.onEnter,
    required this.child,
    this.enabled = true,
    this.onFocusChange,
  });

  /// Focus the current row (called when the list itself receives focus).
  final VoidCallback onEnter;
  final Widget child;

  /// False for an empty list: no Tab stop at all.
  final bool enabled;

  /// Whether focus is anywhere in the list (a row's selected marker is brass then).
  final ValueChanged<bool>? onFocusChange;

  @override
  State<RovingFocus> createState() => _RovingFocusState();
}

class _RovingFocusState extends State<RovingFocus> {
  late final FocusNode _entry = FocusNode(debugLabel: 'list', canRequestFocus: widget.enabled)
    ..addListener(_changed);
  bool _had = false;

  void _changed() {
    final inside = _entry.hasFocus && !_entry.hasPrimaryFocus;
    // Not a Tab stop while one of its rows has focus.
    _entry.skipTraversal = inside;
    if (_entry.hasPrimaryFocus) {
      widget.onEnter();
    }
    if (_had != _entry.hasFocus) {
      _had = _entry.hasFocus;
      widget.onFocusChange?.call(_had);
    }
  }

  @override
  void didUpdateWidget(RovingFocus old) {
    super.didUpdateWidget(old);
    _entry.canRequestFocus = widget.enabled;
  }

  @override
  void dispose() {
    _entry
      ..removeListener(_changed)
      ..dispose();
    super.dispose();
  }

  @override
  // The list's own node takes Tab and hands focus on; it is not a control of its own for a
  // screen reader (the rows are), so it adds no semantics node.
  Widget build(BuildContext context) =>
      Focus(focusNode: _entry, includeSemantics: false, child: widget.child);
}

/// Focus nodes keyed by row id for a lazily built list, out of the Tab order. Nodes of rows that
/// are no longer built (scrolled away, or gone from the list) are disposed after the frame, so a
/// long scroll keeps only the built rows' nodes (AUD-083).
class RowFocusNodes {
  RowFocusNodes({this.debugPrefix = 'row'});
  final String debugPrefix;
  final _nodes = <String, FocusNode>{};
  bool _pruneScheduled = false;
  bool _disposed = false;

  /// The node for [id] (created out of the Tab order). Schedules a prune after the frame.
  FocusNode operator [](String id) {
    _schedulePrune();
    return _nodes.putIfAbsent(
      id,
      () => FocusNode(debugLabel: '$debugPrefix $id', skipTraversal: true),
    );
  }

  /// The node for [id] if one exists.
  FocusNode? existing(String id) => _nodes[id];

  int get length => _nodes.length;

  void _schedulePrune() {
    if (_pruneScheduled || _disposed) return;
    _pruneScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _pruneScheduled = false;
      prune();
    });
  }

  /// Disposes the nodes not attached to a built row (and not focused).
  void prune() {
    if (_disposed) return;
    _nodes.removeWhere((_, n) {
      final ctx = n.context;
      if ((ctx == null || !ctx.mounted) && !n.hasFocus) {
        n.dispose();
        return true;
      }
      return false;
    });
  }

  void dispose() {
    _disposed = true;
    for (final n in _nodes.values) {
      n.dispose();
    }
    _nodes.clear();
  }
}
