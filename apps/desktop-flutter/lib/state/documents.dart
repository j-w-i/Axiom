// Open documents (FL-EDIT-001): a port of the Tauri shell's DocumentsStore, with the same rules,
// minus the editor bridge (the text lives in each document's controller here).
// - Saving sends `expectedVersion`; a `conflict` puts the save in the conflict queue (Overwrite,
//   Reload, Cancel).
// - An external change: the version decides. A clean document reloads; a dirty one is marked
//   "changed on disk".
// - Closing a dirty tab, switching or closing the folder and quitting ask first (Save, Don't
//   Save, Cancel), one question at a time.
// - Tabs, the active tab and the right group are saved per workspace (`editor.*` keys) and
//   restored when the workspace reopens.

import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:flutter/scheduler.dart';
import 'package:re_editor/re_editor.dart';

import '../daemon/client.dart';
import '../daemon/protocol.g.dart';
import '../editor/languages.dart';
import '../editor/lsp/semantic_tokens.dart';
import '../editor/lsp/underline.dart';
import '../ui/tokens.dart';
import 'lsp.dart';
import 'recovery.dart';
import 'workspace.dart';

const openPathsKey = 'editor.openPaths';
const activePathKey = 'editor.activePath';
const splitPathsKey = 'editor.splitPaths';
const splitActiveKey = 'editor.splitActive';
const _tabSaveDebounce = Duration(milliseconds: 500);

String titleOf(String path) => isExternal(path)
    ? (Uri.tryParse(path)?.pathSegments.where((s) => s.isNotEmpty).lastOrNull ?? path)
    : path.split('/').last;

/// A document outside the workspace: its path is the URI a language server gave. A `file://` one
/// (an SDK's or a dependency's source) the daemon reads, read-only, because the server pointed to
/// it (protocol 0.4, ADR-0018 §2); another scheme's text comes from the server itself (jdtls's
/// `jdt://` class files). It is never saved.
bool isExternal(String path) => RegExp(r'^[a-z][a-z0-9+.-]+:/').hasMatch(path);

/// The name a document's language is detected from: a class file's source is Java, an
/// assembly's (csharp-ls's `csharp:/metadata/…`) C#.
String _languageName(String path) => path.startsWith('jdt://')
    ? '${titleOf(path).replaceAll(RegExp(r'\.class$'), '')}.java'
    : path.startsWith('csharp:')
    ? '${titleOf(path).replaceAll(RegExp(r'\.cs$'), '')}.cs'
    : path;

/// Languages whose calls and members get their colours from the structural pass when the grammar
/// leaves them plain ([applyStructure]), by highlight.js name: code, not data or prose.
const structuredLanguages = {
  'arduino',
  'bash',
  'c',
  'cpp',
  'crystal',
  'csharp',
  'd',
  'dart',
  'elixir',
  'erlang',
  'fsharp',
  'go',
  'groovy',
  'haskell',
  'java',
  'javascript',
  'julia',
  'kotlin',
  'lua',
  'nim',
  'objectivec',
  'ocaml',
  'perl',
  'php',
  'powershell',
  'python',
  'r',
  'ruby',
  'rust',
  'scala',
  'sql',
  'swift',
  'typescript',
  'vbnet',
};

/// The file's line break: the first one in it (CRLF, CR or LF), LF when it has none. The
/// editor joins lines with it, so a CRLF file is the same bytes until edited and is saved with
/// CRLF (requirement E-3; audit finding 2).
TextLineBreak lineBreakOf(String text) {
  final i = text.indexOf(RegExp('[\r\n]'));
  if (i < 0 || text[i] == '\n') return TextLineBreak.lf;
  return i + 1 < text.length && text[i + 1] == '\n' ? TextLineBreak.crlf : TextLineBreak.cr;
}

/// The length of [lines] joined with a [breakLength]-character line break, without joining
/// them; null when a line has folded lines inside (they are counted by joining).
int? textLength(CodeLines lines, int breakLength) {
  final n = lines.length;
  var length = n > 0 ? (n - 1) * breakLength : 0;
  // One pass over the segments: `lines[i]` walks them from the start each time (AUD-005).
  for (final line in lines.lines) {
    if (line.chunks.isNotEmpty) return null;
    length += line.text.length;
  }
  return length;
}

/// Each line's own break when a file mixes them (CRLF and LF, or a stray CR); null when every
/// break is the same. The editor joins lines with one break, so a mixed file would look edited as
/// soon as it was read, and saving would rewrite its breaks (AUD-023).
List<String>? mixedBreaks(String text) {
  final breaks = <String>[];
  for (var i = 0; i < text.length; i++) {
    final ch = text.codeUnitAt(i);
    if (ch == 0x0D) {
      if (i + 1 < text.length && text.codeUnitAt(i + 1) == 0x0A) {
        breaks.add('\r\n');
        i++;
      } else {
        breaks.add('\r');
      }
    } else if (ch == 0x0A) {
      breaks.add('\n');
    }
  }
  if (breaks.isEmpty) return null;
  final first = breaks.first;
  return breaks.every((b) => b == first) ? null : breaks;
}

/// [text] with every line break replaced by [lineBreak] (what the editor shows and joins with).
String _normalised(String text, String lineBreak) =>
    text.replaceAll(RegExp('\r\n|\r|\n'), lineBreak);

String _firstLine(String text) {
  final i = text.indexOf('\n');
  return i < 0 ? text : text.substring(0, i);
}

class Doc {
  Doc({required this.path, required String text, required this.savedVersion})
    : _savedText = text,
      _breaks = mixedBreaks(text),
      languageNotifier = ValueNotifier(
        Languages.instance.detect(_languageName(path), firstLine: _firstLine(text)),
      ) {
    controller = CodeLineEditingController(
      codeLines: CodeLines.fromText(text),
      options: CodeLineOptions(lineBreak: lineBreakOf(text)),
      spanBuilder: _span,
    );
    _shiftLines = controller.codeLines;
    controller.addListener(_followLines);
    if (_breaks != null) _savedText = _normalised(text, controller.options.lineBreak.value);
  }

  /// The file's own line breaks, one per line, when it mixes them (see [mixedBreaks]).
  List<String>? _breaks;

