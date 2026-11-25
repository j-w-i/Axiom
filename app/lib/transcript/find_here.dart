import 'dart:async';
import 'dart:isolate';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../host/session_model.dart';
import '../ui/commands.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';
import 'transcript_view.dart';

/// A place in a transcript: an item, and which of its parts.
typedef Place = (int item, int part);

/// More places than this are not gone through.
const mostPlaces = 5000;
final _marks = RegExp(r'[*_`~]');

/// Where [words] stand in what was said, in order: as it is written, or as it is read, without the
/// marks of Markdown. Large and small letters are the same.
List<Place> placesOf(List<List<String>> all, String words) {
  final w = words.toLowerCase(), bare = w.replaceAll(_marks, '');
  final out = <Place>[];
  if (w.trim().isEmpty) return out;
  for (var i = 0; i < all.length; i++) {
    for (var k = 0; k < all[i].length; k++) {
      final t = all[i][k].toLowerCase();
      final here = t.contains(w) || (bare.isNotEmpty && t.replaceAll(_marks, '').contains(bare));
      if (!here) continue;
      out.add((i, k));
      if (out.length == mostPlaces) return out;
    }
  }
  return out;
}

/// What an item is searched in: the blocks of what was said; of work, its name, what it was given
/// and what it gave.
List<String> partsOf(ItemModel i) => i.rich
    ? [for (final b in i.doc.closed) b.text, for (final b in i.doc.tail) b.text]
    : ['${i.name} ${i.args ?? ''}\n${i.plain}\n${i.result ?? ''}'];

/// A transcript that can be searched: all of it, also what is not built.
class FindHere extends StatefulWidget {
  const FindHere(this.agent, {required this.builder, super.key});
  final AgentModel agent;
  final Widget Function(GlobalKey<TranscriptViewState>) builder;
  @override
  State<FindHere> createState() => _FindHereState();
}

class _FindHereState extends State<FindHere> {
  final _view = GlobalKey<TranscriptViewState>();
  final _words = TextEditingController(), _focus = FocusNode(debugLabel: 'find here');
  bool _open = false;
  List<Place> _all = const [];
  int _at = -1;
  Timer? _soon;

  /// What was asked for last: an answer to anything before that is not shown.
  int _asked = 0;
  bool _looking = false;

  @override
  void initState() {
    super.initState();
    _words.addListener(_typed);
    widget.agent.addListener(_grew);
  }

  @override
  void didUpdateWidget(FindHere old) {
    super.didUpdateWidget(old);
    if (identical(old.agent, widget.agent)) return;
    old.agent.removeListener(_grew);
    widget.agent.addListener(_grew);
    _asked++;
    _all = const [];
    _at = -1;
    _was = '';
    if (_open) _typed();
  }

  @override
  void dispose() {
    _soon?.cancel();
    widget.agent.removeListener(_grew);
    _words.dispose();
    _focus.dispose();
    super.dispose();
  }

  String _was = '';
  void _typed() {
    if (_words.text == _was) return;
    _was = _words.text;
    _soon?.cancel();
    _soon = Timer(const Duration(milliseconds: 120), () => unawaited(_look(go: true)));
  }

  /// What is said meanwhile is searched too, not more often than is of use. Nothing moves for it.
  void _grew() {
    if (!_open || _was.trim().isEmpty || (_soon?.isActive ?? false)) return;
    _soon = Timer(const Duration(seconds: 1), () => unawaited(_look(go: false)));
  }

  Future<void> _look({required bool go}) async {
    final asked = ++_asked, words = _was;
    if (words.trim().isEmpty) {
      setState(() {
        _all = const [];
        _at = -1;
      });
      _view.currentState?.mark(null, 0);
      return;
    }
    final all = [for (final i in widget.agent.items) partsOf(i)];
    setState(() {
      _looking = true;
      // What was found for other words is not what is looked for now.
      if (go) {
        _all = const [];
        _at = -1;
      }
    });
    // Texts are shared with who searches them, not copied.
    final found = await _search(all, words);
    if (!mounted || asked != _asked) return;
    final was = _at >= 0 && _at < _all.length ? _all[_at] : null;
    setState(() {
      _looking = false;
      _all = found;
      _at = go ? (found.isEmpty ? -1 : 0) : (was == null ? -1 : found.indexOf(was));
    });
    if (go) _show();
  }

