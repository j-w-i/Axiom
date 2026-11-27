import 'dart:async';
import 'dart:convert';
import 'dart:isolate';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../host/host_store.dart';
import '../host/proto.dart';
import '../patterns/highlight.dart';
import '../patterns/session_panel.dart';
import '../transcript/markdown.dart';
import '../transcript/safe_text.dart';
import '../transcript/transcript_view.dart';
import '../ui/commands.dart';
import '../ui/controls.dart';
import '../ui/editable.dart';
import '../ui/icons.dart';
import '../ui/press.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';
import 'drafts.dart';
import 'start.dart';

/// What its person looks at in a session's folder beside what has changed: one file, the files
/// found by name or by what is in them. All of it in the panel; nothing of it is written (0020).
class FolderLook extends ChangeNotifier {
  FolderLook(this.store, this.session);
  final HostStore store;
  final String session;

  /// Of whose copy, if not of the folder.
  String? agent;

  /// The file that is read, where it was asked to be shown, and what came of it.
  String? path;
  int? line;
  FileText? file;
  String? problem;
  bool loading = false;
  int _asked = 0;

  /// What is shown instead of the list: a file, a search, or the box to find a file in.
  bool get reads => path != null;
  bool searching = false, finding = false;

  Future<void> read(String path, {int? line, String? agent}) async {
    this.path = path;
    this.line = line;
    this.agent = agent;
    file = null;
    problem = null;
    loading = true;
    finding = false;
    final n = ++_asked;
    notifyListeners();
    try {
      final f = await store.readFile(session, path, agent: agent);
      if (n != _asked) return;
      file = f;
    } on Object catch (e) {
      if (n != _asked) return;
      problem = wordsOf(e);
    } finally {
      if (n == _asked) {
        loading = false;
        notifyListeners();
      }
    }
  }

  void close() {
    _asked++;
    path = null;
    file = null;
    problem = null;
    notifyListeners();
  }

  void search(bool on) {
    searching = on;
    finding = false;
    if (on) close();
    notifyListeners();
  }

  void find(bool on) {
    finding = on;
    notifyListeners();
  }

  bool _gone = false;
  @override
  void notifyListeners() {
    if (!_gone) super.notifyListeners();
  }

  @override
  void dispose() {
    _gone = true;
    super.dispose();
  }
}

/// The lines of a text, split where the screen is not drawn.
Future<List<String>> _lines(String text) =>
    text.length < 64 * 1024 ? Future.value(text.split('\n')) : Isolate.run(() => text.split('\n'));

/// Where in [lines] [words] are found, at most [most]: line and column, from 0.
List<(int, int)> findIn(List<String> lines, String words, {int most = 10000}) {
  if (words.isEmpty) return const [];
  final w = words.toLowerCase();
  final out = <(int, int)>[];
  for (var i = 0; i < lines.length && out.length < most; i++) {
    final l = lines[i].toLowerCase();
    var at = l.indexOf(w);
    while (at >= 0 && out.length < most) {
      out.add((i, at));
      at = l.indexOf(w, at + w.length);
    }
  }
  return out;
}

/// The size of a picture, from its first bytes; null if it is not one that is shown.
(int, int)? pictureSize(Uint8List b) {
  int be(int at, int n) {
    var v = 0;
    for (var i = 0; i < n; i++) {
      v = v << 8 | b[at + i];
    }
    return v;
  }

  int le(int at, int n) {
    var v = 0;
    for (var i = n - 1; i >= 0; i--) {
      v = v << 8 | b[at + i];
    }
    return v;
  }

  try {
    if (b.length > 24 && b[0] == 0x89 && b[1] == 0x50) return (be(16, 4), be(20, 4));
    if (b.length > 10 && b[0] == 0x47 && b[1] == 0x49) return (le(6, 2), le(8, 2));
    if (b.length > 26 && b[0] == 0x42 && b[1] == 0x4D) return (le(18, 4), le(22, 4).abs());
    if (b.length > 30 && b[8] == 0x57 && b[9] == 0x45) {
      final kind = String.fromCharCodes(b.sublist(12, 16));
      if (kind == 'VP8X') return (le(24, 3) + 1, le(27, 3) + 1);
      if (kind == 'VP8L') {
        final v = le(21, 4);
        return ((v & 0x3FFF) + 1, (v >> 14 & 0x3FFF) + 1);
      }
      if (kind == 'VP8 ') return (le(26, 2) & 0x3FFF, le(28, 2) & 0x3FFF);
      return null;
    }
    if (b.length > 4 && b[0] == 0xFF && b[1] == 0xD8) {
      var i = 2;
      while (i + 9 < b.length) {
        if (b[i] != 0xFF) return null;
        final m = b[i + 1];
        // A frame's beginning says its size; nothing else of it is read.
        if (m >= 0xC0 && m <= 0xCF && m != 0xC4 && m != 0xC8 && m != 0xCC) {
          return (be(i + 7, 2), be(i + 5, 2));
        }
        i += 2 + be(i + 2, 2);
      }
    }
  } on RangeError {
    return null;
  }
  return null;
}