  /// [text] (as the editor joins it) as it is written to disk: a file with mixed line breaks keeps
  /// each line's own break (AUD-023). The lines are matched with the saved ones from the start
  /// and from the end, so the lines around an edit keep theirs when lines are added or removed;
  /// a line changed in place keeps the break at its place, and a line added takes the break of
  /// the line above the edit (review B6a).
  String diskText(String text) {
    final breaks = _breaks;
    if (breaks == null) return text;
    final lb = controller.options.lineBreak.value;
    final lines = text.split(lb);
    final saved = _savedText.split(lb);
    if (saved.length != breaks.length + 1) return text; // not the text the breaks belong to
    final most = lines.length < saved.length ? lines.length : saved.length;
    var head = 0;
    while (head < most && lines[head] == saved[head]) {
      head++;
    }
    var tail = 0;
    while (tail < most - head && lines[lines.length - 1 - tail] == saved[saved.length - 1 - tail]) {
      tail++;
    }
    final shift = lines.length - saved.length;
    // The saved line j's own break (the last saved line has none).
    String? at(int j) => j >= 0 && j < breaks.length ? breaks[j] : null;
    final out = StringBuffer();
    String? above;
    for (var i = 0; i < lines.length; i++) {
      out.write(lines[i]);
      if (i == lines.length - 1) break;
      final b =
          (i < head ? at(i) : null) ?? // unchanged, above the edit
          (i >= lines.length - tail ? at(i - shift) : null) ?? // unchanged, below it
          (i < saved.length - tail ? at(i) : null) ?? // changed in place
          above ?? // added: the break of the line above
          breaks.first;
      out.write(b);
      above = b;
    }
    return out.toString();
  }

  /// [disk] (a file's bytes as text) as the editor would join it.
  String editorText(String disk) {
    _breaks = mixedBreaks(disk);
    return _breaks == null ? disk : _normalised(disk, controller.options.lineBreak.value);
  }

  /// The lines the server's decorations were last placed for.
  CodeLines? _shiftLines;

  /// Lines inserted or removed: the semantic tokens below the edit move with their lines, and the
  /// diagnostics and highlights below it, which carry their own positions, go until the server
  /// sends new ones, rather than showing on a neighbour's line (audit finding 16).
  void _followLines() {
    final now = controller.codeLines;
    final before = _shiftLines;
    if (identical(now, before)) return;
    _shiftLines = now;
    if (before == null || before.length == now.length) return;
    // The first changed line, in one pass over both (indexing walks the segments each time).
    var at = 0;
    final a = before.lines.iterator, b = now.lines.iterator;
    while (a.moveNext() && b.moveNext() && a.current.text == b.current.text) {
      at++;
    }
    final delta = now.length - before.length;
    if (semantic.isNotEmpty) {
      semantic = {
        for (final e in semantic.entries)
          if (e.key < at)
            e.key: e.value
          else if (e.key + delta > at || (delta > 0 && e.key + delta >= at))
            e.key + delta: e.value,
      };
    }
    if (diagnostics.isNotEmpty) {
      _diagnosticsByLine = {
        for (final e in _diagnosticsByLine.entries)
          if (e.key < at) e.key: e.value,
      };
    }
    if (highlights.isNotEmpty) highlights = const [];
  }

  /// Workspace-relative; changes when the file is renamed or moved in Axiom (AUD-022).
  String path;

  /// The document's language: detected from its name and first line; the status bar's picker
  /// changes it for the session.
  final ValueNotifier<Language> languageNotifier;
  Language get language => languageNotifier.value;
  late final CodeLineEditingController controller;

  /// The language servers' diagnostics on this document (set by the LSP layer).
  List<LspDiagnostic> diagnostics = const [];

  /// Underline colours by LSP severity (set by the editor from the theme).
  Color Function(int severity) underlineColor = (_) => AxiomColors.dark.danger;

  /// The code colours (set by the editor from the theme); the server's tokens follow a change.
  SyntaxColors get syntax => _syntax;
  SyntaxColors _syntax = AxiomColors.dark.syntax;
  set syntax(SyntaxColors y) {
    if (identical(y, _syntax)) return;
    semantic = rethemeSemantic(semantic, _syntax, y);
    _syntax = y;
  }

  TextSpan _span({
    required BuildContext context,
    required int index,
    required CodeLine codeLine,
    required TextSpan textSpan,
    required TextStyle style,
  }) {
    var span = textSpan;
    if (structuredLanguages.contains(language.hljs)) {
      span = applyStructure(span, syntax.text, syntax.function, syntax.variable);
    }
    final sem = semantic[index];
    if (sem != null && sem.isNotEmpty) span = applyStyles(span, sem, codeLine.text.length);
    if (highlights.isNotEmpty) {
      span = applyHighlights(
        span,
        highlightsOn(index, codeLine.text.length, highlights),
        highlightColor,
      );
    }
    final onLine = _diagnosticsByLine[index];
    if (onLine == null) return span;
    return applyUnderlines(span, underlinesOn(index, codeLine.text.length, onLine, underlineColor));
  }

  /// The server's semantic tokens by line, over the grammar's colours (ADR-0021 §2).
  Map<int, List<SemanticRange>> semantic = const {};

  void setSemantic(Map<int, List<SemanticRange>> tokens) {
    if (tokens.isEmpty && semantic.isEmpty) return;
    semantic = tokens;
    controller.forceRepaint();
    _secondary?.forceRepaint();
  }

  /// The symbol under the caret, everywhere it is in this document (`documentHighlight`).
  List<LspRange> highlights = const [];
  Color highlightColor = AxiomColors.dark.accentSoft;

  void setHighlights(List<LspRange> list) {
    if (list.isEmpty && highlights.isEmpty) return;
    highlights = list;
    controller.forceRepaint();
    _secondary?.forceRepaint();
  }

  /// New diagnostics: redraw the lines.
  void setDiagnostics(List<LspDiagnostic> list) {
    diagnostics = list;
    // Indexed by line once, so building a line looks at its own diagnostics only, not every
    // diagnostic in the file (audit finding 12). A range over many lines is indexed on its
    // first and last 200 lines.
    final byLine = <int, List<LspDiagnostic>>{};
    for (final d in list) {
      final first = d.range.start.line, last = d.range.end.line;
      for (var i = first; i <= last; i++) {
        if (i - first >= 200 && last - i >= 200) {
          i = last - 200;
          continue;
        }
        (byLine[i] ??= []).add(d);
      }
    }
    _diagnosticsByLine = byLine;
    controller.forceRepaint();
    _secondary?.forceRepaint();
  }

