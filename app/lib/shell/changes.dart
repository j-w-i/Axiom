import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../host/host_store.dart';
import '../host/proto.dart';
import '../host/session_model.dart';
import '../patterns/models.dart';
import '../patterns/session_panel.dart';
import '../patterns/transcript_parts.dart';
import '../transcript/safe_text.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/overlay.dart';
import '../ui/prefs.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';
import 'drafts.dart';
import 'start.dart';

/// What has been changed in the session's folder since its last commit. Looked at when it is shown,
/// when a run ends while it is, and when its person asks.
class FolderChanges extends ChangeNotifier {
  FolderChanges(this.store, this.session) {
    session.addListener(_changed);
    _running = _atWork;
  }
  final HostStore store;
  final SessionModel session;

  Changes? all;
  List<(Changed, FileDiff)> files = const [];

  /// The files whose change is read.
  final opened = <String>{};
  bool get reads =>
      !folded && files.any((f) => opened.contains(f.$1.path) && f.$2.lines.isNotEmpty);

  /// Only how much has changed is shown.
  bool folded = false;
  void fold() {
    folded = !folded;
    notifyListeners();
  }

  void toggle(String path) {
    opened.contains(path) ? opened.remove(path) : opened.add(path);
    notifyListeners();
  }

  void closeAll() {
    opened.clear();
    notifyListeners();
  }

  String? problem;
  bool looking = false;
  bool _again = false, _gone = false;
  int _running = 0;

  int get _atWork => session.agents.values
      .where((a) => a.phase == AgentPhase.running || a.phase == AgentPhase.asking)
      .length;

  void _changed() {
    final now = _atWork;
    if (now < _running && hasListeners && all != null) unawaited(look());
    _running = now;
  }

  Future<void> lookOnce() => all == null && !looking ? look() : Future.value();

  Future<void> look() async {
    if (looking) {
      _again = true;
      return;
    }
    looking = true;
    _tell();
    try {
      do {
        _again = false;
        final c = await store.changes(session.id);
        all = c;
        final read = {for (final f in readDiffs(c.diff, most: 600)) f.path: f};
        files = [for (final f in c.files) (f, read[f.path] ?? FileDiff(f.path))];
        problem = null;
      } while (_again);
    } on Object catch (e) {
      problem = wordsOf(e);
    } finally {
      looking = false;
      _tell();
    }
  }

  // Not while it is built that somebody begins to look.
  void _tell() => scheduleMicrotask(() {
    if (!_gone) notifyListeners();
  });

  @override
  void dispose() {
    _gone = true;
    session.removeListener(_changed);
    super.dispose();
  }
}

class ChangesView extends StatefulWidget {
  const ChangesView(
    this.changes, {
    required this.agent,
    required this.draft,
    super.key,
    this.lead = const [],
    this.tail = const [],
    this.onRead,
  });

  /// What stands over it in the panel, and under it.
  final List<Widget> lead, tail;

  /// A file is to be read, by its path in the folder.
  final ValueChanged<String>? onRead;
  final FolderChanges changes;

  /// Who is told of what is said about a line, and where that is written.
  final String agent;
  final Draft draft;
  @override
  State<ChangesView> createState() => _ChangesViewState();
}

class _ChangesViewState extends State<ChangesView> with _Remarks<ChangesView> {
  @override
  (String, Draft) get told => (widget.agent, widget.draft);

  @override
  void initState() {
    super.initState();
    unawaited(widget.changes.lookOnce());
  }

  @override
  void didUpdateWidget(ChangesView old) {
    super.didUpdateWidget(old);
    // The session was made again under its name: what it changed is looked at again.
    if (!identical(old.changes, widget.changes)) unawaited(widget.changes.lookOnce());
  }

  @override
  Widget build(BuildContext context) =>
      ListenableBuilder(listenable: widget.changes, builder: (context, _) => _build(context));

