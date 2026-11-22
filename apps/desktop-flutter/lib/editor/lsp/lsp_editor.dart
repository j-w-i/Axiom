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

/// The LSP language id of a workspace path, or null when no server language applies.
String? lspLanguageId(String path) {
  final name = path.split('/').last.toLowerCase();
  final dot = name.lastIndexOf('.');
  if (dot < 0) return null;
  return switch (name.substring(dot + 1)) {
    'rs' => 'rust',
    'ts' || 'mts' || 'cts' => 'typescript',
    'tsx' => 'typescriptreact',
    'js' || 'mjs' || 'cjs' => 'javascript',
    'jsx' => 'javascriptreact',
    'json' => 'json',
    'py' => 'python',
    'toml' => 'toml',
    'md' => 'markdown',
    'sh' || 'bash' => 'shellscript',
    'go' => 'go',
    'c' || 'h' => 'c',
    'cc' || 'cpp' || 'hpp' => 'cpp',
    'java' => 'java',
    'dart' => 'dart',
    'css' => 'css',
    'html' => 'html',
    'yaml' || 'yml' => 'yaml',
    _ => null,
  };
}

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

  String? _serverFor(String path) => lsp.serverFor(lspLanguageId(path))?.serverId;

  void _onDocs() {
    final now = {for (final d in docs.docs) d.path: d};
    for (final path in [..._open.keys]) {
      if (!now.containsKey(path)) _close(path);
    }
    for (final d in now.values) {
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
    final server = _serverFor(d.path);
    if (server == null) return;
    final s = _Synced(d, server, lspLanguageId(d.path)!);
    _open[d.path] = s;
    unawaited(
      _notify(server, 'textDocument/didOpen', {
        'textDocument': {
          'uri': lsp.uriOf(d.path),
          'languageId': s.languageId,
          'version': s.version,
          'text': d.text,
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
  });
  final String label;
  final String insert;
  final String? detail;
  final int? kind;

  /// The server's edit range, when it gave one.
  final LspRange? range;
  final String? filter;
  final String? sort;
}

/// A snippet's text without its tab stops (`${1:x}` → `x`, `$0` → nothing).
String stripSnippet(String s) => s
    .replaceAllMapped(RegExp(r'\$\{\d+:([^}]*)\}'), (m) => m[1]!)
    .replaceAll(RegExp(r'\$\{\d+\}'), '')
    .replaceAll(RegExp(r'\$\d+'), '');

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
        var text =
            (te is Map ? te['newText'] as String? : null) ?? i['insertText'] as String? ?? label;
        if (snippet) text = stripSnippet(text);
        LspRange? range;
        if (te is Map) range = lspRange(te['range'] ?? te['replace'] ?? te['insert']);
        return CompletionChoice(
          label: label,
          insert: text,
          detail: i['detail'] as String?,
          kind: (i['kind'] as num?)?.toInt(),
          range: range,
          filter: i['filterText'] as String?,
          sort: i['sortText'] as String?,
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