  Map<int, List<LspDiagnostic>> _diagnosticsByLine = const {};

  String savedVersion;
  String _savedText;
  bool dirty = false;
  bool changedOnDisk = false;

  /// Where the editor was (restored when the tab is shown again).
  double scrollOffset = 0;

  /// The right group's view of this document. re_editor binds one editor per controller, so
  /// the second view has its own controller; the two share the text (`codeLines` is immutable,
  /// so keeping them in step is a reference copy) and keep their own caret and undo history.
  CodeLineEditingController get secondary {
    final existing = _secondary;
    if (existing != null) return existing;
    final c = CodeLineEditingController(
      codeLines: controller.codeLines,
      options: controller.options,
      spanBuilder: _span,
    );
    void fromPrimary() {
      if (_syncing || identical(c.codeLines, controller.codeLines)) return;
      _syncing = true;
      c.codeLines = controller.codeLines;
      _syncing = false;
    }

    void fromSecondary() {
      if (_syncing || identical(c.codeLines, controller.codeLines)) return;
      _syncing = true;
      controller.codeLines = c.codeLines;
      _syncing = false;
    }

    controller.addListener(fromPrimary);
    c.addListener(fromSecondary);
    _unlink = () {
      controller.removeListener(fromPrimary);
      c.removeListener(fromSecondary);
    };
    return _secondary = c;
  }

  CodeLineEditingController? _secondary;
  VoidCallback? _unlink;
  bool _syncing = false;

  /// Ends the right group's view (its controller is disposed after the frame).
  CodeLineEditingController? dropSecondary() {
    _unlink?.call();
    _unlink = null;
    final c = _secondary;
    _secondary = null;
    return c;
  }

  String get title => titleOf(path);

  /// Outside the workspace: read-only (see [isExternal]); also while the daemon is gone and the
  /// document waits to attach to the folder again (AUD-001).
  bool get readOnly => isExternal(path) || detached;

  /// The daemon went away: the document keeps its text (and unsaved changes) until the folder is
  /// open again (AUD-001).
  bool detached = false;

  /// The file was deleted on disk while the document was open (AUD-019).
  bool deletedOnDisk = false;

  /// The text as last saved or read (the dirty mark compares with it).
  String get savedText => _savedText;

  /// The lines the recovery copy was last scheduled for (AUD-001).
  Object? recoveryLines;
  String get text => controller.text;

  /// Recomputes [dirty]; true when it changed.
  /// The lines the dirty mark was last computed for: a caret or selection move keeps them, so it
  /// costs nothing (the comparison joins the whole document).
  Object? _dirtyLines;

  bool refreshDirty() {
    final lines = controller.codeLines;
    if (identical(lines, _dirtyLines)) return false;
    _dirtyLines = lines;
    // A different length is a difference, found without joining the document (most edits);
    // only an edit that keeps the length compares the text (audit finding 8).
    final length = textLength(lines, controller.options.lineBreak.value.length);
    final now = (length != null && length != _savedText.length) || controller.text != _savedText;
    if (now == dirty) return false;
    dirty = now;
    return true;
  }

  void markSaved(String version, String text, {String? disk}) {
    savedVersion = version;
    if (disk != null) _breaks = mixedBreaks(disk);
    _savedText = text;
    changedOnDisk = false;
    deletedOnDisk = false;
    _dirtyLines = null;
    refreshDirty();
  }

  void replaceText(String text, String version) {
    final shown = editorText(text);
    // The saved state first: the controller's listeners run during the assignment below and
    // must see a clean document, or the recovery copy keeps the reloaded text and the next start
    // offers it back over a later change on disk (review B1).
    savedVersion = version;
    _savedText = shown;
    dirty = false;
    changedOnDisk = false;
    deletedOnDisk = false;
    controller.text = shown;
    _dirtyLines = controller.codeLines;
  }

  /// Puts recovered unsaved [text] into the document (the saved text stays as it is on disk).
  void restoreUnsaved(String text) {
    controller.text = text;
    _dirtyLines = null;
    refreshDirty();
  }
}

/// A question about unsaved work.
final class Prompt {
  const Prompt(this.kind, this.names);

  /// `close`, `folder` or `quit`.
  final String kind;
  final List<String> names;
}

class DocumentsStore extends ChangeNotifier {
  DocumentsStore(this.client, this.workspace, {RecoveryStore? recovery})
    : recovery = recovery ?? RecoveryStore(null) {
    _offChange = client.on(Notifications.fsDidChange, _onDidChange);
    _offOpened = workspace.onOpened(() => unawaited(_restoreTabs()));
    _offClosed = workspace.onClosed(_onClosed);
  }

  final DaemonClient client;
  final WorkspaceStore workspace;

  /// Unsaved text kept on disk (AUD-001).
  final RecoveryStore recovery;

  /// The folder the documents belonged to when the daemon went away.
  String _detachedRoot = '';

  /// The root the open documents belong to (the recovery copies are filed under it).
  String _root = '';
  late final void Function() _offChange, _offOpened, _offClosed;

  final docs = <Doc>[];
  String activePath = '';

  /// The right editor group: a subset of the open documents and the one it shows.
  ({List<String> paths, String active})? split;
  String banner = '';
  Prompt? prompt;
  final _conflicts = <({String path, String text})>[];

  /// The status bar's caret text ("Ln 3, Col 7"), apart from the store: a caret or selection
  /// move redraws the status bar, not every listener of the documents.
  final cursor = ValueNotifier<String>('');
  String get cursorText => cursor.value;
  set cursorText(String v) => cursor.value = _cursorPending = v;

  /// Where to put the caret when a document shows (Go to File, Problems, search results).
  ({String path, int line, int column})? pendingReveal;

  /// The document whose editor takes keyboard focus when it shows: opening by keyboard (Enter,
  /// Go to File) moves focus to the editor; a click in the explorer leaves focus there.
  String? focusRequest;

  VoidCallback? onQuitConfirmed;

  /// A document was saved (the language servers are told).
  void Function(String path)? onSaved;
  final _opening = <String>{};
  int _generation = 0;
  bool _restoring = false;
  Timer? _tabSave;
  VoidCallback? _proceed;
  List<String> _promptPaths = const [];

