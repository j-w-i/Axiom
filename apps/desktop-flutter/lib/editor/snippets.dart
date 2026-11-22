// LSP snippets (M5.5-EDIT-001, ADR-0021 §7): `insertTextFormat: 2` text parsed into the text to
// insert and its tab stops, as VS Code does: `$1`, `${1:default}` (nested), `${1|a,b|}` (the first
// choice), `$0` (the final caret), variables (`$TM_FILENAME`, `${VAR:default}`) as their default or
// nothing, transforms (`${1/regex/format/}`, `${VAR/…/…/}`) as nothing (the text they
// transform is not known when the snippet is inserted), and `\$`, `\}`, `\\` escapes.

/// A tab stop: its number and where it is in the inserted text (offsets; start == end for an empty
/// stop). One number may appear more than once (mirrors): each occurrence is a range.
typedef TabStop = ({int index, int start, int end});

/// The text a snippet inserts and its tab stops, in the order Tab visits them: 1, 2, … then 0 (the
/// final caret; the end of the text when the snippet has none).
typedef ParsedSnippet = ({String text, List<TabStop> stops});

ParsedSnippet parseSnippet(String snippet) {
  final out = StringBuffer();
  final stops = <TabStop>[];
  var i = 0;

  // Parses until [end] (a `}` closing a placeholder) or the end; returns nothing, writes [out].
  void parse({bool inside = false}) {
    while (i < snippet.length) {
      final ch = snippet[i];
      if (ch == '\\' && i + 1 < snippet.length && r'$}\,|'.contains(snippet[i + 1])) {
        out.write(snippet[i + 1]);
        i += 2;
        continue;
      }
      if (inside && ch == '}') return;
      if (ch != r'$') {
        out.write(ch);
        i++;
        continue;
      }
      // `$1`, `$VAR`
      final simple = RegExp(r'\$(\d+|[A-Za-z_][A-Za-z0-9_]*)').matchAsPrefix(snippet, i);
      if (simple != null) {
        final name = simple.group(1)!;
        final n = int.tryParse(name);
        if (n != null) stops.add((index: n, start: out.length, end: out.length));
        i = simple.end;
        continue;
      }
      // `${…}`
      if (i + 1 < snippet.length && snippet[i + 1] == '{') {
        final head = RegExp(r'\$\{(\d+|[A-Za-z_][A-Za-z0-9_]*)').matchAsPrefix(snippet, i);
        if (head == null) {
          out.write(ch);
          i++;
          continue;
        }
        final name = head.group(1)!;
        final n = int.tryParse(name);
        i = head.end;
        final start = out.length;
        var transform = false;
        if (i < snippet.length && snippet[i] == '/') {
          // A transform: `/regex/format/options` up to the `}`; skipped (audit finding 20).
          transform = true;
          // The format may hold `${1:/upcase}` and the like: their braces are nested.
          var slashes = 0, depth = 0;
          while (i < snippet.length && !(slashes >= 3 && depth == 0 && snippet[i] == '}')) {
            final c = snippet[i];
            if (c == '\\' && i + 1 < snippet.length) {
              i += 2;
              continue;
            }
            if (c == r'$' && i + 1 < snippet.length && snippet[i + 1] == '{') {
              depth++;
              i += 2;
              continue;
            }
            if (c == '}' && depth > 0) {
              depth--;
            } else if (c == '/' && depth == 0) {
              slashes++;
            }
            i++;
          }
        } else if (i < snippet.length && snippet[i] == ':') {
          i++;
          parse(inside: true);
        } else if (i < snippet.length && snippet[i] == '|') {
          // Choices: the first one is inserted.
          final close = snippet.indexOf('|}', i + 1);
          final body = close < 0 ? '' : snippet.substring(i + 1, close);
          final first = _splitChoices(body).firstOrNull ?? '';
          out.write(first);
          i = close < 0 ? snippet.length : close + 1;
        }
        if (i < snippet.length && snippet[i] == '}') i++;
        if (n != null && !transform) stops.add((index: n, start: start, end: out.length));
        continue;
      }
      out.write(ch);
      i++;
    }
  }

  parse();
  final text = out.toString();
  final numbered = stops.where((s) => s.index > 0).toList()
    ..sort((a, b) => a.index != b.index ? a.index.compareTo(b.index) : a.start.compareTo(b.start));
  final finals = stops.where((s) => s.index == 0).toList();
  return (
    text: text,
    stops: [
      ...numbered,
      if (finals.isNotEmpty) ...finals else (index: 0, start: text.length, end: text.length),
    ],
  );
}

List<String> _splitChoices(String body) {
  final out = <String>[];
  final b = StringBuffer();
  for (var i = 0; i < body.length; i++) {
    final c = body[i];
    if (c == '\\' && i + 1 < body.length) {
      b.write(body[i + 1]);
      i++;
    } else if (c == ',') {
      out.add(b.toString());
      b.clear();
    } else {
      b.write(c);
    }
  }
  out.add(b.toString());
  return out;
}

/// The (line, column) of [offset] in [text] inserted at [line], [column].
({int line, int column}) positionIn(String text, int offset, int line, int column) {
  final before = text.substring(0, offset.clamp(0, text.length));
  final nl = before.lastIndexOf('\n');
  if (nl < 0) return (line: line, column: column + before.length);
  return (line: line + '\n'.allMatches(before).length, column: before.length - nl - 1);
}

/// Words of a document worth completing (word completion where no server serves the language):
/// identifiers of three or more characters, most frequent first, without [current].
List<String> documentWords(Iterable<String> lines, {String current = ''}) {
  final counts = <String, int>{};
  final word = RegExp(r'[A-Za-z_$][A-Za-z0-9_$]{2,}');
  for (final l in lines) {
    for (final m in word.allMatches(l)) {
      final w = m[0]!;
      if (w != current) counts[w] = (counts[w] ?? 0) + 1;
    }
  }
  final words = counts.keys.toList()
    ..sort((a, b) {
      final c = counts[b]!.compareTo(counts[a]!);
      return c != 0 ? c : a.compareTo(b);
    });
  return words;
}