  static Future<List<Place>> _search(List<List<String>> all, String words) =>
      Isolate.run(() => placesOf(all, words));

  void _show() {
    final v = _view.currentState;
    if (_at < 0) return v?.mark(null, 0);
    final (i, k) = _all[_at];
    if (i < widget.agent.items.length) v?.mark(widget.agent.items[i], k);
  }

  void _by(int d) {
    if (_all.isEmpty) return;
    setState(() => _at = _at < 0 ? 0 : (_at + d) % _all.length);
    _show();
  }

  void _begin() {
    setState(() => _open = true);
    _words.selection = TextSelection(baseOffset: 0, extentOffset: _words.text.length);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _focus.requestFocus();
    });
    if (_was.trim().isNotEmpty) unawaited(_look(go: false));
  }

  void _close() {
    _soon?.cancel();
    _asked++;
    setState(() => _open = _looking = false);
    _view.currentState?.mark(null, 0);
  }

  void _enter(int d) {
    final composing = _words.value.composing;
    if (!composing.isValid || composing.isCollapsed) _by(d);
  }

  String get _count {
    if (_was.trim().isEmpty) return '';
    if (_looking && _all.isEmpty) return 'Searching';
    if (_all.isEmpty) return 'No results';
    final n = _all.length == mostPlaces ? '$mostPlaces or more' : '${_all.length}';
    return _at < 0 ? n : '${_at + 1} of $n';
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return OnCommand(
      Command.findHere,
      _begin,
      child: Stack(
        fit: StackFit.expand,
        children: [
          widget.builder(_view),
          if (_open)
            Positioned(
              top: 8,
              right: AxSpace.edge,
              left: AxSpace.edge,
              child: Align(
                alignment: Alignment.topRight,
                heightFactor: 1,
                child: ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 400),
                  child: CallbackShortcuts(
                    bindings: {
                      const SingleActivator(LogicalKeyboardKey.escape): _close,
                      const SingleActivator(LogicalKeyboardKey.enter): () => _enter(1),
                      const SingleActivator(LogicalKeyboardKey.enter, shift: true): () =>
                          _enter(-1),
                    },
                    child: Semantics(
                      container: true,
                      explicitChildNodes: true,
                      label: 'Find in the conversation',
                      child: Surface(
                        c.overlay,
                        radius: AxRadius.card,
                        child: Padding(
                          padding: const EdgeInsets.all(6),
                          child: Row(
                            children: [
                              Expanded(
                                child: AxField(
                                  label: 'Find in the conversation',
                                  placeholder: 'Find in the conversation',
                                  controller: _words,
                                  focusNode: _focus,
                                  leading: AxGlyph(AxIcon.search, color: c.ink3),
                                ),
                              ),
                              if (_count.isNotEmpty) ...[
                                const SizedBox(width: 10),
                                Semantics(
                                  liveRegion: true,
                                  child: Text(_count, style: t.type.small.ink(c.ink3)),
                                ),
                              ],
                              const SizedBox(width: 6),
                              AxButton.icon(
                                AxIcon.up,
                                label: 'Previous match',
                                tone: AxTone.quiet,
                                onPressed: _all.isEmpty ? null : () => _by(-1),
                              ),
                              AxButton.icon(
                                AxIcon.down,
                                label: 'Next match',
                                tone: AxTone.quiet,
                                onPressed: _all.isEmpty ? null : () => _by(1),
                              ),
                              AxButton.icon(
                                AxIcon.close,
                                label: 'Close',
                                tone: AxTone.quiet,
                                onPressed: _close,
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
        ],
      ),
    );
  }
}