  Doc? doc(String path) {
    for (final d in docs) {
      if (d.path == path) return d;
    }
    return null;
  }

  Doc? get active => doc(activePath);
  String? get conflictName => _conflicts.isEmpty ? null : titleOf(_conflicts.first.path);
  List<String> get dirtyPaths => [
    for (final d in docs)
      if (d.dirty) d.path,
  ];

  // ---- opening -------------------------------------------------------------------------

  /// Opens [path] (or shows it if open), optionally at [line]:[column] (1-based).
  /// Opens a read-only document whose [text] a language server gave (a class file's source), or
  /// shows it when it is open.
  Future<void> openText(String path, String text, {int? line, int? column}) async {
    if (line != null) pendingReveal = (path: path, line: line, column: column ?? 1);
    focusRequest = path;
    if (doc(path) == null) {
      docs.add(Doc(path: path, text: text, savedVersion: ''));
    }
    _setActive(path);
    notifyListeners();
  }

  Future<void> open(
    String path, {
    int? line,
    int? column,
    bool background = false,
    bool focus = true,
  }) async {
    if (line != null) pendingReveal = (path: path, line: line, column: column ?? 1);
    focusRequest = focus && !background ? path : null;
    if (doc(path) != null) {
      if (!background) {
        _setActive(path);
        // A failed open's message is about that file, not the one shown now (AUD-063).
        if (banner.startsWith('Cannot open')) banner = '';
      }
      notifyListeners();
      return;
    }
    await _openInternal(path, silent: false, activate: !background);
  }

  Future<void> _openInternal(String path, {required bool silent, required bool activate}) async {
    if (!workspace.isOpen || _opening.contains(path)) return;
    _opening.add(path);
    final gen = _generation;
    try {
      final r = await client.call(
        Methods.fsReadFile,
        ReadFileParams(workspaceId: workspace.id, path: path),
      );
      if (gen != _generation || doc(path) != null) return;
      final d = Doc(path: path, text: r.text, savedVersion: r.version);
      _watch(d);
      docs.add(d);
      banner = '';
      if (activate || activePath.isEmpty) _setActive(path);
      notifyListeners();
    } on DaemonException catch (e) {
      if (gen != _generation || silent) return;
      banner = 'Cannot open ${titleOf(path)}: ${_cannotOpenReason(e)}';
      notifyListeners();
    } finally {
      _opening.remove(path);
    }
  }

  static String _cannotOpenReason(DaemonException e) => switch (e.kind) {
    ErrorKind.unsupportedEncoding => 'it is not a UTF-8 text file',
    ErrorKind.tooLarge => 'it is too large (over 16 MiB)',
    _ => e.message,
  };

  /// Failures that say nothing about the file (the backend stopped or did not answer).
  static const _transient = {
    ErrorKind.internal,
    ErrorKind.timeout,
    ErrorKind.cancelled,
    ErrorKind.notInitialized,
    ErrorKind.workspaceNotOpen,
  };

  /// Why [path] does not open; null when it does.
  Future<DaemonException?> _readFailure(String path) async {
    try {
      await client.call(Methods.fsReadFile, ReadFileParams(workspaceId: workspace.id, path: path));
      return null;
    } on DaemonException catch (e) {
      return e;
    }
  }

  /// Follows [d]'s edits: the dirty mark, and the recovery copy of unsaved text.
  void _watch(Doc d) {
    d.recoveryLines = d.controller.codeLines;
    d.controller.addListener(() {
      if (d.refreshDirty()) _notifySafely();
      final lines = d.controller.codeLines;
      if (identical(lines, d.recoveryLines)) return; // a caret move
      d.recoveryLines = lines;
      _keepOrForget(d);
    });
  }

  void _keepOrForget(Doc d) {
    if (d.readOnly && !d.detached) return;
    final root = _root.isNotEmpty ? _root : workspace.root;
    if (d.dirty) {
      recovery.keep(root, d.path, () => d.text, d.savedVersion);
    } else {
      recovery.forget(root, d.path);
    }
  }

  void _setActive(String path) {
    if (activePath == path) return;
    activePath = path;
    cursorText = '';
    _scheduleTabSave();
  }

  /// Sets [path]'s language for the session (the status bar's picker).
  void setLanguage(String path, Language language) {
    final d = doc(path);
    if (d == null || d.language.id == language.id) return;
    d.languageNotifier.value = language;
    notifyListeners();
  }

  void activate(String path) {
    if (doc(path) == null) return;
    if (banner.startsWith('Cannot open')) banner = '';
    _setActive(path);
    notifyListeners();
  }

  /// Ctrl+Tab / Ctrl+Shift+Tab.
  void cycle(int delta) {
    if (docs.length < 2) return;
    final i = docs.indexWhere((d) => d.path == activePath);
    activate(docs[((i < 0 ? 0 : i) + delta + docs.length) % docs.length].path);
  }

  /// Moves a tab (Ctrl+Shift+PgUp/PgDn, or a drag).
  void moveTab(String path, int to) {
    final from = docs.indexWhere((d) => d.path == path);
    if (from < 0) return;
    to = to.clamp(0, docs.length - 1);
    if (to == from) return;
    docs.insert(to, docs.removeAt(from));
    _scheduleTabSave();
    notifyListeners();
  }

  void setCursor(int line, int column, int selected) {
    final text = selected > 0
        ? 'Ln $line, Col $column ($selected selected)'
        : 'Ln $line, Col $column';
    if (text == _cursorPending) return;
    _cursorPending = text;
    // Like _notifySafely: the caret moves while the editor builds or lays out; the status bar
    // hears of it after the frame.
    final phase = SchedulerBinding.instance.schedulerPhase;
    if (phase == SchedulerPhase.idle || phase == SchedulerPhase.postFrameCallbacks) {
      cursor.value = text;
      return;
    }
    if (_cursorQueued) return;
    _cursorQueued = true;
    SchedulerBinding.instance.addPostFrameCallback((_) {
      _cursorQueued = false;
      cursor.value = _cursorPending;
    });
  }

  String _cursorPending = '';
  bool _cursorQueued = false;

  bool _notifyQueued = false;