  Widget _build(BuildContext context) {
    final now = widget.changes;
    final all = now.all, problem = now.problem, files = now.files, looking = now.looking;
    final t = Ax.of(context), c = t.colors;
    final words = all == null
        ? (problem ?? 'Loading')
        : !all.repository
        ? 'The folder is not a git repository, so what has changed in it is not known.'
        : files.isEmpty
        ? 'Nothing has changed since the last commit.'
        : null;
    final n = words == null ? files.length : 0, shown = now.folded ? 0 : n;
    final head = <Widget>[
      ...widget.lead,
      PanelHeading(
        switch (n) {
          0 => 'Changed files',
          1 => '1 changed file',
          _ => '$n changed files',
        },
        Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (all?.branch case final b?) ...[
              AxGlyph(AxIcon.branch, color: c.ink4),
              const SizedBox(width: 4),
              ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 110),
                child: Text(
                  oneLine(literal(b), 80),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: t.type.small.ink(c.ink3),
                ),
              ),
              const SizedBox(width: 10),
            ],
            if (n > 0)
              ChangedBy(
                files.fold(0, (n, f) => n + f.$2.added),
                files.fold(0, (n, f) => n + f.$2.removed),
              ),
            const SizedBox(width: 2),
            if (n > 0) DiffMenu(onClose: now.opened.isEmpty ? null : now.closeAll),
            AxButton.icon(
              AxIcon.refresh,
              label: 'Refresh',
              tone: AxTone.quiet,
              size: 26,
              onPressed: looking ? null : () => unawaited(now.look()),
            ),
          ],
        ),
        acts: true,
        folded: now.folded,
        onFold: now.fold,
      ),
      if (problem != null && all != null)
        Padding(
          padding: const EdgeInsets.fromLTRB(AxSpace.card, 0, AxSpace.card, 8),
          child: Text(visible(problem), style: t.type.ui.ink(c.faultText)),
        ),
      if (words != null && !now.folded)
        Padding(
          padding: const EdgeInsets.fromLTRB(AxSpace.card, 2, AxSpace.card, 12),
          child: Text(
            visible(words),
            style: t.type.ui.ink(problem != null && all == null ? c.faultText : c.ink3),
          ),
        ),
    ];
    final more = words == null && !all!.whole && !now.folded ? 1 : 0;
    final tail = [if (widget.tail.isNotEmpty) const SizedBox(height: 10), ...widget.tail];
    return ListView.builder(
      padding: const EdgeInsets.fromLTRB(8, 0, AxSpace.edge, AxSpace.edge),
      itemCount: head.length + shown + more + tail.length,
      itemBuilder: (context, i) => i < head.length
          ? head[i]
          : i < head.length + shown
          ? _file(files[i - head.length], i - head.length, n)
          : i < head.length + shown + more
          ? Padding(
              padding: const EdgeInsets.fromLTRB(AxSpace.card, 10, AxSpace.card, 0),
              child: Text('There is more than can be shown here.', style: t.type.label.ink(c.ink3)),
            )
          : tail[i - head.length - shown - more],
    );
  }

  Widget _file((Changed, FileDiff) f, int i, int n) => CardPart(
    first: i == 0,
    last: i == n - 1,
    child: _FileRow(
      f.$1,
      f.$2,
      remarks: this,
      last: i == n - 1,
      open: widget.changes.opened.contains(f.$1.path),
      onOpen: () => widget.changes.toggle(f.$1.path),
      onRead: f.$1.what == 'removed' || widget.onRead == null
          ? null
          : () => widget.onRead!(f.$1.path),
    ),
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
    this.tag,
    this.shut = false,
    this.last = false,
    this.onRead,
  });
  final bool last;

  /// The file is read, whole, in the panel.
  final VoidCallback? onRead;

  /// What it is, in a word, where that is more than how it has changed.
  final String? tag;

  /// Nothing can be done with it.
  final bool shut;
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
    final shape = diffShape(context);
    final lines = diff.lines, add = diff.added, remove = diff.removed, left = diff.left;
    final open = this.open && lines.isNotEmpty;
    final name = oneLine(literal(file.path), 300);
    final row = ChangedRow(
      name,
      added: add,
      removed: remove,
      what: file.what,
      open: open,
      tag: tag ?? (file.what == 'changed' ? null : _upper(file.what)),
      quiet: shut,
      onPressed: lines.isEmpty ? null : onOpen,
    );
    final in_ = leading == null ? 39.0 : 66.0;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            const SizedBox(width: 8),
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
            if (onRead case final read?)
              AxButton.icon(
                AxIcon.file,
                label: 'Read $name',
                tone: AxTone.quiet,
                size: 28,
                onPressed: read,
              ),
            if (open)
              AxButton.icon(
                AxIcon.comment,
                label: 'Comment on $name',
                tone: AxTone.quiet,
                size: 28,
                onPressed: () => remarks.begin(file.path, null),
              ),
            const SizedBox(width: 8),
          ],
        ),
        for (final (note, look) in notes)
          Padding(
            padding: EdgeInsets.fromLTRB(in_, 0, AxSpace.card, 8),
            child: look
                ? Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Padding(
                        padding: const EdgeInsets.only(top: 2),
                        child: AxGlyph(AxIcon.warn, color: c.attention),
                      ),
                      const SizedBox(width: 8),
                      Expanded(child: Text(visible(note), style: t.type.label.ink(c.ink2))),
                    ],
                  )
                : Text(visible(note), style: t.type.label.ink(c.ink3)),
          ),
        if (open)
          Padding(
            padding: const EdgeInsets.fromLTRB(10, 0, 10, 10),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                DiffLines(
                  lines,
                  shape: shape,
                  onLine: (l) => remarks.begin(file.path, l),
                  at: remarks.lineOf(file.path, shape.space ? lines : withoutSpace(lines)),
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
        if (!last)
          const Padding(
            padding: EdgeInsets.symmetric(horizontal: AxSpace.card),
            child: Hairline(),
          ),
      ],
    );
  }
}

