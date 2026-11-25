import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../host/host_store.dart';
import '../host/proto.dart';
import '../host/session_model.dart';
import '../patterns/models.dart';
import '../patterns/transcript_parts.dart';
import '../transcript/safe_text.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/press.dart';
import '../ui/tokens.dart';
import 'drafts.dart';
import 'start.dart';

/// What has been changed in the session's folder since its last commit. Looked at when it is shown,
/// when a run ends, and when its person asks.
class ChangesView extends StatefulWidget {
  const ChangesView(
    this.store,
    this.session, {
    required this.agent,
    required this.draft,
    super.key,
  });

  /// Who is told of what is said about a line, and where that is written.
  final String agent;
  final Draft draft;
  final HostStore store;
  final SessionModel session;
  @override
  State<ChangesView> createState() => _ChangesViewState();
}

class _ChangesViewState extends State<ChangesView> with _Remarks<ChangesView> {
  @override
  (String, Draft) get told => (widget.agent, widget.draft);

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
              Expanded(
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
                  ],
                ),
              ),
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

  Widget _file(BuildContext context, (Changed, FileDiff) f) => _FileRow(
    f.$1,
    f.$2,
    remarks: this,
    open: !_shut.contains(f.$1.path),
    onOpen: () => setState(() {
      _shut.contains(f.$1.path) ? _shut.remove(f.$1.path) : _shut.add(f.$1.path);
    }),
  );
}

/// A file, and what has changed in it.
class _FileRow extends StatelessWidget {
  const _FileRow(
    this.file,
    this.diff, {
    required this.open,
    required this.onOpen,
    required this.remarks,
    this.digest,
    this.leading,
    this.notes = const [],
  });
  final _Remarks remarks;

  /// Of which content it is.
  final String? digest;
  final Changed file;
  final FileDiff diff;
  final bool open;
  final VoidCallback onOpen;
  final Widget? leading;