/// More pixels than this are not decoded.
const mostPixels = 40 * 1000 * 1000, widest = 16384;

/// A picture's bytes, decoded from what the host sent and checked, off the screen's isolate; null
/// with why it is not shown.
Future<(Uint8List?, String?)> picture(String base64) => Isolate.run(() {
  final Uint8List b;
  try {
    b = base64Decode(base64);
  } on FormatException {
    return (null, 'The picture could not be read.');
  }
  final size = pictureSize(b);
  if (size == null) return (null, 'This kind of picture is not shown.');
  final (w, h) = size;
  if (w <= 0 || h <= 0 || w > widest || h > widest || w * h > mostPixels) {
    return (null, 'The picture is too large to show ($w × $h).');
  }
  return (b, null);
});

/// One file, read: its lines with numbers, found words, a line to go to; Markdown as it reads, a
/// table as a table, a picture as a picture. Nothing in it is followed or run.
class FileReader extends StatefulWidget {
  const FileReader(this.look, {required this.draft, super.key});
  final FolderLook look;

  /// Where a mention of it goes.
  final Draft? draft;
  @override
  State<FileReader> createState() => _FileReaderState();
}

enum _As { lines, page }

class _FileReaderState extends State<FileReader> {
  final _find = TextEditingController(), _line = TextEditingController();
  final _findFocus = FocusNode(debugLabel: 'find in file');
  final _selectFocus = FocusNode(debugLabel: 'the file');
  final _scroll = ScrollController();
  List<String>? _all;
  FileText? _of;
  List<(int, int)> _hits = const [];
  int _hit = 0;
  bool _wrap = false;
  _As _as = _As.page;
  List<Block>? _page;
  (Uint8List?, String?)? _picture;
  String? _said;

  FolderLook get look => widget.look;

  @override
  void initState() {
    super.initState();
    look.addListener(_changed);
    _find.addListener(_found);
    _changed();
  }

  @override
  void dispose() {
    look.removeListener(_changed);
    _find.dispose();
    _line.dispose();
    _findFocus.dispose();
    _selectFocus.dispose();
    _scroll.dispose();
    super.dispose();
  }

  bool get _markdown => _of?.path.toLowerCase().endsWith('.md') ?? false;
  bool get _csv => _of?.path.toLowerCase().endsWith('.csv') ?? false;

  void _changed() {
    final f = look.file;
    if (identical(f, _of)) return;
    _of = f;
    _all = null;
    _page = null;
    _picture = null;
    _hits = const [];
    _as = _As.page;
    if (f == null) {
      if (mounted) setState(() {});
      return;
    }
    if (f.text case final text?) {
      unawaited(
        _lines(text).then((l) {
          if (!mounted || !identical(_of, f)) return;
          setState(() => _all = l);
          _found();
          if (look.line case final n?) _go(n);
        }),
      );
      if (_markdown) {
        unawaited(
          Isolate.run(() {
            final d = MarkdownDoc()
              ..append(text)
              ..finish();
            return d.closed;
          }).then((b) {
            if (mounted && identical(_of, f)) setState(() => _page = b);
          }),
        );
      }
    }
    if (f.bytes case final b?) {
      unawaited(
        picture(b).then((p) {
          if (mounted && identical(_of, f)) setState(() => _picture = p);
        }),
      );
    }
    if (mounted) setState(() {});
  }

  void _found() {
    final all = _all;
    if (all == null) return;
    setState(() {
      _hits = findIn(all, _find.text);
      _hit = 0;
    });
    if (_hits.isNotEmpty) _go(_hits.first.$1 + 1, quiet: true);
  }

  double _extent(AxTheme t) {
    final s = t.type.codeSmall;
    return ((s.fontSize ?? 12) * (s.height ?? 1.5)).ceilToDouble();
  }