  /// Controller-driven updates (dirty marks, the cursor) can arrive while the editor builds or
  /// lays out; listeners are then told after the frame, once.
  void _notifySafely() {
    final phase = SchedulerBinding.instance.schedulerPhase;
    if (phase == SchedulerPhase.idle || phase == SchedulerPhase.postFrameCallbacks) {
      notifyListeners();
      return;
    }
    if (_notifyQueued) return;
    _notifyQueued = true;
    SchedulerBinding.instance.addPostFrameCallback((_) {
      _notifyQueued = false;
      notifyListeners();
    });
  }

  void dismissBanner() {
    banner = '';
    notifyListeners();
  }

  // ---- the right group ----------------------------------------------------------------------

  void splitRight([String? path]) {
    path ??= activePath;
    if (path.isEmpty || doc(path) == null) return;
    final paths = split?.paths ?? <String>[];
    split = (paths: paths.contains(path) ? paths : [...paths, path], active: path);
    _scheduleTabSave();
    notifyListeners();
  }

  void activateSplit(String path) {
    final s = split;
    if (s == null || !s.paths.contains(path)) return;
    split = (paths: s.paths, active: path);
    _scheduleTabSave();
    notifyListeners();
  }

  void closeInSplit(String path) {
    final s = split;
    if (s == null || !s.paths.contains(path)) return;
    final i = s.paths.indexOf(path);
    final paths = [...s.paths]..remove(path);
    final active = s.active == path
        ? (paths.isEmpty ? '' : paths[i.clamp(0, paths.length - 1)])
        : s.active;
    split = paths.isEmpty ? null : (paths: paths, active: active);
    _scheduleTabSave();
    notifyListeners();
  }

  void unsplit() {
    final s = split;
    if (s == null) return;
    split = null;
    for (final p in s.paths) {
      _dropLater(doc(p));
    }
    _scheduleTabSave();
    notifyListeners();
  }

  void _dropLater(Doc? d) {
    final c = d?.dropSecondary();
    if (c != null) SchedulerBinding.instance.addPostFrameCallback((_) => c.dispose());
  }

  // ---- closing ----------------------------------------------------------------------------

  void close(String path) {
    final d = doc(path);
    if (d == null) return;
    if (d.dirty) {
      _guarded('close', [path], () => _closeNow(path));
    } else {
      _closeNow(path);
    }
  }

  void _closeNow(String path) {
    final i = docs.indexWhere((d) => d.path == path);
    if (i < 0) return;
    final wasActive = activePath == path;
    if (split?.paths.contains(path) ?? false) closeInSplit(path);
    final d = docs.removeAt(i);
    recovery.forget(_root, d.path);
    final second = d.dropSecondary();
    _conflicts.removeWhere((c) => c.path == path);
    if (wasActive) {
      activePath = '';
      cursorText = '';
      if (docs.isNotEmpty) _setActive(docs[i.clamp(0, docs.length - 1)].path);
    }
    _scheduleTabSave();
    notifyListeners();
    // After this frame: the editor may still hold the controller until it rebuilds.
    SchedulerBinding.instance.addPostFrameCallback((_) {
      second?.dispose();
      d.controller.dispose();
    });
  }

  void _onClosed() {
    if (workspace.closingForLoss) {
      _detachAll();
    } else {
      _clearAll();
    }
  }

  /// The daemon went away: the documents stay, read-only, with their unsaved text (also written
  /// to the recovery folder now, in case the shell goes too), until the folder opens again
  /// (AUD-001, requirement N3).
  void _detachAll() {
    ++_generation;
    _tabSave?.cancel();
    _detachedRoot = _root.isNotEmpty ? _root : workspace.lostRoot;
    for (final d in docs) {
      if (!isExternal(d.path)) d.detached = true;
    }
    _conflicts.clear();
    prompt = null;
    _proceed = null;
    final unsaved = dirtyPaths.length;
    banner = unsaved == 0
        ? ''
        : 'The backend stopped. Your unsaved changes to ${unsaved == 1 ? titleOf(dirtyPaths.first) : '$unsaved files'} are kept and come back when it restarts.';
    for (final d in docs) {
      if (d.dirty) recovery.keep(_detachedRoot, d.path, () => d.text, d.savedVersion);
    }
    unawaited(recovery.flush());
    notifyListeners();
  }

  /// The folder (the same one) is open again: the kept documents attach to it. A clean document
  /// takes the text on disk; a dirty one keeps its text, and is marked changed (or deleted) on
  /// disk when the file moved on meanwhile.
  Future<void> _reattach() async {
    final kept = [
      for (final d in docs)
        if (d.detached) d,
    ];
    if (kept.isEmpty) return;
    if (workspace.root != _detachedRoot) {
      // Another folder opened: the unsaved text stays in the recovery folder for its own folder.
      await recovery.flush();
      final unsaved = kept.where((d) => d.dirty).length;
      _drop(kept);
      if (unsaved > 0) {
        final name = _detachedRoot.split(RegExp(r'[/\\]')).where((x) => x.isNotEmpty).lastOrNull;
        banner =
            'Unsaved changes to $unsaved file${unsaved == 1 ? '' : 's'} in ${name ?? _detachedRoot} are kept; open that folder again to get them back.';
      }
      notifyListeners();
      return;
    }
    final gen = _generation;
    for (final d in kept) {
      ReadFileResult? r;
      var gone = false;
      try {
        r = await client.call(
          Methods.fsReadFile,
          ReadFileParams(workspaceId: workspace.id, path: d.path),
        );
      } on DaemonException catch (e) {
        gone = e.kind == ErrorKind.notFound;
      }
      if (gen != _generation) return;
      d.detached = false;
      if (r == null) {
        if (gone && !d.dirty) {
          _drop([d]);
        } else if (gone) {
          d.deletedOnDisk = true;
        }
        continue;
      }
      // The file is there (again): not "deleted on disk" any more (review N10).
      d.deletedOnDisk = false;
      if (!d.dirty) {
        if (d.editorText(r.text) != d.text) {
          d.replaceText(r.text, r.version);
        } else {
          d.savedVersion = r.version;
        }
      } else if (d.editorText(r.text) == d.savedText) {
        d.savedVersion = r.version;
        d.changedOnDisk = false;
      } else {
        d.changedOnDisk = true;
      }
    }
    _detachedRoot = '';
    if (banner.startsWith('The backend stopped') || banner.contains('the backend is stopped')) {
      banner = '';
    }
    notifyListeners();
  }