  /// What is to be known of it, and whether somebody should look.
  final List<(String, bool)> notes;

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final lines = diff.lines, add = diff.added, remove = diff.removed, left = diff.left;
    final open = this.open && lines.isNotEmpty;
    final name = oneLine(literal(file.path), 300);
    final row = Pressable(
      label: '$name, ${file.what}, $add added, $remove removed',
      expanded: open,
      onPressed: lines.isEmpty ? null : onOpen,
      builder: (_, s) => Padding(
        padding: const EdgeInsets.fromLTRB(6, 8, 6, 8),
        child: Row(
          children: [
            AxGlyph(open ? AxIcon.down : AxIcon.right, size: 9, color: s.hovered ? c.ink1 : c.ink4),
            const SizedBox(width: 9),
            Expanded(
              child: Row(
                children: [
                  Flexible(
                    child: Text(
                      name,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: t.type.code.ink(c.ink1),
                    ),
                  ),
                  const SizedBox(width: 10),
                  if (file.what != 'changed')
                    Text(
                      file.what,
                      style: t.type.small.ink(file.what == 'conflict' ? c.attentionText : c.ink3),
                    ),
                ],
              ),
            ),
            if (add > 0) Text('+$add', style: t.type.small.ink(c.doneText)),
            if (remove > 0) ...[
              const SizedBox(width: 8),
              Text('−$remove', style: t.type.small.ink(c.faultText)),
            ],
            if (lines.isEmpty) Text('not shown', style: t.type.small.ink(c.ink3)),
          ],
        ),
      ),
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            ?leading,
            Expanded(child: row),
            if (remarks._added == file.path)
              Padding(
                padding: const EdgeInsets.only(left: 4),
                child: Semantics(
                  liveRegion: true,
                  child: Text('In your message', style: t.type.small.ink(c.ink3)),
                ),
              ),
            if (open)
              AxButton.icon(
                AxIcon.edit,
                label: 'Say something about $name',
                tone: AxTone.quiet,
                size: 24,
                onPressed: () => remarks.begin(file.path, null),
              ),
          ],
        ),
        for (final (note, look) in notes)
          Padding(
            padding: const EdgeInsets.fromLTRB(50, 0, 6, 5),
            child: Text(visible(note), style: t.type.label.ink(look ? c.attentionText : c.ink3)),
          ),
        if (open)
          Padding(
            padding: EdgeInsets.fromLTRB(leading == null ? 24 : 50, 0, 0, 8),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                DiffLines(
                  lines,
                  onLine: (l) => remarks.begin(file.path, l),
                  at: remarks.lineOf(file.path, lines),
                  note: remarks.isOn(file.path) ? remarks.editor(context, digest) : null,
                ),
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

/// What a person says about a line of a file: written here, and added to what they are saying to the
/// agent. Nothing is sent. One at a time; looking again does not lose it.
mixin _Remarks<W extends StatefulWidget> on State<W> {
  final _remark = TextEditingController(), _remarkFocus = FocusNode(debugLabel: 'remark');
  String? _on;
  DiffLine? _at;
  String? _added;

  /// Who it is for, and what is being said to them.
  (String, Draft) get told;

  @override
  void dispose() {
    _remark.dispose();
    _remarkFocus.dispose();
    super.dispose();
  }

  bool isOn(String path) => _on == path;

  /// The line it is about, if it is there still.
  DiffLine? lineOf(String path, List<DiffLine> lines) => _on != path || _at == null
      ? null
      : lines
            .where((l) => l.kind == _at!.kind && l.number == _at!.number && l.text == _at!.text)
            .firstOrNull;

  /// [line]: null, about the whole file.
  void begin(String path, DiffLine? line) {
    setState(() {
      _on = path;
      _at = line;
      _added = null;
    });
    _remarkFocus.requestFocus();
  }

  void _leave() => setState(() {
    _on = null;
    _at = null;
    _remark.clear();
  });

  void _add(String? digest) {
    final words = _remark.text.trim(), path = _on, l = _at;
    // While an input method is composing, Enter is its own.
    final composing = _remark.value.composing;
    if (composing.isValid && !composing.isCollapsed) return;
    if (words.isEmpty || path == null) return;
    final sign = switch (l?.kind) {
      DiffKind.add => '+',
      DiffKind.remove => '-',
      _ => ' ',
    };
    told.$2.add(
      [
        [
          'On $path',
          if (l != null) ', line ${l.number}',
          if (digest != null) ' (as of ${digest.length > 12 ? digest.substring(0, 12) : digest})',
          ':',
        ].join(),
        if (l != null) '    $sign ${l.text}',
        words,
      ].join('\n'),
    );
    _leave();
    setState(() => _added = path);
  }

  Widget editor(BuildContext context, String? digest) {
    final t = Ax.of(context), c = t.colors;
    return Padding(
      padding: const EdgeInsets.fromLTRB(56, 6, 10, 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          CallbackShortcuts(
            bindings: {
              const SingleActivator(LogicalKeyboardKey.enter): () => _add(digest),
              const SingleActivator(LogicalKeyboardKey.escape): _leave,
            },
            child: AxField(
              label: 'About ${_at == null ? 'the file' : 'line ${_at!.number}'}',
              controller: _remark,
              focusNode: _remarkFocus,
              placeholder: 'For ${told.$1}',
            ),
          ),
          const SizedBox(height: 6),
          Wrap(
            spacing: 6,
            runSpacing: 6,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              AxButton(label: 'Add to your message', onPressed: () => _add(digest)),
              AxButton(label: 'Cancel', tone: AxTone.quiet, onPressed: _leave),
              Text('Nothing is sent yet.', style: t.type.small.ink(c.ink3)),
            ],
          ),
        ],
      ),
    );
  }
}

/// What an agent has changed in its own copy of the folder, and what of it is brought in. Nothing is,
/// unless it is pressed here.
class CopyView extends StatefulWidget {
  const CopyView(this.store, this.session, this.agent, {required this.draft, super.key});
  final HostStore store;
  final SessionModel session;
  final AgentModel agent;

  /// What is being written to the agent.
  final Draft draft;
  @override
  State<CopyView> createState() => _CopyViewState();
}

class _CopyViewState extends State<CopyView> with _Remarks<CopyView> {
  @override
  (String, Draft) get told => (widget.agent.name, widget.draft);

