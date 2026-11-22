// The editor's language-server client (FL-EDIT-002; the Tauri shell's editor/lsp/client.ts in
// Dart): document sync through `lsp/notify`, diagnostics onto the documents' underlines, and the
// requests the editor makes (completion, hover, definition, rename, formatting). The daemon
// owns the servers, starts them on first use and mirrors documents for them (ADR-0018 §2).

import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:re_editor/re_editor.dart';

import '../../daemon/client.dart';
import '../../state/documents.dart';
import '../../state/lsp.dart';

/// The LSP language id of a document: its language's id (the registry's, ADR-0021 §1), which
/// the status bar's picker may have changed.
String lspLanguageId(Doc d) => d.language.id;

/// One `contentChanges` entry for going from [before] to [after]: the changed lines as one range
/// when an unchanged line follows them (the common case: typing), else the whole text.
Map<String, Object?> contentChange(List<String> before, List<String> after) {
  final n = before.length, m = after.length;
  var p = 0;
  while (p < n && p < m && before[p] == after[p]) {
    p++;
  }
  var s = 0;
  while (s < n - p && s < m - p && before[n - 1 - s] == after[m - 1 - s]) {
    s++;
  }
  if (s == 0) return {'text': after.join('\n')};
  final oldEnd = n - s, newEnd = m - s;
  final b = StringBuffer();
  for (var i = p; i < newEnd; i++) {
    b
      ..write(after[i])
      ..write('\n');
  }
  return {
    'range': {
      'start': {'line': p, 'character': 0},
      'end': {'line': oldEnd, 'character': 0},
    },
    'text': b.toString(),
  };
}

List<String> _lines(CodeLines c) => [for (var i = 0; i < c.length; i++) c[i].text];

/// Keeps every open document in sync with its server and its diagnostics on screen.
class LspSync {
  LspSync({required this.docs, required this.lsp}) {
    docs.addListener(_onDocs);
    lsp.addListener(_onLsp);
  }

  final DocumentsStore docs;
  final LspStore lsp;
  final _open = <String, _Synced>{};
  int _seenDiagnostics = -1;

  String? _serverFor(Doc d) => lsp.serverFor(lspLanguageId(d))?.serverId;

  void _onDocs() {
    final now = {for (final d in docs.docs) d.path: d};
    for (final path in [..._open.keys]) {
      if (!now.containsKey(path)) _close(path);
    }
    for (final d in now.values) {
      // A document whose language was changed is closed and opened again under the new one.
      final s = _open[d.path];
      if (s != null && s.languageId != lspLanguageId(d)) _close(d.path);
      if (!_open.containsKey(d.path)) {
        _openDoc(d);
      }
    }
  }

  void _onLsp() {
    // Servers listed late (the workspace just opened): open what is waiting.
    for (final d in docs.docs) {
      if (!_open.containsKey(d.path)) {
        _openDoc(d);
      }
    }
    if (lsp.diagnosticsVersion == _seenDiagnostics) return;
    _seenDiagnostics = lsp.diagnosticsVersion;
    for (final d in docs.docs) {
      final list = lsp.diagnosticsOn(d.path);
      if (!identical(list, d.diagnostics) && !(list.isEmpty && d.diagnostics.isEmpty)) {
        d.setDiagnostics(list);
      }
    }
  }

  void _openDoc(Doc d) {
    final server = _serverFor(d);
    if (server == null) return;
    final s = _Synced(d, server, lspLanguageId(d));
    _open[d.path] = s;
    unawaited(
      _notify(server, 'textDocument/didOpen', {
        'textDocument': {
          'uri': lsp.uriOf(d.path),
          'languageId': s.languageId,
          'version': s.version,
          // LF, as every change after it is (the server's copy has one kind of line break).
          'text': _lines(d.controller.codeLines).join('\n'),
        },
      }),
    );
    s.lines = _lines(d.controller.codeLines);
    s.codeLines = d.controller.codeLines;
    void onChange() {
      final now = d.controller.codeLines;
      if (identical(now, s.codeLines)) return; // a caret move
      final after = _lines(now);
      final change = contentChange(s.lines, after);
      s.lines = after;
      s.codeLines = now;
      s.version++;
      // Full sync servers get the whole text; the daemon's mirror applies ranges otherwise.
      final full = _syncKind(server) == 1;
      unawaited(
        _notify(server, 'textDocument/didChange', {
          'textDocument': {'uri': lsp.uriOf(d.path), 'version': s.version},
          'contentChanges': [
            full ? {'text': after.join('\n')} : change,
          ],
        }),
      );
    }

    d.controller.addListener(onChange);
    s.detach = () => d.controller.removeListener(onChange);
    d.setDiagnostics(lsp.diagnosticsOn(d.path));
  }