String _upper(String s) => s.isEmpty ? s : s[0].toUpperCase() + s.substring(1);

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
  DiffLine? lineOf(String path, List<DiffLine> lines) =>
      _on != path || _at == null ? null : lines.where((l) => l == _at).firstOrNull;

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
      padding: const EdgeInsets.fromLTRB(12, 6, 12, 8),
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
              placeholder: 'Comment for ${told.$1}',
            ),
          ),
          const SizedBox(height: 6),
          Wrap(
            spacing: 6,
            runSpacing: 6,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              AxButton(
                label: 'Add to message',
                tone: AxTone.primary,
                onPressed: () => _add(digest),
              ),
              AxButton(label: 'Cancel', onPressed: _leave),
              Text('Not sent yet.', style: t.type.small.ink(c.ink3)),
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
  const CopyView(
    this.store,
    this.session,
    this.agent, {
    required this.draft,
    super.key,
    this.lead = const [],
    this.tail = const [],
    this.onRead,
  });
  final List<Widget> lead, tail;

  /// A file of the copy is to be read, by its path in it.
  final ValueChanged<String>? onRead;
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
  void didUpdateWidget(CopyView old) {
    super.didUpdateWidget(old);
    if (identical(old.agent, widget.agent)) return;
    // The session was made again under its name: the agent heard from is the new one.
    old.agent.removeListener(_changed);
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
    _tell(await widget.store.bringIn(widget.session.id, widget.agent.id, files), 'Applied');
  });

  Future<void> _back(String id) => _do(() async {
    _tell(await widget.store.putBack(widget.session.id, widget.agent.id, id), 'Reverted');
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
          taken == 0 ? 'Already up to date.' : 'Updated ${_files(taken)} from the folder.',
          if (say != null)
            'Some were changed in both. A message to ${widget.agent.name} about them is written; '
                'it is not sent yet.',
        ].join(' '),
        ..._left(left),
      ].join('\n');
    });
  });

  Future<void> _remove(bool lose) => _do(() async {
    await widget.store.removeCopy(widget.session.id, widget.agent.id, lose: lose);
    if (!mounted) return;
    setState(() => _said = 'The copy was discarded. A new one is made with the next message.');
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
        ? (_problem ?? 'Loading')
        : !made
        ? '${widget.agent.name} works in a separate copy of the folder. It is made with the first message.'
        : files.isEmpty
        ? (all.ignored + all.odd > 0
              ? 'No differences that can be listed.'
              : 'No differences from the folder.')
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
        Expanded(child: _list(context, all, files, words, only)),
        if (files.isNotEmpty && _atWork)
          Padding(
            padding: const EdgeInsets.fromLTRB(22, 8, 30, 0),
            child: Text(
              '${widget.agent.name} is still working: there may be more changes.',
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: t.type.label.ink(c.ink3),
            ),
          ),
        // What is done with them stands where the reading ends.
        if (made)
          Padding(
            padding: const EdgeInsets.fromLTRB(8, 10, AxSpace.edge, AxSpace.edge),
            child: Wrap(
              spacing: 8,
              runSpacing: 8,
              alignment: WrapAlignment.spaceBetween,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    if (!_leaving)
                      AxButton(
                        label: 'Discard copy…',
                        onPressed: quiet
                            ? null
                            : () {
                                setState(() => _leaving = true);
                                _up();
                              },
                      ),
                    AxButton(
                      label: 'Update from folder',
                      onPressed: quiet ? null : () => unawaited(_renew()),
                    ),
                  ],
                ),
                if (files.isNotEmpty)
                  AxButton(
                    label: chosen.isEmpty ? 'Apply' : 'Apply ${_files(chosen.length)}',
                    tone: AxTone.primary,
                    onPressed: _busy || chosen.isEmpty ? null : () => unawaited(_bring(chosen)),
                  ),
              ],
            ),
          ),
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
      ...widget.lead,
      PanelHeading(
        files.isEmpty
            ? '${widget.agent.name}\'s copy'
            : '${_files(files.length)} in ${widget.agent.name}\'s copy',
        Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (files.isNotEmpty)
              DiffMenu(
                onClose: files.every((f) => _shut.contains(f.path))
                    ? null
                    : () => setState(() => _shut.addAll(files.map((f) => f.path))),
              ),
            AxButton.icon(
              AxIcon.refresh,
              label: 'Refresh',
              tone: AxTone.quiet,
              size: 26,
              onPressed: _busy ? null : () => unawaited(_look()),
            ),
          ],
        ),
        acts: true,
      ),
      if (_leaving)
        Padding(
          padding: const EdgeInsets.fromLTRB(AxSpace.card, 0, AxSpace.card, 10),
          child: Wrap(
            spacing: 8,
            runSpacing: 6,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              Row(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  if (only.isNotEmpty) ...[
                    Padding(
                      padding: const EdgeInsets.only(top: 3),
                      child: AxGlyph(AxIcon.warn, color: c.attention),
                    ),
                    const SizedBox(width: 6),
                  ],
                  Flexible(
                    child: Text(
                      [
                        only.isEmpty
                            ? 'The folder already has all of its changes.'
                            : 'These are only in the copy and would be lost: ${only.join(', ')}.',
                        if (all?.back != null) 'The last apply could no longer be undone.',
                      ].join(' '),
                      style: t.type.ui.ink(only.isEmpty ? c.ink2 : c.ink1),
                    ),
                  ),
                ],
              ),
              AxButton(
                label: 'Keep',
                autofocus: true,
                onPressed: () => setState(() => _leaving = false),
              ),
              AxButton(
                label: 'Discard copy',
                onPressed: quiet ? null : () => unawaited(_remove(only.isNotEmpty)),
              ),
            ],
          ),
        ),
      if (_problem case final p? when all != null)
        Padding(
          padding: const EdgeInsets.fromLTRB(AxSpace.card, 0, AxSpace.card, 8),
          child: Text(visible(p), style: t.type.ui.ink(c.faultText)),
        ),
      if (_said != null || all?.back != null)
        Semantics(
          liveRegion: true,
          child: Padding(
            padding: const EdgeInsets.fromLTRB(AxSpace.card, 0, AxSpace.card, 10),
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
                    label: 'Undo apply (${_files(n)})',
                    onPressed: _busy ? null : () => unawaited(_back(id)),
                  ),
              ],
            ),
          ),
        ),
      if (words != null)
        Padding(
          padding: const EdgeInsets.fromLTRB(AxSpace.card, 2, AxSpace.card, 12),
          child: Text(
            visible(words),
            style: t.type.ui.ink(_problem != null && all == null ? c.faultText : c.ink3),
          ),
        ),
    ];
    final tail = <Widget>[
      if (words == null && !all!.whole)
        Padding(
          padding: const EdgeInsets.fromLTRB(AxSpace.card, 10, AxSpace.card, 0),
          child: Text('There is more than can be shown here.', style: t.type.label.ink(c.ink3)),
        ),
      if (all != null && all.ignored + all.odd > 0)
        Padding(
          padding: const EdgeInsets.fromLTRB(AxSpace.card, 10, AxSpace.card, 8),
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
      ...widget.tail,
    ];
    final n = words == null ? files.length : 0;
    return ListView.builder(
      controller: _scroll,
      padding: const EdgeInsets.fromLTRB(8, 0, AxSpace.edge, 4),
      itemCount: head.length + n + tail.length,
      itemBuilder: (context, i) => i < head.length
          ? head[i]
          : i < head.length + n
          ? CardPart(
              first: i == head.length,
              last: i == head.length + n - 1,
              child: _file(files[i - head.length], last: i == head.length + n - 1),
            )
          : tail[i - head.length - n],
    );
  }

  Widget _file(CopyFile f, {required bool last}) {
    final name = oneLine(literal(f.path), 300);
    return _FileRow(
      f,
      _diffs[f.path] ?? FileDiff(f.path),
      remarks: this,
      digest: f.digest,
      last: last,
      open: !_shut.contains(f.path),
      onOpen: () => setState(() {
        _shut.contains(f.path) ? _shut.remove(f.path) : _shut.add(f.path);
      }),
      onRead: f.digest == null || widget.onRead == null ? null : () => widget.onRead!(f.path),
      leading: Padding(
        padding: const EdgeInsets.only(left: 6, right: 4),
        child: AxCheck(
          label: 'Apply $name',
          bare: true,
          value: _chosen(f),
          onChanged: !f.can || _busy
              ? null
              : (v) => setState(() => _picked[(f.path, f.digest, f.run)] = v),
        ),
      ),
      shut: !f.can,
      tag: f.never != null
          ? 'Blocked'
          : f.clash
          ? 'Conflict'
          : null,
      notes: [
        if (f.never case final why?) ('Never applied: $why.', false),
        if (f.clash && f.never == null)
          ('Also changed in your folder. Update from folder first.', true),
        if (f.unsettled && f.can)
          (
            'You both changed it, and ${widget.agent.name} has not seen yours. What applying it '
                'would undo is shown here.',
            true,
          ),
        if (f.diff == null && f.can) ('No preview: too large, or not text.', true),
        if (f.runs && f.can) ('Runs when someone builds, tests or installs.', true),
        if (f.others.isNotEmpty)
          ('Also changed by ${f.others.map((o) => oneLine(o, 40)).join(', ')}.', true),
      ],
    );
  }
}

