import 'dart:async';

import 'package:flutter/widgets.dart';

import '../host/host_store.dart';
import '../host/proto.dart';
import '../host/session_model.dart';
import '../patterns/transcript_parts.dart';
import '../transcript/safe_text.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/press.dart';
import '../ui/tokens.dart';
import 'start.dart';

/// What has been changed in the session's folder since its last commit. Looked at when it is shown,
/// when a run ends, and when its person asks.
class ChangesView extends StatefulWidget {
  const ChangesView(this.store, this.session, {super.key});
  final HostStore store;
  final SessionModel session;
  @override
  State<ChangesView> createState() => _ChangesViewState();
}

class _ChangesViewState extends State<ChangesView> {
  Changes? _all;
  List<(Changed, FileDiff)> _files = const [];
  String? _problem;
  bool _looking = false, _again = false;
  int _running = 0;
  final _shut = <String>{};

  @override
  void initState() {
    super.initState();
    widget.session.addListener(_changed);
    _running = _atWork;
    unawaited(_look());
  }

  @override
  void dispose() {
    widget.session.removeListener(_changed);
    super.dispose();
  }

  int get _atWork => widget.session.agents.values
      .where((a) => a.phase == AgentPhase.running || a.phase == AgentPhase.asking)
      .length;

  void _changed() {
    final now = _atWork;
    if (now < _running) unawaited(_look());
    _running = now;
  }

  Future<void> _look() async {
    if (_looking) {
      _again = true;
      return;
    }
    setState(() => _looking = true);
    try {
      do {
        _again = false;
        final c = await widget.store.changes(widget.session.id);
        if (!mounted) return;
        setState(() {
          _all = c;
          _files = _read(c);
          _problem = null;
        });
      } while (_again);
    } on Object catch (e) {
      if (mounted) setState(() => _problem = wordsOf(e));
    } finally {
      if (mounted) setState(() => _looking = false);
    }
  }

  static List<(Changed, FileDiff)> _read(Changes c) {
    final all = {for (final f in readDiffs(c.diff, most: 600)) f.path: f};
    return [for (final f in c.files) (f, all[f.path] ?? FileDiff(f.path))];
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors, all = _all;
    final words = all == null
        ? (_problem ?? 'Looking')
        : !all.repository
        ? 'The folder is not a git repository, so what has changed in it is not known.'
        : _files.isEmpty
        ? 'Nothing has changed since the last commit.'
        : null;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(24, 10, 18, 8),
          child: Row(
            children: [
              if (all?.branch case final b?) ...[
                AxGlyph(AxIcon.branch, size: 13, color: c.ink3),
                const SizedBox(width: 7),
                Flexible(
                  child: Text(
                    oneLine(literal(b), 80),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: t.type.label.ink(c.ink2),
                  ),
                ),
                const SizedBox(width: 14),
              ],
              if (_files.isNotEmpty)
                Text(
                  '${_files.length} ${_files.length == 1 ? 'file' : 'files'}',
                  style: t.type.label.ink(c.ink3),
                ),
              const Spacer(),
              AxButton(
                label: _looking ? 'Looking' : 'Look again',
                tone: AxTone.quiet,
                onPressed: _looking ? null : () => unawaited(_look()),
              ),
            ],
          ),
        ),
        ColoredBox(color: c.bayRule, child: const SizedBox(height: 1)),
        if (_problem != null && all != null)
          Padding(
            padding: const EdgeInsets.fromLTRB(24, 10, 24, 0),
            child: Text(visible(_problem!), style: t.type.ui.ink(c.faultText)),
          ),
        Expanded(
          child: words != null
              ? Align(
                  alignment: Alignment.topLeft,
                  child: Padding(
                    padding: const EdgeInsets.fromLTRB(24, 16, 24, 16),
                    child: Text(
                      visible(words),
                      style: t.type.ui.ink(_problem != null && all == null ? c.faultText : c.ink3),
                    ),
                  ),
                )
              : ListView.builder(
                  padding: const EdgeInsets.fromLTRB(18, 8, 24, 24),
                  itemCount: _files.length + (all!.whole ? 0 : 1),
                  itemBuilder: (context, i) => i == _files.length
                      ? Padding(
                          padding: const EdgeInsets.fromLTRB(6, 10, 0, 0),
                          child: Text(
                            'There is more than can be shown here.',
                            style: t.type.label.ink(c.ink3),
                          ),
                        )
                      : _file(context, _files[i]),
                ),
        ),
      ],
    );
  }

  Widget _file(BuildContext context, (Changed, FileDiff) f) {
    final t = Ax.of(context), c = t.colors;
    final (file, diff) = f;
    final lines = diff.lines, add = diff.added, remove = diff.removed, left = diff.left;
    final open = !_shut.contains(file.path) && lines.isNotEmpty;
    final name = oneLine(literal(file.path), 300);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Pressable(
          label: '$name, ${file.what}, $add added, $remove removed',
          expanded: open,
          onPressed: lines.isEmpty
              ? null
              : () => setState(() {
                  _shut.contains(file.path) ? _shut.remove(file.path) : _shut.add(file.path);
                }),
          builder: (_, s) => Padding(
            padding: const EdgeInsets.fromLTRB(6, 7, 6, 7),
            child: Row(
              children: [
                AxGlyph(open ? AxIcon.down : AxIcon.right, size: 9, color: c.ink4),
                const SizedBox(width: 9),
                Flexible(
                  child: Text(
                    name,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: t.type.codeSmall.ink(s.hovered ? c.ink1 : c.ink2),
                  ),
                ),
                const SizedBox(width: 10),
                if (file.what != 'changed')
                  Text(
                    file.what,
                    style: t.type.small.ink(file.what == 'conflict' ? c.attentionText : c.ink3),
                  ),
                const Spacer(),
                if (add > 0) Text('+$add', style: t.type.small.ink(c.doneText)),
                if (remove > 0) ...[
                  const SizedBox(width: 8),
                  Text('−$remove', style: t.type.small.ink(c.faultText)),
                ],
                if (lines.isEmpty) Text('not shown', style: t.type.small.ink(c.ink4)),
              ],
            ),
          ),
        ),
        if (open)
          Padding(
            padding: const EdgeInsets.fromLTRB(24, 0, 0, 8),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                DiffLines(lines),
                if (left > 0)
                  Padding(
                    padding: const EdgeInsets.only(top: 4),
                    child: Text('$left more lines', style: t.type.label.ink(c.ink3)),
                  ),
              ],
            ),
          ),
      ],
    );
  }
}
