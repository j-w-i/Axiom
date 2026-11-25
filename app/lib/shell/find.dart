import 'dart:math' as math;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/marks.dart';
import '../ui/press.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';

/// Somewhere to go, or something to do.
class Found {
  const Found(this.label, this.go, {this.detail, this.icon, this.mark, this.keys});
  final String label;
  final String? detail, keys;
  final AxIcon? icon;
  final ServiceMark? mark;
  final VoidCallback go;
}

/// Everything there is to go to, narrowed down by what is written. The arrow keys choose, Enter goes,
/// Escape leaves.
class SearchBox extends StatefulWidget {
  const SearchBox({required this.all, required this.onClose, super.key});
  final List<Found> all;
  final VoidCallback onClose;

  /// More than this are only counted.
  static const most = 30;
  @override
  State<SearchBox> createState() => _SearchBoxState();
}

class _SearchBoxState extends State<SearchBox> {
  final _words = TextEditingController(), _focus = FocusNode(debugLabel: 'find');
  final _scroll = ScrollController();
  FocusNode? _back;
  int _at = 0;
  static const _row = 34.0;

  @override
  void initState() {
    super.initState();
    _back = FocusManager.instance.primaryFocus;
    _words.addListener(_changed);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _focus.requestFocus();
    });
  }

  String _was = '';
  void _changed() {
    if (_words.text == _was) return;
    setState(() {
      _was = _words.text;
      _at = 0;
    });
  }

  @override
  void dispose() {
    _words.dispose();
    _focus.dispose();
    _scroll.dispose();
    super.dispose();
  }

  List<Found> get _fit {
    final words = _words.text.toLowerCase().split(' ').where((w) => w.isNotEmpty).toList();
    return [
      for (final f in widget.all)
        if (words.every('${f.label} ${f.detail ?? ''}'.toLowerCase().contains)) f,
    ];
  }

  void _leave() {
    widget.onClose();
    _back?.requestFocus();
  }

  void _go(Found f) {
    widget.onClose();
    f.go();
  }

  void _by(int d) {
    final n = math.min(_fit.length, SearchBox.most);
    if (n == 0) return;
    setState(() => _at = (_at + d).clamp(0, n - 1));
    if (!_scroll.hasClients) return;
    final p = _scroll.position, top = _at * _row;
    if (top < p.pixels) _scroll.jumpTo(top);
    if (top + _row > p.pixels + p.viewportDimension) {
      _scroll.jumpTo(math.min(top + _row - p.viewportDimension, p.maxScrollExtent));
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final fit = _fit, shown = fit.take(SearchBox.most).toList();
    return Stack(
      children: [
        Positioned.fill(
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            excludeFromSemantics: true,
            onTap: _leave,
          ),
        ),
        Align(
          alignment: const Alignment(0, -.55),
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 520),
              child: CallbackShortcuts(
                bindings: {
                  const SingleActivator(LogicalKeyboardKey.escape): _leave,
                  const SingleActivator(LogicalKeyboardKey.arrowDown): () => _by(1),
                  const SingleActivator(LogicalKeyboardKey.arrowUp): () => _by(-1),
                  const SingleActivator(LogicalKeyboardKey.enter): () {
                    final composing = _words.value.composing;
                    if (composing.isValid && !composing.isCollapsed) return;
                    if (_at < shown.length) _go(shown[_at]);
                  },
                },
                child: FocusScope(
                  child: Semantics(
                    scopesRoute: true,
                    explicitChildNodes: true,
                    label: 'Search',
                    child: Surface(
                      c.overlay,
                      radius: AxRadius.card,
                      child: Padding(
                        padding: const EdgeInsets.all(6),
                        child: Column(
                          mainAxisSize: MainAxisSize.min,
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            AxField(
                              label: 'Search',
                              placeholder: 'Search sessions and actions',
                              controller: _words,
                              focusNode: _focus,
                              leading: AxGlyph(AxIcon.search, color: c.ink3),
                            ),
                            const SizedBox(height: 6),
                            if (shown.isEmpty)
                              Padding(
                                padding: const EdgeInsets.all(8),
                                child: Text('No results', style: t.type.ui.ink(c.ink3)),
                              ),
                            Flexible(
                              child: ConstrainedBox(
                                constraints: const BoxConstraints(maxHeight: _row * 9),
                                child: ListView.builder(
                                  controller: _scroll,
                                  shrinkWrap: true,
                                  itemCount: shown.length,
                                  itemExtent: _row,
                                  itemBuilder: (context, i) => _line(t, shown[i], i),
                                ),
                              ),
                            ),
                            if (fit.length > shown.length)
                              Padding(
                                padding: const EdgeInsets.fromLTRB(10, 6, 10, 4),
                                child: Text(
                                  '${fit.length - shown.length} more',
                                  style: t.type.small.ink(c.ink3),
                                ),
                              ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }

  Widget _line(AxTheme t, Found f, int i) {
    final c = t.colors;
    return Pressable(
      label: f.label,
      hint: f.detail,
      selected: i == _at,
      onPressed: () => _go(f),
      builder: (context, s) => DecoratedBox(
        decoration: BoxDecoration(
          color: i == _at
              ? c.selected
              : s.hovered
              ? c.hover
              : null,
          borderRadius: BorderRadius.circular(AxRadius.key),
        ),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 10),
          child: Row(
            children: [
              SizedBox(
                width: 24,
                child: Align(
                  alignment: Alignment.centerLeft,
                  child: f.mark == null
                      ? AxGlyph(f.icon ?? AxIcon.chat, color: c.ink3)
                      : AxServiceMark(f.mark!, size: 14),
                ),
              ),
              Expanded(
                child: Text.rich(
                  TextSpan(
                    children: [
                      TextSpan(text: f.label),
                      if (f.detail case final d?)
                        TextSpan(text: '   $d', style: t.type.label.ink(c.ink3)),
                    ],
                  ),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: t.type.ui,
                ),
              ),
              const SizedBox(width: 10),
              if (f.keys case final k?) Text(k, style: t.type.small.ink(c.ink3)),
            ],
          ),
        ),
      ),
    );
  }
}