  CopyChanges? _all;
  Map<String, FileDiff?> _diffs = const {};
  String? _problem, _said;
  bool _busy = false, _again = false, _leaving = false, _wasAtWork = false;
  final _shut = <String>{};
  final _scroll = ScrollController();

  /// What is said stands first.
  void _up() {
    if (_scroll.hasClients) _scroll.jumpTo(0);
  }

  /// What its person has chosen or not, of what they were shown then.
  final _picked = <(String, String?, bool), bool>{};

  bool get _atWork =>
      widget.agent.phase == AgentPhase.running || widget.agent.phase == AgentPhase.asking;

  @override
  void initState() {
    super.initState();
    widget.agent.addListener(_changed);
    _wasAtWork = _atWork;
    unawaited(_look());
  }

  @override
  void dispose() {
    widget.agent.removeListener(_changed);
    _scroll.dispose();
    super.dispose();
  }

  void _changed() {
    final now = _atWork;
    if (_wasAtWork && !now) unawaited(_look());
    if (_wasAtWork != now) setState(() => _wasAtWork = now);
  }

  Future<void> _look() => _do(() async {});

  /// One thing at a time, and then what there is now.
  Future<void> _do(Future<void> Function() f) async {
    if (_busy) {
      _again = true;
      return;
    }
    setState(() {
      _busy = true;
      _problem = null;
      _leaving = false;
    });
    String? wrong;
    try {
      await f();
    } on Object catch (e) {
      wrong = wordsOf(e);
    }
    // Also after what went wrong: what is shown is what there is.
    try {
      do {
        _again = false;
        final c = await widget.store.copyChanges(widget.session.id, widget.agent.id);
        if (!mounted) return;
        setState(() {
          _all = c;
          _diffs = {
            for (final f in c.files)
              if (f.diff case final d?) f.path: readDiffs(d, most: 600).firstOrNull,
          };
          final now = {for (final f in c.files) (f.path, f.digest, f.run)};
          _picked.removeWhere((k, _) => !now.contains(k));
        });
      } while (_again);
    } on Object catch (e) {
      wrong ??= wordsOf(e);
    } finally {
      if (mounted) {
        setState(() {
          _busy = false;
          _problem = wrong;
        });
        if (wrong != null) _up();
      }
    }
  }

  /// What somebody should look at first is chosen by hand.
  bool _chosen(CopyFile f) => f.can && (_picked[(f.path, f.digest, f.run)] ?? f.plain);

  static String _files(int n) => n == 1 ? '1 file' : '$n files';

  void _tell(Moved m, String done) {
    if (!mounted) return;
    _picked.clear();
    _up();
    setState(() {
      _said = [
        if (m.done.isNotEmpty) '$done ${_files(m.done.length)}.',
        ..._left(m.left),
      ].join('\n');
    });
  }

  static List<String> _left(List<(String, String)> left) => [
    for (final (path, why) in left.take(5)) '${literal(path)}: $why.',
    if (left.length > 5) 'And ${left.length - 5} more.',
  ];

  Future<void> _bring(List<CopyFile> files) => _do(() async {
    _tell(await widget.store.bringIn(widget.session.id, widget.agent.id, files), 'Brought in');
  });

  Future<void> _back(String id) => _do(() async {
    _tell(await widget.store.putBack(widget.session.id, widget.agent.id, id), 'Put back');
  });

  Future<void> _renew() => _do(() async {
    final (taken, say, left) = await widget.store.renewCopy(widget.session.id, widget.agent.id);
    if (say != null) {
      widget.draft.add(say);
    }
    if (!mounted) return;
    setState(() {
      _said = [
        [
          taken == 0 ? 'Nothing was taken over.' : 'Took over ${_files(taken)}.',
          if (say != null)
            'Some have been changed in both. What to tell ${widget.agent.name} is written where you '
                'speak to it; nothing is sent yet.',
        ].join(' '),
        ..._left(left),
      ].join('\n');
    });
  });

