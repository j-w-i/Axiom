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
bool isExternal(String path) => RegExp(r'^[a-z][a-z0-9+.-]*://').hasMatch(path);

/// The name a document's language is detected from: a class file's source is Java.
String _languageName(String path) =>
    path.startsWith('jdt://') ? '${titleOf(path).replaceAll(RegExp(r'\.class$'), '')}.java' : path;

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
  for (var i = 0; i < n; i++) {
    final line = lines[i];
    if (line.chunks.isNotEmpty) return null;
    length += line.text.length;
  }
  return length;
}

String _firstLine(String text) {
  final i = text.indexOf('\n');
  return i < 0 ? text : text.substring(0, i);
}

class Doc {
  Doc({required this.path, required String text, required this.savedVersion})
    : _savedText = text,
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
    final n = before.length < now.length ? before.length : now.length;
    var at = 0;
    while (at < n && before[at].text == now[at].text) {
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

  final String path;

  /// The document's language: detected from its name and first line; the status bar's picker
  /// changes it for the session.
  final ValueNotifier<Language> languageNotifier;
  Language get language => languageNotifier.value;
  late final CodeLineEditingController controller;

  /// The language servers' diagnostics on this document (set by the LSP layer).
  List<LspDiagnostic> diagnostics = const [];

  /// Underline colours by LSP severity (set by the editor from the theme).
  Color Function(int severity) underlineColor = (_) => AxiomColors.dark.danger;

  TextSpan _span({
    required BuildContext context,
    required int index,
    required CodeLine codeLine,
    required TextSpan textSpan,
    required TextStyle style,
  }) {
    var span = textSpan;
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

  /// Outside the workspace: read-only (see [isExternal]).
  bool get readOnly => isExternal(path);
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

  void markSaved(String version, String text) {
    savedVersion = version;
    _savedText = text;
    changedOnDisk = false;
    _dirtyLines = null;
    refreshDirty();
  }

  void replaceText(String text, String version) {
    controller.text = text;
    savedVersion = version;
    _savedText = text;
    dirty = false;
    changedOnDisk = false;
    _dirtyLines = controller.codeLines;
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
  DocumentsStore(this.client, this.workspace) {
    _offChange = client.on(Notifications.fsDidChange, _onDidChange);
    _offOpened = workspace.onOpened(() => unawaited(_restoreTabs()));
    _offClosed = workspace.onClosed(_clearAll);
  }

  final DaemonClient client;
  final WorkspaceStore workspace;
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
      if (!background) _setActive(path);
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
      d.controller.addListener(() {
        if (d.refreshDirty()) _notifySafely();
      });
      docs.add(d);
      banner = '';
      if (activate || activePath.isEmpty) _setActive(path);
      notifyListeners();
    } on DaemonException catch (e) {
      if (gen != _generation || silent) return;
      final reason = switch (e.kind) {
        ErrorKind.unsupportedEncoding => 'it is not a UTF-8 text file',
        ErrorKind.tooLarge => 'it is too large (over 16 MiB)',
        _ => e.message,
      };
      banner = 'Cannot open ${titleOf(path)}: $reason';
      notifyListeners();
    } finally {
      _opening.remove(path);
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

  void _clearAll() {
    ++_generation;
    _tabSave?.cancel();
    final old = [...docs];
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
    try {
      await beforeSave?.call(d);
    } on Object {
      // The text is saved as it is.
    }
    await _write(d, d.text, overwrite: false);
  }

  Future<void> saveAll() async {
    for (final p in dirtyPaths) {
      await save(p);
    }
  }

  /// True when the save succeeded.
  Future<bool> _write(Doc d, String text, {required bool overwrite}) async {
    final gen = _generation;
    try {
      final r = await client.call(
        Methods.fsWriteFile,
        WriteFileParams(
          workspaceId: workspace.id,
          path: d.path,
          text: text,
          overwrite: overwrite ? true : null,
          expectedVersion: overwrite || d.savedVersion.isEmpty ? null : d.savedVersion,
        ),
      );
      if (gen != _generation) return false;
      d.markSaved(r.version, text);
      onSaved?.call(d.path);
      notifyListeners();
      return true;
    } on DaemonException catch (e) {
      if (gen != _generation) return false;
      if (e.kind == ErrorKind.conflict) {
        _conflicts.add((path: d.path, text: text));
      } else {
        banner = 'Could not save ${d.title}: ${e.message}';
      }
      notifyListeners();
      return false;
    }
  }

  /// The answer to "changed on disk": overwrite it with ours, reload theirs, or leave it.
  Future<void> resolveConflict(String action) async {
    if (_conflicts.isEmpty) return;
    final c = _conflicts.removeAt(0);
    notifyListeners();
    final d = doc(c.path);
    if (d == null) return;
    if (action == 'overwrite') {
      await _write(d, c.text, overwrite: true);
    } else if (action == 'reload') {
      await _reloadFromDisk(c.path);
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
    } on DaemonException {
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
    try {
      r = await client.call(
        Methods.fsReadFile,
        ReadFileParams(workspaceId: workspace.id, path: path),
      );
    } on DaemonException {
      r = null;
    }
    final d = doc(path);
    if (gen != _generation || d == null) return;
    if (r == null) {
      if (d.dirty && !d.changedOnDisk) {
        d.changedOnDisk = true;
        notifyListeners();
      }
      return;
    }
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
      for (final p in paths) {
        final d = doc(p);
        if (d == null) continue;
        if (!await _write(d, d.text, overwrite: false)) return; // a failed save stops here
      }
      proceed();
    } else if (action == 'discard') {
      for (final p in paths) {
        doc(p)?.dirty = false;
      }
      proceed();
    }
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
  }

  @override
  void dispose() {
    _offChange();
    _offOpened();
    _offClosed();
    _tabSave?.cancel();
    super.dispose();
  }
}