  void _close(String path) {
    final s = _open.remove(path);
    if (s == null) return;
    s.detach?.call();
    unawaited(
      _notify(s.server, 'textDocument/didClose', {
        'textDocument': {'uri': lsp.uriOf(path)},
      }),
    );
  }

  /// Formats [d] with its server (format on save), when the server formats documents.
  Future<void> formatForSave(Doc d) async {
    final s = _open[d.path];
    if (s == null) return;
    final info = lsp.servers.where((x) => x.serverId == s.server).firstOrNull;
    final caps = info?.capabilities;
    if (caps is! Map || caps['documentFormattingProvider'] == null) return;
    if (caps['documentFormattingProvider'] == false) return;
    final before = d.controller.codeLines;
    final r = await lsp
        .request(s.server, 'textDocument/formatting', {
          'textDocument': {'uri': lsp.uriOf(d.path)},
          'options': {'tabSize': 4, 'insertSpaces': true},
        })
        .timeout(const Duration(seconds: 2));
    // Typing while the formatter worked: its ranges are for the text before, so the document is
    // saved as it is, unformatted (audit finding 5).
    if (!identical(d.controller.codeLines, before)) return;
    if (r is List) {
      applyEdits(d.controller, [
        for (final e in r.whereType<Map<Object?, Object?>>())
          (range: lspRange(e['range']), text: e['newText'] as String? ?? ''),
      ]);
    }
  }

  /// Tells the server the document was saved.
  void saved(String path) {
    final s = _open[path];
    if (s == null) return;
    unawaited(
      _notify(s.server, 'textDocument/didSave', {
        'textDocument': {'uri': lsp.uriOf(path)},
      }),
    );
  }

  /// `textDocumentSync` kind: 1 full, 2 incremental (unknown before the server is ready: the
  /// daemon's mirror takes ranges).
  int _syncKind(String serverId) {
    final caps = lsp.servers.where((s) => s.serverId == serverId).firstOrNull?.capabilities;
    if (caps is! Map) return 2;
    final sync = caps['textDocumentSync'];
    if (sync is num) return sync.toInt();
    if (sync is Map && sync['change'] is num) return (sync['change'] as num).toInt();
    return 2;
  }

  Future<void> _notify(String server, String method, Object? params) async {
    try {
      await lsp.notify(server, method, params);
    } on DaemonException catch (e) {
      debugPrint('lsp $method: ${e.message}');
    }
  }

  void dispose() {
    docs.removeListener(_onDocs);
    lsp.removeListener(_onLsp);
    for (final s in _open.values) {
      s.detach?.call();
    }
  }
}

class _Synced {
  _Synced(this.doc, this.server, this.languageId);
  final Doc doc;
  final String server;
  final String languageId;
  int version = 1;
  List<String> lines = const [];
  CodeLines? codeLines;
  VoidCallback? detach;
}

// ---- requests ---------------------------------------------------------------------------------

/// One completion item, ready to show and to insert.
final class CompletionChoice {
  const CompletionChoice({
    required this.label,
    required this.insert,
    this.detail,
    this.kind,
    this.range,
    this.filter,
    this.sort,
    this.snippet = false,
    this.labelDetail,
    this.documentation,
    this.additional = const [],
    this.commit = const [],
    this.preselect = false,
    this.raw,
  });
  final String label;

  /// The text to insert: a snippet (tab stops) when [snippet].
  final String insert;
  final String? detail;
  final int? kind;

  /// The server's edit range, when it gave one.
  final LspRange? range;
  final String? filter;
  final String? sort;
  final bool snippet;

  /// `labelDetails` (a signature, a module), shown faint after the label.
  final String? labelDetail;