  /// Unsaved text kept for this folder from an earlier session (the shell or the daemon ended
  /// with unsaved changes): opened again as unsaved changes.
  Future<void> _recover() async {
    final entries = recovery.entries(workspace.root);
    if (entries.isEmpty) return;
    final gen = _generation;
    var n = 0;
    final gone = <String>[];
    final unreadable = <({String path, String reason})>[];
    for (final e in entries) {
      if (gen != _generation) return;
      var d = doc(e.path);
      if (d == null) {
        await _openInternal(e.path, silent: true, activate: false);
        if (gen != _generation) return;
        d = doc(e.path);
      }
      if (d == null) {
        // Empty text whose file does not open: nothing to give back, and the entry would stay
        // for ever (review D6).
        if (e.text.isEmpty) {
          recovery.forget(workspace.root, e.path);
          continue;
        }
        final failure = await _readFailure(e.path);
        if (gen != _generation) return;
        if (doc(e.path) != null) continue; // opened meanwhile
        // The backend stopped (or the file opens now): the entry is kept and offered on the next
        // open of the folder.
        if (failure == null || _transient.contains(failure.kind)) continue;
        // The file is gone (deleted, or moved while the shell was not running): the text opens as
        // an unsaved document of that name, which Save creates again, with its folder; closing it
        // asks first (review N10, D3). A file that is there but cannot be opened (no longer
        // UTF-8, too large, not readable) is not left in the recovery folder unseen: the text
        // opens the same way, and as nothing was read of the file, Save asks before it replaces
        // it ("changed on disk"); closing it without saving forgets the text (review D6).
        final deleted = failure.kind == ErrorKind.notFound;
        // Made from the text (its line breaks are kept), with nothing saved: every line unsaved.
        d = Doc(path: e.path, text: e.text, savedVersion: '')
          ..markSaved('', '')
          ..deletedOnDisk = deleted;
        _watch(d);
        docs.add(d);
        if (activePath.isEmpty) _setActive(d.path);
        _scheduleTabSave();
        if (deleted) {
          gone.add(e.path);
        } else {
          unreadable.add((path: e.path, reason: _cannotOpenReason(failure)));
        }
        continue;
      }
      if (d.dirty || d.text == e.text) {
        if (!d.dirty) recovery.forget(workspace.root, e.path);
        continue;
      }
      if (e.savedVersion.isNotEmpty && d.savedVersion != e.savedVersion) {
        // The file changed since the text was kept: the save is checked against the version the
        // text was based on, so it asks rather than overwrite the newer file (review B2).
        d.changedOnDisk = true;
        d.savedVersion = e.savedVersion;
      }
      d.restoreUnsaved(e.text);
      n++;
    }
    if (n > 0 || gone.isNotEmpty || unreadable.isNotEmpty) {
      final missing = gone.isEmpty
          ? ''
          : ' ${gone.length == 1 ? '${titleOf(gone.first)} is no longer on disk' : '${gone.length} of them are no longer on disk'}: Save creates ${gone.length == 1 ? 'it' : 'them'} again.';
      final u = unreadable.length;
      final cannot = u == 0
          ? ''
          : ' ${u == 1 ? '${titleOf(unreadable.first.path)} cannot be opened (${unreadable.first.reason})' : '$u of them cannot be opened'}: Save asks before it replaces ${u == 1 ? 'the file' : 'them'}.';
      final total = n + gone.length + u;
      banner =
          'Recovered unsaved changes to ${total == 1 ? '1 file' : '$total files'} from the last session.$missing$cannot';
      notifyListeners();
    }
  }

  /// Removes [list] from the open documents (their controllers are disposed after the frame).
  void _drop(List<Doc> list) {
    for (final d in list) {
      docs.remove(d);
      if (split?.paths.contains(d.path) ?? false) closeInSplit(d.path);
      if (activePath == d.path) activePath = docs.isEmpty ? '' : docs.first.path;
    }
    final seconds = [for (final d in list) d.dropSecondary()];
    SchedulerBinding.instance.addPostFrameCallback((_) {
      for (final c in seconds) {
        c?.dispose();
      }
      for (final d in list) {
        d.controller.dispose();
      }
    });
  }

  void _clearAll() {
    ++_generation;
    _tabSave?.cancel();
    final old = [...docs];
    for (final d in old) {
      recovery.forget(_root, d.path);
    }
    _root = '';
    _detachedRoot = '';
    docs.clear();
    split = null;
    activePath = '';
    cursorText = '';
    _conflicts.clear();
    prompt = null;
    _proceed = null;
    notifyListeners();
    final seconds = [for (final d in old) d.dropSecondary()];
    SchedulerBinding.instance.addPostFrameCallback((_) {
      for (final c in seconds) {
        c?.dispose();
      }
      for (final d in old) {
        d.controller.dispose();
      }
    });
  }

  // ---- saving -------------------------------------------------------------------------------

  /// Runs before a save writes (format on save, ADR-0021 §8); its failure does not stop the save.
  Future<void> Function(Doc d)? beforeSave;

  Future<void> save(String path) async {
    final d = doc(path);
    if (d == null || d.readOnly) return;
    final gen = _generation;
    try {
      await beforeSave?.call(d);
    } on Object {
      // The text is saved as it is.
    }
    // The folder may have changed while the formatter ran: this document's text is not written
    // into the next folder's file of the same name (AUD-002, review B3).
    if (gen != _generation || !identical(doc(path), d)) return;
    await _write(d, d.text, overwrite: false);
  }

  Future<void> saveAll() async {
    for (final p in dirtyPaths) {
      await save(p);
    }
  }

  /// Saves of one document run one after another: two in flight sent the same
  /// `expectedVersion`, and the second came back as a false conflict (AUD-024).
  final _saving = <String, Future<bool>>{};

  /// True when the save succeeded.
  Future<bool> _write(Doc d, String text, {required bool overwrite}) {
    // The folder and generation of the request, not of the moment a queued save runs: a save
    // waiting behind another must not land in a folder opened meanwhile (review B3).
    final gen = _generation;
    final workspaceId = workspace.id;
    final before = _saving[d.path];
    final next = (before == null ? Future<bool>.value(true) : before.then((_) => true)).then(
      (_) => _writeNow(d, text, overwrite: overwrite, gen: gen, workspaceId: workspaceId),
    );
    _saving[d.path] = next;
    next.whenComplete(() {
      if (identical(_saving[d.path], next)) _saving.remove(d.path);
    });
    return next;
  }

