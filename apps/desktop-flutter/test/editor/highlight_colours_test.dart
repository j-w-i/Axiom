// Every language's code is highlighted in several colours, not one or two (the owner, 2026-11-22):
// each fixture of tests/fixtures/languages, highlighted by its grammar, the editor's theme and
// the structural pass the editor applies (calls and members), uses at least [minimum] colours.

import 'dart:io';

import 'package:axiom_desktop/editor/code_view.dart';
import 'package:axiom_desktop/editor/languages.dart';
import 'package:axiom_desktop/editor/lsp/underline.dart';
import 'package:axiom_desktop/state/documents.dart';
import 'package:axiom_desktop/ui/tokens.dart';
import 'package:flutter/painting.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:re_highlight/re_highlight.dart';

/// The highlighted text as lines of coloured pieces, scopes resolved as re_editor does (the whole
/// name, then without its first part).
class _Lines implements HighlightRenderer {
  _Lines(this.theme, this.text);
  final Map<String, TextStyle> theme;
  final Color text;
  final lines = <List<TextSpan>>[[]];
  final _scopes = <String>[];

  Color _colour() {
    for (var i = _scopes.length - 1; i >= 0; i--) {
      var name = _scopes[i];
      while (true) {
        final c = theme[name]?.color;
        if (c != null) return c;
        final dot = name.indexOf('.');
        if (dot < 0) break;
        name = name.substring(dot + 1);
      }
    }
    return text;
  }

  @override
  void addText(String t) {
    final parts = t.split('\n');
    for (var i = 0; i < parts.length; i++) {
      if (i > 0) lines.add([]);
      if (parts[i].isNotEmpty) {
        lines.last.add(
          TextSpan(
            text: parts[i],
            style: TextStyle(color: _colour()),
          ),
        );
      }
    }
  }

  @override
  void openNode(DataNode node) => _scopes.add(node.scope ?? '');

  @override
  void closeNode(DataNode node) => _scopes.removeLast();
}

Set<Color> _colours(InlineSpan s) {
  final out = <Color>{};
  void walk(InlineSpan s, Color? inherited) {
    if (s is! TextSpan) return;
    final c = s.style?.color ?? inherited;
    if ((s.text?.trim().isNotEmpty ?? false) && c != null) out.add(c);
    for (final x in s.children ?? const <InlineSpan>[]) {
      walk(x, c);
    }
  }

  walk(s, null);
  return out;
}

void main() {
  test('calls and members of plain text are coloured; keywords, strings, comments are not', () {
    const plain = Color(0xFF000001), kw = Color(0xFF000002), str = Color(0xFF000003);
    const fn = Color(0xFF000004), v = Color(0xFF000005);
    final span = TextSpan(
      style: const TextStyle(color: plain),
      children: const [
        TextSpan(
          text: 'if',
          style: TextStyle(color: kw),
        ),
        TextSpan(text: ' (list.count(x)) '),
        TextSpan(
          text: '"call()"',
          style: TextStyle(color: str),
        ),
      ],
    );
    final out = applyStructure(span, plain, fn, v);
    final pieces = <(String, Color?)>[];
    void walk(InlineSpan s, Color? inherited) {
      if (s is! TextSpan) return;
      final c = s.style?.color ?? inherited;
      if (s.text != null && s.text!.isNotEmpty) pieces.add((s.text!, c));
      for (final x in s.children ?? const <InlineSpan>[]) {
        walk(x, c);
      }
    }

    walk(out, plain);
    final line = pieces.map((p) => p.$1).join();
    final perChar = [
      for (final p in pieces)
        for (var i = 0; i < p.$1.length; i++) p.$2,
    ];
    Color? colourOf(String word) => perChar[line.indexOf(word)];
    expect(line, 'if (list.count(x)) "call()"');
    expect(colourOf('if'), kw);
    expect(colourOf('count'), fn);
    expect(colourOf('list'), plain);
    expect(colourOf('call'), str);
  });

  for (final (name, colours) in [('dark', AxiomColors.dark), ('light', AxiomColors.light)]) {
    test('every fixture is highlighted in several colours ($name)', () {
      final y = colours.syntax;
      final theme = syntaxTheme(y);
      final reg = Languages.instance;
      final root = Directory('../../tests/fixtures/languages');
      final counts = <String, int>{};
      for (final f in root.listSync(recursive: true).whereType<File>()) {
        final rel = f.path.substring(root.path.length + 1);
        if (rel == 'README.md') continue;
        final text = f.readAsStringSync();
        final lang = reg.detect(rel, firstLine: text.split('\n').first);
        if (lang.hljs == null) continue;
        final hl = Highlight()..registerLanguage(lang.hljs!, reg.mode(lang)!);
        final r = hl.highlight(code: text, language: lang.hljs!);
        final lines = _Lines(theme, y.text);
        r.render(lines);
        final seen = <Color>{};
        for (final line in lines.lines) {
          var span = TextSpan(
            children: line,
            style: TextStyle(color: y.text),
          );
          if (structuredLanguages.contains(lang.hljs)) {
            span = applyStructure(span, y.text, y.function, y.variable);
          }
          seen.addAll(_colours(span));
        }
        counts[rel] = seen.length;
      }
      // ignore: avoid_print
      print([for (final e in counts.entries) '${e.key}: ${e.value}'].join('\n'));
      final flat = [
        for (final e in counts.entries)
          if (e.value < minimum(e.key)) '${e.key}: ${e.value} < ${minimum(e.key)}',
      ];
      expect(flat, isEmpty);
    });
  }
}

/// Data formats, build files and prose have fewer roles than code.
int minimum(String path) {
  final name = path.split('/').last;
  final ext = name.contains('.') ? name.split('.').last : name;
  return const {
        'json',
        'md',
        'txt',
        'ini',
        'toml',
        'yaml',
        'yml',
        'csv',
        'mod',
        'Dockerfile',
        'kts',
        'css',
        'csproj',
        'xml',
      }.contains(ext)
      ? 2
      : 5;
}