  /// Shows line [n], from 1.
  void _go(int n, {bool quiet = false}) {
    final all = _all;
    if (all == null) return;
    setState(() => _as = _As.lines);
    final at = (n - 1).clamp(0, all.length - 1);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !_scroll.hasClients) return;
      final y = at * _extent(Ax.of(context)) - 60;
      _scroll.jumpTo(y.clamp(0, _scroll.position.maxScrollExtent));
    });
    if (!quiet) look.line = n;
  }

  void _step(int by) {
    if (_hits.isEmpty) return;
    setState(() => _hit = (_hit + by) % _hits.length);
    _go(_hits[_hit].$1 + 1, quiet: true);
  }

  Future<void> _do(String what, Future<void> Function() f) async {
    setState(() => _said = null);
    try {
      await f();
    } on Object catch (e) {
      if (mounted) setState(() => _said = '$what: ${wordsOf(e)}');
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final f = look.file, path = look.path ?? '';
    final name = oneLine(literal(path), 300);
    final head = Padding(
      padding: const EdgeInsets.fromLTRB(AxSpace.card, 4, 9, 4),
      child: Row(
        children: [
          AxButton.icon(
            AxIcon.close,
            label: 'Close $name',
            tone: AxTone.quiet,
            size: 26,
            onPressed: look.close,
          ),
          const SizedBox(width: 6),
          Expanded(
            child: Semantics(
              header: true,
              child: Text(
                name,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: t.type.smallStrong.ink(c.ink2),
              ),
            ),
          ),
          if (widget.draft case final d?)
            AxButton.icon(
              AxIcon.attach,
              label: 'Mention $name in the message',
              tone: AxTone.quiet,
              size: 26,
              onPressed: () {
                d.add('@$path');
                setState(() => _said = 'Mentioned in your message.');
              },
            ),
          AxButton.icon(
            AxIcon.edit,
            label: 'Open $name in your editor',
            tone: AxTone.quiet,
            size: 26,
            onPressed: () => unawaited(
              _do(
                'It was not opened',
                () => look.store.openFile(look.session, path, agent: look.agent, line: look.line),
              ),
            ),
          ),
          AxButton.icon(
            AxIcon.folder,
            label: Command.isMac ? 'Show $name in Finder' : 'Show $name in its folder',
            tone: AxTone.quiet,
            size: 26,
            onPressed: () => unawaited(
              _do(
                'It was not shown',
                () => look.store.showFile(look.session, path, agent: look.agent),
              ),
            ),
          ),
        ],
      ),
    );
    final words = look.loading
        ? 'Reading'
        : look.problem ??
              (f == null
                  ? null
                  : f.kind == 'other'
                  ? 'Not text or a picture that is shown here: ${_size(f.size)}.'
                  : f.kind == 'image' && f.bytes == null
                  ? 'The picture is too large to send here: ${_size(f.size)}.'
                  : null);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        head,
        if (_said case final s?)
          Semantics(
            liveRegion: true,
            child: Padding(
              padding: const EdgeInsets.fromLTRB(AxSpace.card, 0, AxSpace.card, 6),
              child: Text(visible(s), style: t.type.small.ink(c.ink3)),
            ),
          ),
        if (words != null)
          Padding(
            padding: const EdgeInsets.fromLTRB(AxSpace.card, 6, AxSpace.card, 12),
            child: Text(
              visible(words),
              style: t.type.ui.ink(look.problem != null ? c.faultText : c.ink3),
            ),
          )
        else if (f?.kind == 'image')
          Expanded(child: _pictureOf(t))
        else if (f != null) ...[
          _tools(t, f),
          Expanded(child: _body(t, f)),
        ],
      ],
    );
  }

  Widget _pictureOf(AxTheme t) {
    final p = _picture;
    if (p == null) return const SizedBox.shrink();
    final (bytes, why) = p;
    if (bytes == null) {
      return Padding(
        padding: const EdgeInsets.all(AxSpace.card),
        child: Text(why ?? '', style: t.type.ui.ink(t.colors.ink3)),
      );
    }
    return Padding(
      padding: const EdgeInsets.fromLTRB(10, 0, 10, 10),
      child: InteractiveViewer(
        maxScale: 8,
        child: Image.memory(
          bytes,
          fit: BoxFit.contain,
          gaplessPlayback: true,
          cacheWidth: 2048,
          semanticLabel: 'Picture ${look.path}',
          errorBuilder: (context, _, _) =>
              Text('The picture could not be shown.', style: t.type.ui.ink(t.colors.ink3)),
        ),
      ),
    );
  }

  Widget _tools(AxTheme t, FileText f) {
    final c = t.colors;
    final count = _find.text.isEmpty
        ? null
        : _hits.isEmpty
        ? 'None'
        : '${_hit + 1} of ${_hits.length}${_hits.length >= 10000 ? '+' : ''}';
    return Padding(
      padding: const EdgeInsets.fromLTRB(10, 0, AxSpace.edge, 8),
      child: Wrap(
        spacing: 6,
        runSpacing: 6,
        crossAxisAlignment: WrapCrossAlignment.center,
        children: [
          SizedBox(
            width: 200,
            child: CallbackShortcuts(
              bindings: {
                const SingleActivator(LogicalKeyboardKey.enter): () => _step(1),
                const SingleActivator(LogicalKeyboardKey.enter, shift: true): () => _step(-1),
              },
              child: AxField(
                label: 'Find in the file',
                placeholder: 'Find in the file',
                controller: _find,
                focusNode: _findFocus,
              ),
            ),
          ),
          if (count != null) Text(count, style: t.type.small.ink(c.ink3)),
          SizedBox(
            width: 92,
            child: AxField(
              label: 'Go to line',
              placeholder: 'Line',
              controller: _line,
              onSubmitted: (v) {
                if (int.tryParse(v.trim()) case final n? when n > 0) _go(n);
              },
            ),
          ),
          if (_markdown || _csv)
            AxSegments<_As>(
              label: 'Shown as',
              value: _as,
              items: [
                (_As.page, _markdown ? 'As it reads' : 'Table', null),
                (_As.lines, 'Lines', null),
              ],
              onChanged: (v) => setState(() => _as = v),
            ),
          AxButton(
            label: 'Wrap',
            engaged: _wrap,
            tone: AxTone.quiet,
            onPressed: () => setState(() => _wrap = !_wrap),
          ),
          if (!f.whole)
            Text(
              'Only the first ${_size(2 << 20)} are shown.',
              style: t.type.small.ink(c.attentionText),
            ),
        ],
      ),
    );
  }

  Widget _body(AxTheme t, FileText f) {
    final c = t.colors;
    if (_as == _As.page && _markdown) {
      final blocks = _page;
      if (blocks == null) return const SizedBox.shrink();
      return ListView.builder(
        padding: const EdgeInsets.fromLTRB(AxSpace.card, 0, AxSpace.card, AxSpace.edge),
        itemCount: blocks.length,
        itemBuilder: (context, i) => BlockView(blocks[i], voice: ItemKind.prose),
      );
    }
    if (_as == _As.page && _csv) {
      final source = csvTable(f.text ?? '');
      return SingleChildScrollView(
        padding: const EdgeInsets.fromLTRB(10, 0, 10, AxSpace.edge),
        child: TableBlock(source),
      );
    }
    final all = _all;
    if (all == null) return const SizedBox.shrink();
    final extent = _extent(t);
    final digits = '${all.length}'.length;
    final on = _hits.isEmpty ? -1 : _hits[_hit].$1;
    final style = t.type.codeSmall;
    return ClipPath(
      clipper: const ContinuousClip(AxRadius.block),
      child: ColoredBox(
        color: c.block.fill,
        child: SelectableRegion(
          focusNode: _selectFocus,
          selectionControls: axSelectionControls,
          contextMenuBuilder: (context, s) =>
              AxContextMenu(anchors: s.contextMenuAnchors, items: s.contextMenuButtonItems),
          child: ListView.builder(
            controller: _scroll,
            padding: const EdgeInsets.symmetric(vertical: 6),
            itemExtent: _wrap ? null : extent,
            itemCount: all.length,
            itemBuilder: (context, i) => ColoredBox(
              color: i == on ? c.tinted(c.attention) : const Color(0x00000000),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  SizedBox(
                    width: 14 + digits * 8,
                    child: SelectionContainer.disabled(
                      child: Text('${i + 1}', textAlign: TextAlign.right, style: style.ink(c.ink3)),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Text.rich(
                      highlight(literal(cutTo(all[i], 4000)), style, c),
                      maxLines: _wrap ? null : 1,
                      softWrap: _wrap,
                      overflow: _wrap ? TextOverflow.clip : TextOverflow.fade,
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

String _size(int b) => b < 1024
    ? '$b bytes'
    : b < 1 << 20
    ? '${(b / 1024).toStringAsFixed(0)} KiB'
    : '${(b / (1 << 20)).toStringAsFixed(1)} MiB';

/// A CSV file as the source of a table, at most 500 rows: quoted fields, with commas and quotes in
/// them, are one cell.
String csvTable(String text, {int rows = 500}) {
  final out = StringBuffer();
  var n = 0;
  final cells = <String>[];
  final cell = StringBuffer();
  var quoted = false;
  void row() {
    cells.add(cell.toString());
    cell.clear();
    out.writeln(
      '| ${cells.map((x) => x.replaceAll('|', r'\|').replaceAll('\n', ' ')).join(' | ')} |',
    );
    if (n == 0) out.writeln('|${List.filled(cells.length, '---').join('|')}|');
    cells.clear();
    n++;
  }

  for (var i = 0; i < text.length && n < rows; i++) {
    final ch = text[i];
    if (quoted) {
      if (ch == '"' && i + 1 < text.length && text[i + 1] == '"') {
        cell.write('"');
        i++;
      } else if (ch == '"') {
        quoted = false;
      } else {
        cell.write(ch);
      }
    } else if (ch == '"' && cell.isEmpty) {
      quoted = true;
    } else if (ch == ',') {
      cells.add(cell.toString());
      cell.clear();
    } else if (ch == '\n') {
      row();
    } else if (ch != '\r') {
      cell.write(ch);
    }
  }
  if ((cell.isNotEmpty || cells.isNotEmpty) && n < rows) row();
  return out.toString();
}

/// Finding a file by name: what is typed narrows the folder's files, the best first; Enter reads the
/// first.
class FileFinder extends StatefulWidget {
  const FileFinder(this.look, {super.key});
  final FolderLook look;
  @override
  State<FileFinder> createState() => _FileFinderState();
}

class _FileFinderState extends State<FileFinder> {
  final _words = TextEditingController();
  List<String> _fit = const [];
  String? _problem;
  Timer? _soon;
  int _asked = 0;

  @override
  void initState() {
    super.initState();
    _words.addListener(_typed);
    _typed();
  }

  @override
  void dispose() {
    _soon?.cancel();
    _words.dispose();
    super.dispose();
  }

  void _typed() {
    _soon?.cancel();
    _soon = Timer(const Duration(milliseconds: 120), () async {
      final n = ++_asked;
      final l = widget.look;
      try {
        final fit = await l.store.findFiles(l.session, _words.text, agent: l.agent);
        if (mounted && n == _asked) {
          setState(() {
            _fit = fit;
            _problem = null;
          });
        }
      } on Object catch (e) {
        if (mounted && n == _asked) setState(() => _problem = wordsOf(e));
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors, l = widget.look;
    return PanelGroup(
      'Go to file',
      AxButton.icon(
        AxIcon.close,
        label: 'Close Go to file',
        tone: AxTone.quiet,
        size: 26,
        onPressed: () => l.find(false),
      ),
      [
        Padding(
          padding: const EdgeInsets.fromLTRB(8, 4, 8, 4),
          child: CallbackShortcuts(
            bindings: {const SingleActivator(LogicalKeyboardKey.escape): () => l.find(false)},
            child: AxField(
              label: 'File name',
              placeholder: 'File name',
              controller: _words,
              autofocus: true,
              sunken: true,
              onSubmitted: (_) {
                if (_fit.firstOrNull case final p?) unawaited(l.read(p, agent: l.agent));
              },
            ),
          ),
        ),
        if (_problem case final p?)
          Padding(
            padding: const EdgeInsets.fromLTRB(AxSpace.card, 4, AxSpace.card, 8),
            child: Text(visible(p), style: t.type.small.ink(c.faultText)),
          ),
        for (final p in _fit.take(20))
          PanelRow(
            AxIcon.file,
            oneLine(literal(p), 200),
            onPressed: () => unawaited(l.read(p, agent: l.agent)),
          ),
        if (_fit.isEmpty && _problem == null && _words.text.isNotEmpty)
          Padding(
            padding: const EdgeInsets.fromLTRB(AxSpace.card, 4, AxSpace.card, 8),
            child: Text('No file fits.', style: t.type.small.ink(c.ink3)),
          ),
      ],
    );
  }
}

/// Lines of the folder's files that hold what is looked for; a line pressed is read where it is.
class FileSearch extends StatefulWidget {
  const FileSearch(this.look, {super.key});
  final FolderLook look;
  @override
  State<FileSearch> createState() => _FileSearchState();
}

class _FileSearchState extends State<FileSearch> {
  final _words = TextEditingController();
  bool _case = false, _word = false, _regex = false, _busy = false;
  FoundLines? _found;
  String? _problem;
  int _asked = 0;

  @override
  void dispose() {
    _words.dispose();
    super.dispose();
  }

  Future<void> _go() async {
    final w = _words.text;
    if (w.isEmpty) return;
    final n = ++_asked, l = widget.look;
    setState(() => _busy = true);
    try {
      final f = await l.store.searchFiles(
        l.session,
        w,
        agent: l.agent,
        matchCase: _case,
        word: _word,
        regex: _regex,
      );
      if (mounted && n == _asked) {
        setState(() {
          _found = f;
          _problem = null;
        });
      }
    } on Object catch (e) {
      if (mounted && n == _asked) setState(() => _problem = wordsOf(e));
    } finally {
      if (mounted && n == _asked) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors, l = widget.look, f = _found;
    final rows = <Widget>[];
    String? was;
    for (final h in f?.hits ?? const <Hit>[]) {
      if (h.path != was) {
        rows.add(
          Padding(
            padding: EdgeInsets.fromLTRB(AxSpace.card, was == null ? 4 : 12, AxSpace.card, 2),
            child: Text(oneLine(literal(h.path), 200), style: t.type.smallStrong.ink(c.ink2)),
          ),
        );
        was = h.path;
      }
      rows.add(
        _HitRow(
          h,
          onPressed: () => unawaited(l.read(h.path, line: h.line, agent: l.agent)),
        ),
      );
    }
    void toggle(void Function() f) {
      setState(f);
      unawaited(_go());
    }

    return ListView(
      padding: const EdgeInsets.fromLTRB(8, 0, AxSpace.edge, AxSpace.edge),
      children: [
        PanelHeading(
          'Search in files',
          AxButton.icon(
            AxIcon.close,
            label: 'Close the search',
            tone: AxTone.quiet,
            size: 26,
            onPressed: () => l.search(false),
          ),
          acts: true,
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(4, 0, 0, 8),
          child: Wrap(
            spacing: 6,
            runSpacing: 6,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              SizedBox(
                width: 260,
                child: CallbackShortcuts(
                  bindings: {
                    const SingleActivator(LogicalKeyboardKey.escape): () => l.search(false),
                  },
                  child: AxField(
                    label: 'Search in files',
                    placeholder: 'Search in files',
                    controller: _words,
                    autofocus: true,
                    mono: _regex,
                    onSubmitted: (_) => unawaited(_go()),
                  ),
                ),
              ),
              AxButton(
                label: 'Aa',
                engaged: _case,
                tone: AxTone.quiet,
                onPressed: () => toggle(() => _case = !_case),
              ),
              AxButton(
                label: 'Whole words',
                engaged: _word,
                tone: AxTone.quiet,
                onPressed: () => toggle(() => _word = !_word),
              ),
              AxButton(
                label: '.*',
                engaged: _regex,
                tone: AxTone.quiet,
                onPressed: () => toggle(() => _regex = !_regex),
              ),
            ],
          ),
        ),
        if (_problem case final p?)
          Padding(
            padding: const EdgeInsets.fromLTRB(AxSpace.card, 0, AxSpace.card, 8),
            child: Text(visible(p), style: t.type.ui.ink(c.faultText)),
          ),
        if (f != null)
          Semantics(
            liveRegion: true,
            child: Padding(
              padding: const EdgeInsets.fromLTRB(AxSpace.card, 0, AxSpace.card, 6),
              child: Text(
                [
                  if (f.hits.isEmpty)
                    'Nothing found.'
                  else
                    '${f.hits.length} lines in ${f.files} ${f.files == 1 ? 'file' : 'files'}.',
                  if (f.ended case final e?) 'Ended early: $e.',
                  if (f.unread > 0) '${f.unread} not searched: too large or not text.',
                ].join(' '),
                style: t.type.small.ink(c.ink3),
              ),
            ),
          ),
        if (_busy && f == null) Text('Searching', style: t.type.small.ink(c.ink3)),
        if (rows.isNotEmpty)
          Surface(
            c.plate,
            radius: AxRadius.card,
            child: Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: rows),
            ),
          ),
      ],
    );
  }
}

class _HitRow extends StatelessWidget {
  const _HitRow(this.hit, {required this.onPressed});
  final Hit hit;
  final VoidCallback onPressed;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return PanelPress(
      label: '${hit.path}, line ${hit.line}',
      onPressed: onPressed,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(AxSpace.card, 2, AxSpace.card, 2),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            SizedBox(
              width: 44,
              child: Text(
                '${hit.line}',
                textAlign: TextAlign.right,
                style: t.type.codeSmall.ink(c.ink3),
              ),
            ),
            const SizedBox(width: 10),
            Expanded(
              child: Text.rich(
                highlight(literal(hit.text.trim()), t.type.codeSmall, c),
                maxLines: 1,
                softWrap: false,
                overflow: TextOverflow.fade,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// What agents of this session have left running: its command, how long, what it said last, and a
/// way to stop it. Looked at every two seconds while it is shown, and not otherwise.
class JobsPart extends StatefulWidget {
  const JobsPart(this.store, this.session, {super.key, this.names = const {}});
  final HostStore store;
  final String session;

  /// Agents' names, by id.
  final Map<String, String> names;
  @override
  State<JobsPart> createState() => _JobsPartState();
}

class _JobsPartState extends State<JobsPart> {
  List<JobInfo> _jobs = const [];
  Timer? _every;
  final _open = <(String, int)>{};
  final _stopping = <(String, int)>{};
  String? _problem;
  bool _folded = false;

  @override
  void initState() {
    super.initState();
    unawaited(_look());
    _every = Timer.periodic(const Duration(seconds: 2), (_) => unawaited(_look()));
  }

  @override
  void dispose() {
    _every?.cancel();
    super.dispose();
  }

  Future<void> _look() async {
    try {
      final all = await widget.store.jobs();
      if (!mounted) return;
      setState(
        () => _jobs = [
          for (final j in all)
            if (j.session == widget.session) j,
        ],
      );
    } on Object {
      // Asked again soon.
    }
  }

  Future<void> _stop(JobInfo j) async {
    setState(() {
      _stopping.add((j.agent, j.job));
      _problem = null;
    });
    try {
      await widget.store.stopJob(j.session, j.agent, j.job);
      await _look();
    } on Object catch (e) {
      if (mounted) setState(() => _problem = wordsOf(e));
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_jobs.isEmpty) return const SizedBox.shrink();
    final t = Ax.of(context), c = t.colors;
    final running = _jobs.where((j) => j.ended == null).length;
    return PanelGroup(
      running == 0 ? 'Jobs' : '$running running ${running == 1 ? 'job' : 'jobs'}',
      null,
      folded: _folded,
      onFold: () => setState(() => _folded = !_folded),
      [
        if (_problem case final p?)
          Padding(
            padding: const EdgeInsets.fromLTRB(AxSpace.card, 4, AxSpace.card, 4),
            child: Text(visible(p), style: t.type.small.ink(c.faultText)),
          ),
        for (final j in _jobs) _job(t, j),
      ],
    );
  }

  Widget _job(AxTheme t, JobInfo j) {
    final c = t.colors, key = (j.agent, j.job);
    final open = _open.contains(key);
    final command = oneLine(literal(j.command), 200);
    final who = widget.names[j.agent];
    final state = j.ended ?? 'running for ${_age(j.age)}';
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Expanded(
              child: PanelPress(
                label: '$command, ${who ?? ''}, $state',
                expanded: open,
                onPressed: () => setState(() => open ? _open.remove(key) : _open.add(key)),
                child: Padding(
                  padding: const EdgeInsets.fromLTRB(AxSpace.card, 6, 6, 6),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        command,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: t.type.codeSmall.ink(c.ink1),
                      ),
                      const SizedBox(height: 2),
                      Text(
                        [?who, state].join(' · '),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: t.type.small.ink(j.ended == null ? c.ink3 : c.ink4),
                      ),
                    ],
                  ),
                ),
              ),
            ),
            if (j.ended == null)
              Padding(
                padding: const EdgeInsets.only(right: 8),
                child: AxButton(
                  label: _stopping.contains(key) ? 'Stopping' : 'Stop',
                  onPressed: _stopping.contains(key) ? null : () => unawaited(_stop(j)),
                ),
              ),
          ],
        ),
        if (open)
          Padding(
            padding: const EdgeInsets.fromLTRB(10, 0, 10, 8),
            child: ClipPath(
              clipper: const ContinuousClip(AxRadius.block),
              child: ColoredBox(
                color: c.block.fill,
                child: Padding(
                  padding: const EdgeInsets.all(8),
                  child: Text(
                    visible(j.last.isEmpty ? '(nothing yet)' : _tail(j.last, 20)),
                    style: t.type.codeSmall.ink(c.ink2),
                  ),
                ),
              ),
            ),
          ),
      ],
    );
  }
}

String _tail(String s, int lines) {
  final all = s.trimRight().split('\n');
  return all.length <= lines ? all.join('\n') : all.sublist(all.length - lines).join('\n');
}

String _age(int s) => s < 60
    ? '$s s'
    : s < 3600
    ? '${s ~/ 60} min'
    : '${s ~/ 3600} h ${s % 3600 ~/ 60} min';

/// The folder's own files, folder by folder, opened as they are pressed.
class FileTree extends StatefulWidget {
  const FileTree(this.look, {super.key});
  final FolderLook look;
  @override
  State<FileTree> createState() => _FileTreeState();
}

class _FileTreeState extends State<FileTree> {
  bool _folded = true;
  final _lists = <String, FileList>{};
  final _open = <String>{};
  String? _problem;

  Future<void> _list(String path) async {
    final l = widget.look;
    try {
      final got = await l.store.listFiles(l.session, path, agent: l.agent);
      if (mounted) setState(() => _lists[path] = got);
    } on Object catch (e) {
      if (mounted) setState(() => _problem = wordsOf(e));
    }
  }

  void _toggle(String path) {
    setState(() => _open.contains(path) ? _open.remove(path) : _open.add(path));
    if (_open.contains(path) && !_lists.containsKey(path)) unawaited(_list(path));
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final rows = <Widget>[];
    void add(String path, int depth) {
      final l = _lists[path];
      if (l == null) return;
      for (final e in l.entries) {
        if (rows.length > 2000) return;
        final p = path.isEmpty ? e.name : '$path/${e.name}';
        rows.add(
          _Entry(
            e,
            depth,
            open: _open.contains(p),
            onPressed: () {
              if (e.folder) {
                _toggle(p);
              } else {
                unawaited(widget.look.read(p, agent: widget.look.agent));
              }
            },
          ),
        );
        if (e.folder && _open.contains(p)) add(p, depth + 1);
      }
      if (!l.whole) {
        rows.add(
          Padding(
            padding: EdgeInsets.fromLTRB(AxSpace.card + depth * 14.0, 2, AxSpace.card, 4),
            child: Text('More than can be listed here.', style: t.type.small.ink(c.ink3)),
          ),
        );
      }
    }

    add('', 0);
    return PanelGroup(
      'Files',
      null,
      folded: _folded,
      onFold: () {
        setState(() => _folded = !_folded);
        if (!_folded && !_lists.containsKey('')) unawaited(_list(''));
      },
      [
        if (_problem case final p?)
          Padding(
            padding: const EdgeInsets.fromLTRB(AxSpace.card, 4, AxSpace.card, 4),
            child: Text(visible(p), style: t.type.small.ink(c.faultText)),
          ),
        ...rows,
      ],
    );
  }
}

class _Entry extends StatelessWidget {
  const _Entry(this.entry, this.depth, {required this.open, required this.onPressed});
  final FileEntry entry;
  final int depth;
  final bool open;
  final VoidCallback onPressed;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final name = oneLine(literal(entry.name), 200);
    return PanelPress(
      label: entry.folder ? '$name, folder' : name,
      expanded: entry.folder ? open : null,
      onPressed: onPressed,
      child: Padding(
        padding: EdgeInsets.fromLTRB(AxSpace.card + depth * 14.0, 4, AxSpace.card, 4),
        child: Row(
          children: [
            AxGlyph(
              entry.folder ? (open ? AxIcon.down : AxIcon.right) : AxIcon.file,
              color: c.ink3,
            ),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                name,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: t.type.ui.ink(c.ink2),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// A row of the panel that is pressed: its ground under the hand and the keyboard.
class PanelPress extends StatelessWidget {
  const PanelPress({
    required this.label,
    required this.onPressed,
    required this.child,
    super.key,
    this.expanded,
  });
  final String label;
  final bool? expanded;
  final VoidCallback onPressed;
  final Widget child;
  @override
  Widget build(BuildContext context) {
    final c = Ax.of(context).colors;
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 4),
      child: Pressable(
        label: label,
        expanded: expanded,
        onPressed: onPressed,
        builder: (context, s) => Ground(s.hovered ? c.hover : null, child: child),
      ),
    );
  }
}