  Future<bool> _writeNow(
    Doc d,
    String text, {
    required bool overwrite,
    required int gen,
    required String workspaceId,
  }) async {
    if (gen != _generation || !identical(doc(d.path), d) || d.detached) return false;
    // A save queued behind one that conflicted would conflict too: the question already asked
    // answers it, with this newer text (one question, not two; review N10).
    final asked = _conflicts.indexWhere((c) => c.path == d.path);
    if (!overwrite && asked >= 0) {
      _conflicts[asked] = (path: d.path, text: text);
      return false;
    }
    final disk = d.diskText(text);
    // A file deleted on disk is created again, and so is its folder when that is gone too: the
    // write alone fails with "not found" and the question would come back (review D3).
    if (d.deletedOnDisk) {
      await _createParent(workspaceId, d.path);
      if (gen != _generation || !identical(doc(d.path), d)) return false;
    }
    try {
      final r = await client.call(
        Methods.fsWriteFile,
        WriteFileParams(
          workspaceId: workspaceId,
          path: d.path,
          text: disk,
          overwrite: overwrite ? true : null,
          expectedVersion: overwrite || d.savedVersion.isEmpty ? null : d.savedVersion,
        ),
      );
      if (gen != _generation) return false;
      d.markSaved(r.version, text, disk: disk);
      if (!d.dirty) recovery.forget(_root, d.path);
      onSaved?.call(d.path);
      notifyListeners();
      return true;
    } on DaemonException catch (e) {
      if (gen != _generation) return false;
      if (e.kind == ErrorKind.conflict) {
        // A file deleted on disk is not "changed": its own question (AUD-019). Asked of the disk
        // each time: a document marked deleted whose file another program has created since is
        // "changed", or the "deleted" question's Save would replace that file (review C4).
        d.deletedOnDisk = !await _existsOnDisk(d.path);
        if (gen != _generation) return false;
        _conflicts.add((path: d.path, text: text));
      } else if (e.kind == ErrorKind.notFound) {
        d.deletedOnDisk = true;
        _conflicts.add((path: d.path, text: text));
      } else {
        banner = 'Could not save ${d.title}: ${e.message}';
      }
      notifyListeners();
      return false;
    }
  }

  /// Whether the first conflict is a file deleted on disk (the dialog asks differently).
  bool get conflictIsDeleted =>
      _conflicts.isNotEmpty && (doc(_conflicts.first.path)?.deletedOnDisk ?? false);

  /// Creates the folders above [path] that are missing (none for a file at the root). A failure
  /// is left to the write that follows, which says why it could not save.
  Future<void> _createParent(String workspaceId, String path) async {
    final parent = parentPath(path);
    if (parent.isEmpty) return;
    try {
      await client.call(
        Methods.fsCreateDirectory,
        CreateDirectoryParams(workspaceId: workspaceId, path: parent, parents: true),
      );
    } on DaemonException {
      // Already there (the usual case), or the write reports the reason.
    }
  }

  Future<bool> _existsOnDisk(String path) async {
    try {
      await client.call(Methods.fsReadFile, ReadFileParams(workspaceId: workspace.id, path: path));
      return true;
    } on DaemonException catch (e) {
      return e.kind != ErrorKind.notFound;
    }
  }

  /// The answer to "changed on disk": overwrite it with ours, reload theirs, or leave it. For a
  /// file deleted on disk: `overwrite` writes it again (recreates it), `close` closes the tab
  /// without saving (AUD-019).
  Future<void> resolveConflict(String action) async {
    if (_conflicts.isEmpty) return;
    final c = _conflicts.removeAt(0);
    notifyListeners();
    final d = doc(c.path);
    if (d == null) return;
    if (action == 'overwrite') {
      await _write(d, c.text, overwrite: true);
    } else if (action == 'reload' && !d.deletedOnDisk) {
      await _reloadFromDisk(c.path);
    } else if (action == 'close') {
      d.dirty = false;
      _closeNow(c.path);
    }
  }

  Future<void> _reloadFromDisk(String path) async {
    if (!workspace.isOpen) return;
    final gen = _generation;
    final ReadFileResult r;
    try {
      r = await client.call(
        Methods.fsReadFile,
        ReadFileParams(workspaceId: workspace.id, path: path),
      );
    } on DaemonException catch (e) {
      // E.g. a recovered text over a file that cannot be opened (review D6): the text stays.
      if (gen != _generation || doc(path) == null) return;
      banner = 'Cannot open ${titleOf(path)}: ${_cannotOpenReason(e)}';
      notifyListeners();
      return;
    }
    final d = doc(path);
    if (gen != _generation || d == null) return;
    d.replaceText(r.text, r.version);
    notifyListeners();
  }

  // ---- external changes ---------------------------------------------------------------------

  void _onDidChange(DidChangeParams p) {
    if (p.workspaceId != workspace.id) return;
    final everything = p.changes.any((c) => c.path.isEmpty);
    final changed = {for (final c in p.changes) c.path};
    for (final d in [...docs]) {
      if (everything || changed.contains(d.path)) unawaited(_recheck(d.path));
    }
  }

  /// The version is the arbiter: our own saves and unchanged content cause no action.
  Future<void> _recheck(String path) async {
    if (!workspace.isOpen) return;
    final gen = _generation;
    ReadFileResult? r;
    var gone = false;
    try {
      r = await client.call(
        Methods.fsReadFile,
        ReadFileParams(workspaceId: workspace.id, path: path),
      );
    } on DaemonException catch (e) {
      r = null;
      gone = e.kind == ErrorKind.notFound;
    }
    final d = doc(path);
    if (gen != _generation || d == null) return;
    if (r == null) {
      // Deleted on disk: the tab says so (AUD-019); a dirty one keeps its text to save again.
      if (gone && !d.deletedOnDisk) {
        d.deletedOnDisk = true;
        notifyListeners();
      } else if (!gone && d.dirty && !d.changedOnDisk) {
        d.changedOnDisk = true;
        notifyListeners();
      }
      return;
    }
    if (d.deletedOnDisk) d.deletedOnDisk = false;
    if (r.version == d.savedVersion) return;
    if (!d.dirty) {
      d.replaceText(r.text, r.version);
      notifyListeners();
    } else if (!d.changedOnDisk) {
      d.changedOnDisk = true;
      notifyListeners();
    }
  }