/// How changes are shown, as their person chose.
DiffShape diffShape(BuildContext context) {
  final p = PrefsScope.of(context);
  return DiffShape(side: p.diffSide, space: p.diffSpace, wrap: p.diffWrap);
}

enum _Shape { side, space, wrap, close }

/// How changes are looked at: two sides, white space, long lines; and every file closed.
class DiffMenu extends StatelessWidget {
  const DiffMenu({super.key, this.onClose});

  /// Closes every file that is open; null if none is.
  final VoidCallback? onClose;

  @override
  Widget build(BuildContext context) {
    final prefs = PrefsScope.maybeOf(context);
    final p = PrefsScope.of(context);
    AxIcon? on(bool b) => b ? AxIcon.check : null;
    return AxPopover(
      width: 260,
      alignEnd: true,
      anchor: (context, open, toggle) => AxButton.icon(
        AxIcon.more,
        label: 'How changes are shown',
        tone: AxTone.quiet,
        size: 26,
        onPressed: toggle,
      ),
      panel: (context, close) => AxMenu<_Shape>(
        label: 'How changes are shown',
        items: [
          AxMenuItem(
            _Shape.side,
            'Side by side',
            icon: on(p.diffSide),
            detail: 'Where the panel is wide enough',
          ),
          AxMenuItem(_Shape.space, 'Ignore white space', icon: on(!p.diffSpace)),
          AxMenuItem(_Shape.wrap, 'Wrap long lines', icon: on(p.diffWrap)),
          AxMenuItem(_Shape.close, 'Close all files', enabled: onClose != null),
        ],
        onChosen: (v) {
          close();
          if (prefs == null) return;
          final now = prefs.value;
          switch (v) {
            case _Shape.side:
              prefs.value = now.copyWith(diffSide: !now.diffSide);
            case _Shape.space:
              prefs.value = now.copyWith(diffSpace: !now.diffSpace);
            case _Shape.wrap:
              prefs.value = now.copyWith(diffWrap: !now.diffWrap);
            case _Shape.close:
              onClose?.call();
          }
        },
      ),
    );
  }
}