  /// Markdown (or plain text) shown beside the list; from `completionItem/resolve` when absent.
  final String? documentation;

  /// Edits elsewhere (an import), applied with the item in one undo step.
  final List<({LspRange range, String text})> additional;

  /// Characters that accept the item and are then typed (`.`, `(`).
  final List<String> commit;
  final bool preselect;

  /// The item as the server sent it (`completionItem/resolve` takes it back).
  final Map<Object?, Object?>? raw;

  /// This item with what `completionItem/resolve` added.
  CompletionChoice resolved(Object? r) {
    if (r is! Map) return this;
    final docs = _documentation(r['documentation']);
    final more = r['additionalTextEdits'];
    return CompletionChoice(
      label: label,
      insert: insert,
      detail: r['detail'] as String? ?? detail,
      kind: kind,
      range: range,
      filter: filter,
      sort: sort,
      snippet: snippet,
      labelDetail: labelDetail,
      documentation: docs ?? documentation ?? '',
      additional: more is List ? _edits(more) : additional,
      commit: commit,
      preselect: preselect,
      raw: raw,
    );
  }
}

String? _documentation(Object? d) {
  if (d is String) return d;
  if (d is Map && d['value'] is String) {
    final v = d['value']! as String;
    return d['kind'] == 'markdown' ? v : '```\n$v\n```';
  }
  return null;
}

List<({LspRange range, String text})> _edits(List<Object?> list) => [
  for (final e in list.whereType<Map<Object?, Object?>>())
    (range: lspRange(e['range']), text: e['newText'] as String? ?? ''),
];

List<CompletionChoice> parseCompletion(Object? result) {
  final items = result is List
      ? result
      : (result is Map ? (result['items'] as List? ?? const <Object?>[]) : const <Object?>[]);
  return [
    for (final i in items.whereType<Map<Object?, Object?>>())
      () {
        final label = i['label'] as String? ?? '';
        final te = i['textEdit'];
        final snippet = (i['insertTextFormat'] as num?)?.toInt() == 2;
        final text =
            (te is Map ? te['newText'] as String? : null) ?? i['insertText'] as String? ?? label;
        LspRange? range;
        // insertReplaceSupport: the insert range (VS Code's default for typing).
        if (te is Map) range = lspRange(te['insert'] ?? te['range'] ?? te['replace']);
        final ld = i['labelDetails'];
        final labelDetail = ld is Map
            ? [ld['detail'], ld['description']].whereType<String>().join(' ').trim()
            : null;
        final more = i['additionalTextEdits'];
        return CompletionChoice(
          label: label,
          insert: text,
          detail: i['detail'] as String?,
          kind: (i['kind'] as num?)?.toInt(),
          range: range,
          filter: i['filterText'] as String?,
          sort: i['sortText'] as String?,
          snippet: snippet,
          labelDetail: (labelDetail?.isEmpty ?? true) ? null : labelDetail,
          documentation: _documentation(i['documentation']),
          additional: more is List ? _edits(more) : const [],
          commit: [...?(i['commitCharacters'] as List?)?.whereType<String>()],
          preselect: i['preselect'] == true,
          raw: i,
        );
      }(),
  ];
}

/// Hover contents as plain text with code blocks kept (MarkupContent, MarkedString or a list).
String hoverText(Object? result) {
  if (result is! Map) return '';
  String one(Object? c) {
    if (c is String) return c;
    if (c is Map) {
      final v = c['value'] as String? ?? '';
      final lang = c['language'] as String?;
      return lang == null ? v : '```$lang\n$v\n```';
    }
    if (c is List) return c.map(one).where((s) => s.isNotEmpty).join('\n\n');
    return '';
  }

  return one(result['contents']).trim();
}

/// The first location of a definition answer (Location, Location[] or LocationLink[]).
({String uri, LspPos pos})? firstLocation(Object? result) {
  final list = result is List ? result : (result == null ? const <Object?>[] : <Object?>[result]);
  for (final l in list.whereType<Map<Object?, Object?>>()) {
    if (l['targetUri'] is String) {
      return (
        uri: l['targetUri']! as String,
        pos: lspRange(l['targetSelectionRange'] ?? l['targetRange']).start,
      );
    }
    if (l['uri'] is String) return (uri: l['uri']! as String, pos: lspRange(l['range']).start);
  }
  return null;
}