  Future<void> _remove(bool lose) => _do(() async {
    await widget.store.removeCopy(widget.session.id, widget.agent.id, lose: lose);
    if (!mounted) return;
    setState(() => _said = 'The copy is gone. Another is made when it is next spoken to.');
  });

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors, all = _all;
    final files = all?.files ?? const <CopyFile>[];
    final chosen = [
      for (final f in files)
        if (_chosen(f)) f,
    ];
    final made = all?.made ?? false;
    final words = all == null
        ? (_problem ?? 'Looking')
        : !made
        ? '${widget.agent.name} works in a copy of the folder. It is made when you first speak to it.'
        : files.isEmpty
        ? (all.ignored + all.odd > 0
              ? 'The folder has all that can be listed.'
              : 'The folder has all of it.')
        : null;
    final only = <String>[
      if (all != null && !all.whole)
        'more than ${_files(files.length)}'
      else if (files.isNotEmpty)
        _files(files.length),
      if ((all?.ignored ?? 0) > 0) '${all!.ignored} that git ignores',
      if ((all?.odd ?? 0) > 0) '${all!.odd} that cannot be listed',
    ];
    final quiet = _busy || _atWork;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(24, 10, 18, 8),
          child: Wrap(
            spacing: 6,
            runSpacing: 4,
            crossAxisAlignment: WrapCrossAlignment.center,
            alignment: WrapAlignment.spaceBetween,
            children: [
              Text(
                files.isEmpty ? 'Its own copy' : '${_files(files.length)} in its own copy',
                style: t.type.label.ink(c.ink3),
              ),
              Wrap(
                spacing: 4,
                runSpacing: 4,
                children: [
                  if (made)
                    AxButton(
                      label: 'Bring it up to date',
                      tone: AxTone.quiet,
                      onPressed: quiet ? null : () => unawaited(_renew()),
                    ),
                  if (made && !_leaving)
                    AxButton(
                      label: 'Throw it away',
                      tone: AxTone.quiet,
                      onPressed: quiet
                          ? null
                          : () {
                              setState(() => _leaving = true);
                              _up();
                            },
                    ),
                  AxButton(
                    label: _busy ? 'Looking' : 'Look again',
                    tone: AxTone.quiet,
                    onPressed: _busy ? null : () => unawaited(_look()),
                  ),
                ],
              ),
            ],
          ),
        ),
        ColoredBox(color: c.bayRule, child: const SizedBox(height: 1)),
        Expanded(child: _list(context, all, files, words, only)),
        if (files.isNotEmpty) ...[
          ColoredBox(color: c.bayRule, child: const SizedBox(height: 1)),
          Padding(
            padding: const EdgeInsets.fromLTRB(24, 10, 18, 10),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    _atWork
                        ? '${widget.agent.name} is at work: this may not be all of it.'
                        : 'The folder gets what is chosen, as it is shown.',
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: t.type.label.ink(c.ink3),
                  ),
                ),
                const SizedBox(width: 12),
                AxButton(
                  label: chosen.isEmpty ? 'Bring in' : 'Bring in ${_files(chosen.length)}',
                  tone: AxTone.primary,
                  onPressed: _busy || chosen.isEmpty ? null : () => unawaited(_bring(chosen)),
                ),
              ],
            ),
          ),
        ],
      ],
    );
  }

  /// All of it goes by when there is little room.
  Widget _list(
    BuildContext context,
    CopyChanges? all,
    List<CopyFile> files,
    String? words,
    List<String> only,
  ) {
    final t = Ax.of(context), c = t.colors;
    final quiet = _busy || _atWork;
    final head = <Widget>[
      if (_leaving)
        Padding(
          padding: const EdgeInsets.fromLTRB(24, 10, 18, 4),
          child: Wrap(
            spacing: 8,
            runSpacing: 6,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              Text(
                [
                  only.isEmpty
                      ? 'The folder has all that was changed in it.'
                      : 'Only it has these, and they would be lost: ${only.join(', ')}.',
                  if (all?.back != null) 'What was brought in last could not be put back any more.',
                ].join(' '),
                style: t.type.ui.ink(only.isEmpty ? c.ink2 : c.attentionText),
              ),
              AxButton(
                label: 'Keep it',
                tone: AxTone.quiet,
                autofocus: true,
                onPressed: () => setState(() => _leaving = false),
              ),
              AxButton(
                label: 'Throw the copy away',
                onPressed: quiet ? null : () => unawaited(_remove(only.isNotEmpty)),
              ),
            ],
          ),
        ),
      if (_problem case final p? when all != null)
        Padding(
          padding: const EdgeInsets.fromLTRB(24, 10, 24, 0),
          child: Text(visible(p), style: t.type.ui.ink(c.faultText)),
        ),
      if (_said != null || all?.back != null)
        Semantics(
          liveRegion: true,
          child: Padding(
            padding: const EdgeInsets.fromLTRB(24, 10, 18, 0),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Expanded(
                  child: Padding(
                    padding: const EdgeInsets.only(top: 4),
                    child: Text(visible(_said ?? ''), style: t.type.ui.ink(c.ink2)),
                  ),
                ),
                if (all?.back case (final id, final n))
                  AxButton(
                    label: 'Put back ${_files(n)}',
                    tone: AxTone.quiet,
                    onPressed: _busy ? null : () => unawaited(_back(id)),
                  ),
              ],
            ),
          ),
        ),
      if (words != null)
        Padding(
          padding: const EdgeInsets.fromLTRB(24, 16, 24, 16),
          child: Text(
            visible(words),
            style: t.type.ui.ink(_problem != null && all == null ? c.faultText : c.ink3),
          ),
        )
      else
        const SizedBox(height: 8),
    ];
    final tail = <Widget>[
      if (words == null && !all!.whole)
        Padding(
          padding: const EdgeInsets.fromLTRB(24, 10, 24, 0),
          child: Text('There is more than can be shown here.', style: t.type.label.ink(c.ink3)),
        ),
      if (all != null && all.ignored + all.odd > 0)
        Padding(
          padding: const EdgeInsets.fromLTRB(24, 0, 24, 8),
          child: Text(
            [
              if (all.ignored > 0)
                'Not listed: ${all.ignored} that git ignores and only the copy has.',
              if (all.odd > 0) '${all.odd} could not be copied or listed.',
            ].join(' '),
            style: t.type.small.ink(c.ink3),
          ),
        ),
      const SizedBox(height: 16),
    ];
    final n = words == null ? files.length : 0;
    return ListView.builder(
      controller: _scroll,
      itemCount: head.length + n + tail.length,
      itemBuilder: (context, i) => i < head.length
          ? head[i]
          : i < head.length + n
          ? Padding(
              padding: const EdgeInsets.only(left: 18, right: 24),
              child: _file(files[i - head.length]),
            )
          : tail[i - head.length - n],
    );
  }

  Widget _file(CopyFile f) {
    final name = oneLine(literal(f.path), 300);
    return _FileRow(
      f,
      _diffs[f.path] ?? FileDiff(f.path),
      remarks: this,
      digest: f.digest,
      open: !_shut.contains(f.path),
      onOpen: () => setState(() {
        _shut.contains(f.path) ? _shut.remove(f.path) : _shut.add(f.path);
      }),
      leading: Padding(
        padding: const EdgeInsets.only(left: 6),
        child: AxCheck(
          label: 'Bring in $name',
          bare: true,
          value: _chosen(f),
          onChanged: !f.can || _busy
              ? null
              : (v) => setState(() => _picked[(f.path, f.digest, f.run)] = v),
        ),
      ),
      notes: [
        if (f.never case final why?) ('Never brought in: $why.', false),
        if (f.clash && f.never == null)
          ('The folder\'s own has changed too. Bring the copy up to date first.', true),
        if (f.unsettled && f.can)
          (
            'You have both changed it, and ${widget.agent.name} has not looked at yours yet. What '
                'bringing it in would undo is shown here.',
            true,
          ),
        if (f.diff == null && f.can) ('It cannot be shown: too large, or not text.', true),
        if (f.runs && f.can) ('It runs when someone builds, tests or installs.', true),
        if (f.others.isNotEmpty)
          ('Also changed by ${f.others.map((o) => oneLine(o, 40)).join(', ')}.', true),
      ],
    );
  }
}