  // ---- unsaved-work prompts -----------------------------------------------------------------

  void _guarded(String kind, List<String> dirty, VoidCallback proceed) {
    if (dirty.isEmpty) {
      proceed();
      return;
    }
    if (prompt != null) return; // one question at a time
    _promptPaths = dirty;
    _proceed = proceed;
    prompt = Prompt(kind, [for (final p in dirty) titleOf(p)]);
    notifyListeners();
  }

  Future<void> resolvePrompt(String action) async {
    if (prompt == null) return;
    final proceed = _proceed;
    final paths = _promptPaths;
    _proceed = null;
    prompt = null;
    notifyListeners();
    if (proceed == null) return;
    if (action == 'save') {
      // A document kept while the backend is stopped cannot be saved until it is back: say so
      // rather than do nothing (review N10). Its text stays, and in the recovery folder.
      final waiting = [
        for (final p in paths)
          if (doc(p)?.detached ?? false) p,
      ];
      if (waiting.isNotEmpty) {
        final what = waiting.length == 1 ? titleOf(waiting.first) : '${waiting.length} files';
        banner =
            'Could not save $what: the backend is stopped. Restart it, then save; your changes are kept until then.';
        notifyListeners();
        return;
      }
      for (final p in paths) {
        final d = doc(p);
        if (d == null) continue;
        if (!await _write(d, d.text, overwrite: false)) return; // a failed save stops here
      }
      proceed();
    } else if (action == 'discard') {
      for (final p in paths) {
        doc(p)?.dirty = false;
        recovery.forget(_root, p);
      }
      proceed();
    }
  }

  /// [from] was renamed to [to] (a file, or a folder holding open documents): the documents
  /// follow, with their unsaved text (AUD-022).
  void renamed(String from, String to) {
    String? moved(String p) => p == from
        ? to
        : p.startsWith('$from/')
        ? '$to${p.substring(from.length)}'
        : null;
    var any = false;
    for (final d in docs) {
      final n = moved(d.path);
      if (n == null) continue;
      recovery.forget(_root, d.path);
      d.path = n;
      if (d.dirty) recovery.keep(_root, n, () => d.text, d.savedVersion);
      any = true;
    }
    if (!any) return;
    activePath = moved(activePath) ?? activePath;
    final s = split;
    if (s != null) {
      split = (
        paths: [for (final p in s.paths) moved(p) ?? p],
        active: moved(s.active) ?? s.active,
      );
    }
    _scheduleTabSave();
    notifyListeners();
  }

  /// "Save As": writes [d]'s text to a new file [to] and shows that file instead; the old file
  /// is left as it was on disk. Null when done, else why not.
  Future<String?> saveAs(Doc d, String to) async {
    final err = await workspace.createFile(to, text: d.diskText(d.text));
    if (err != null) return err;
    final from = d.path;
    d.dirty = false;
    recovery.forget(_root, from);
    _closeNow(from);
    await open(to);
    return null;
  }

  void requestOpenFolder(String path) =>
      _guarded('folder', dirtyPaths, () => unawaited(workspace.openFolder(path)));
  void requestCloseFolder() => _guarded('folder', dirtyPaths, () => unawaited(workspace.close()));
  void requestQuit() => _guarded('quit', dirtyPaths, () => onQuitConfirmed?.call());

  // ---- tab persistence ----------------------------------------------------------------------

  void _scheduleTabSave() {
    if (_restoring || !workspace.isOpen) return;
    _tabSave?.cancel();
    _tabSave = Timer(_tabSaveDebounce, () => unawaited(saveTabState()));
  }

  Future<void> saveTabState() async {
    _tabSave?.cancel();
    if (!workspace.isOpen || _restoring) return;
    try {
      await client.call(
        Methods.workspaceSetState,
        SetStateParams(
          workspaceId: workspace.id,
          values_: {
            // Files outside the workspace are readable only while their server pointed to them.
            openPathsKey: [
              for (final d in docs)
                if (!d.readOnly) d.path,
            ],
            activePathKey: activePath.isEmpty || isExternal(activePath) ? null : activePath,
            splitPathsKey: split?.paths,
            splitActiveKey: split?.active,
          },
        ),
      );
    } on DaemonException {
      // Best effort.
    }
  }

  Future<void> _restoreTabs() async {
    if (!workspace.isOpen) return;
    _root = workspace.root;
    await _reattach();
    if (!workspace.isOpen) return;
    final gen = _generation;
    _restoring = true;
    try {
      final Map<String, Object?> values;
      try {
        values = (await client.call(
          Methods.workspaceGetState,
          GetStateParams(
            workspaceId: workspace.id,
            keys: const [openPathsKey, activePathKey, splitPathsKey, splitActiveKey],
          ),
        )).values_;
      } on DaemonException {
        return;
      }
      if (gen != _generation) return;
      final paths =
          (values[openPathsKey] as List?)?.whereType<String>().toList() ?? const <String>[];
      final active = values[activePathKey] as String? ?? '';
      // Sequentially, so tabs keep their order; files that are gone are skipped.
      for (final p in paths) {
        if (gen != _generation) return;
        await _openInternal(p, silent: true, activate: false);
      }
      if (gen != _generation) return;
      if (active.isNotEmpty && doc(active) != null) {
        _setActive(active);
      } else if (docs.isNotEmpty && activePath.isEmpty) {
        _setActive(docs.first.path);
      }
      final sp =
          (values[splitPathsKey] as List?)
              ?.whereType<String>()
              .where((p) => doc(p) != null)
              .toList() ??
          const [];
      if (sp.isNotEmpty) {
        final sa = values[splitActiveKey] as String?;
        split = (paths: sp, active: sa != null && sp.contains(sa) ? sa : sp.first);
      }
      notifyListeners();
    } finally {
      if (gen == _generation) _restoring = false;
    }
    if (gen == _generation) await _recover();
  }

  @override
  void dispose() {
    _offChange();
    _offOpened();
    _offClosed();
    _tabSave?.cancel();
    recovery.dispose();
    super.dispose();
  }
}