/// The text edits of a WorkspaceEdit, by URI (`changes` or `documentChanges`).
Map<String, List<({LspRange range, String text})>> workspaceEdits(Object? edit) {
  final out = <String, List<({LspRange range, String text})>>{};
  if (edit is! Map) return out;
  List<({LspRange range, String text})> parse(Object? edits) => [
    for (final e in (edits as List? ?? const []).whereType<Map<Object?, Object?>>())
      (range: lspRange(e['range']), text: e['newText'] as String? ?? ''),
  ];
  final dc = edit['documentChanges'];
  if (dc is List) {
    for (final c in dc.whereType<Map<Object?, Object?>>()) {
      final td = c['textDocument'];
      if (td is Map && td['uri'] is String) {
        out
            .putIfAbsent(td['uri']! as String, () => <({LspRange range, String text})>[])
            .addAll(parse(c['edits']));
      }
    }
  } else if (edit['changes'] is Map) {
    for (final e in (edit['changes']! as Map).entries) {
      out.putIfAbsent(e.key as String, () => []).addAll(parse(e.value));
    }
  }
  return out;
}

/// [p] after [e] is applied (an edit at or after it leaves it where it is).
LspPos _shift(LspPos p, ({LspRange range, String text}) e) {
  final s = e.range.start, en = e.range.end;
  final before = en.line < p.line || (en.line == p.line && en.character <= p.character);
  if (!before) return p;
  final added = e.text.split('\n');
  final removedLines = en.line - s.line;
  if (en.line < p.line) {
    return (line: p.line + added.length - 1 - removedLines, character: p.character);
  }
  // On the edit's last line: the column follows the new text's last line.
  final col = added.length == 1
      ? s.character + added.first.length + (p.character - en.character)
      : added.last.length + (p.character - en.character);
  return (line: s.line + added.length - 1, character: col);
}

/// Applies [edits] as one undoable step and returns where [track] (a position before the edits
/// are applied) ends up; the caret goes there.
LspPos applyEditsTracking(
  CodeLineEditingController c,
  List<({LspRange range, String text})> edits,
  LspPos track,
) {
  var at = track;
  // Bottom up, so each edit's range is still in the original coordinates when compared.
  final sorted = [...edits]
    ..sort((a, b) {
      final l = b.range.start.line.compareTo(a.range.start.line);
      return l != 0 ? l : b.range.start.character.compareTo(a.range.start.character);
    });
  for (final e in sorted) {
    // Edits wholly before the tracked position move it (the item's own edit starts at it, so
    // it does not).
    at = _shift(at, e);
  }
  applyEdits(c, edits);
  return at;
}

/// Applies [edits] (LSP ranges) to [c] as one undoable step: the text is rebuilt once with every
/// edit (last first) and set as one value, the caret kept where it was (clamped).
void applyEdits(CodeLineEditingController c, List<({LspRange range, String text})> edits) {
  if (edits.isEmpty) return;
  final lines = [for (var i = 0; i < c.lineCount; i++) c.codeLines[i].text];
  final sorted = [...edits]
    ..sort((a, b) {
      final l = b.range.start.line.compareTo(a.range.start.line);
      return l != 0 ? l : b.range.start.character.compareTo(a.range.start.character);
    });
  for (final e in sorted) {
    final last = lines.length - 1;
    final sl = e.range.start.line.clamp(0, last), el = e.range.end.line.clamp(0, last);
    final sc = e.range.start.character.clamp(0, lines[sl].length);
    final ec = e.range.end.character.clamp(0, lines[el].length);
    final merged = lines[sl].substring(0, sc) + e.text + lines[el].substring(ec);
    lines.replaceRange(sl, el + 1, merged.split('\n'));
  }
  final sel = c.selection;
  final codeLines = CodeLines.fromText(lines.join('\n'));
  final line = sel.extentIndex.clamp(0, codeLines.length - 1);
  final offset = sel.extentOffset.clamp(0, codeLines[line].length);
  c.runRevocableOp(() {
    c.value = c.value.copyWith(
      codeLines: codeLines,
      selection: CodeLineSelection.collapsed(index: line, offset: offset),
    );
  });
}
