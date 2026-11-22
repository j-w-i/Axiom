// The LSP client's pure parts: the change diff, completion and hover parsing, workspace edits,
// and applying edits to a controller.

import 'package:axiom_desktop/editor/lsp/lsp_editor.dart';
import 'package:axiom_desktop/editor/snippets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:re_editor/re_editor.dart';

/// Applies one contentChanges entry to [lines] the way a server does.
List<String> apply(List<String> lines, Map<String, Object?> change) {
  if (!change.containsKey('range')) return (change['text']! as String).split('\n');
  final r = change['range']! as Map;
  final s = (r['start']! as Map)['line']! as int;
  final e = (r['end']! as Map)['line']! as int;
  final text = change['text']! as String;
  final inserted = text.isEmpty ? <String>[] : text.substring(0, text.length - 1).split('\n');
  return [...lines.sublist(0, s), ...inserted, ...lines.sublist(e)];
}

void main() {
  test('the change diff: typing, a new line, deleting lines, the last line, everything', () {
    final cases = [
      (['a', 'b', 'c'], ['a', 'bX', 'c']),
      (['a', 'b', 'c'], ['a', 'b', 'new', 'c']),
      (['a', 'b', 'c', 'd'], ['a', 'd']),
      (['a', 'b'], ['a', 'bb']), // the last line: full text
      (['x'], ['y', 'z']),
      (['a', 'b', 'c'], ['a', 'b', 'c']),
    ];
    for (final (before, after) in cases) {
      expect(apply(before, contentChange(before, after)), after, reason: '$before → $after');
    }
    expect(contentChange(['a', 'b', 'c'], ['a', 'bX', 'c']).containsKey('range'), isTrue);
  });

  test('completion items: labels, snippets with their tab stops, text edits', () {
    final items = parseCompletion({
      'isIncomplete': false,
      'items': [
        {
          'label': 'norm',
          'kind': 2,
          'detail': 'fn(&self) -> i32',
          'insertTextFormat': 2,
          'insertText': r'norm()$0',
        },
        {
          'label': 'x',
          'textEdit': {
            'range': {
              'start': {'line': 1, 'character': 2},
              'end': {'line': 1, 'character': 3},
            },
            'newText': 'x',
          },
        },
        {'label': 'call', 'insertTextFormat': 2, 'insertText': r'call(${1:arg})'},
      ],
    });
    // Snippets keep their tab stops (M5.5-EDIT-001); their text is what parseSnippet makes.
    expect([for (final i in items) i.insert], [r'norm()$0', 'x', r'call(${1:arg})']);
    expect([for (final i in items) i.snippet], [true, false, true]);
    expect([for (final i in items) parseSnippet(i.insert).text], ['norm()', 'x', 'call(arg)']);
    expect(items[1].range!.start, (line: 1, character: 2));
    expect(
      parseCompletion([
        {'label': 'a'},
      ]).single.label,
      'a',
    );
  });

  test('hover text from MarkupContent, MarkedString and lists', () {
    expect(
      hoverText({
        'contents': {'kind': 'markdown', 'value': '**fn** main()'},
      }),
      '**fn** main()',
    );
    expect(
      hoverText({
        'contents': {'language': 'rust', 'value': 'fn main()'},
      }),
      '```rust\nfn main()\n```',
    );
    expect(
      hoverText({
        'contents': [
          'a',
          {'language': 'ts', 'value': 'b'},
        ],
      }),
      'a\n\n```ts\nb\n```',
    );
  });

  test('definition answers: Location, Location[], LocationLink[]', () {
    final loc = {
      'uri': 'file:///w/a.rs',
      'range': {
        'start': {'line': 3, 'character': 4},
        'end': {'line': 3, 'character': 9},
      },
    };
    expect(firstLocation(loc)!.uri, 'file:///w/a.rs');
    expect(firstLocation([loc])!.pos, (line: 3, character: 4));
    expect(
      firstLocation([
        {
          'targetUri': 'file:///w/b.rs',
          'targetRange': loc['range'],
          'targetSelectionRange': {
            'start': {'line': 5, 'character': 1},
            'end': {'line': 5, 'character': 2},
          },
        },
      ])!.pos,
      (line: 5, character: 1),
    );
    expect(firstLocation(null), isNull);
  });

  test('workspace edits from changes and documentChanges, applied last-first as one undo', () {
    Map<String, Object?> e(int l, int s, int en, String t) => {
      'range': {
        'start': {'line': l, 'character': s},
        'end': {'line': l, 'character': en},
      },
      'newText': t,
    };
    final w = workspaceEdits({
      'documentChanges': [
        {
          'textDocument': {'uri': 'file:///w/a.rs', 'version': 2},
          'edits': [e(0, 4, 9, 'total'), e(1, 0, 5, 'total')],
        },
      ],
    });
    expect(w.keys, ['file:///w/a.rs']);
    final c = CodeLineEditingController.fromText('let value = 1;\nvalue + 1');
    applyEdits(c, w['file:///w/a.rs']!);
    expect(c.text, 'let total = 1;\ntotal + 1');
    c.undo();
    expect(c.text, 'let value = 1;\nvalue + 1');
    expect(
      workspaceEdits({
        'changes': {
          'file:///w/b.rs': [e(0, 0, 1, 'X')],
        },
      }).keys,
      ['file:///w/b.rs'],
    );
  });

  test('completion details: labelDetails, documentation, auto-imports, commit characters', () {
    final item = parseCompletion([
      {
        'label': 'HashMap',
        'labelDetails': {'detail': '<K, V>', 'description': 'std::collections'},
        'documentation': {'kind': 'markdown', 'value': 'A **hash** map.'},
        'additionalTextEdits': [
          {
            'range': {
              'start': {'line': 0, 'character': 0},
              'end': {'line': 0, 'character': 0},
            },
            'newText': 'use std::collections::HashMap;\n',
          },
        ],
        'commitCharacters': ['.', '('],
        'preselect': true,
      },
    ]).single;
    expect(item.labelDetail, '<K, V> std::collections');
    expect(item.documentation, 'A **hash** map.');
    expect(item.additional.single.text, 'use std::collections::HashMap;\n');
    expect(item.commit, ['.', '(']);
    expect(item.preselect, isTrue);
    // A resolve answer fills what was missing.
    final bare = parseCompletion([
      {'label': 'len'},
    ]).single;
    expect(bare.documentation, isNull);
    final done = bare.resolved({
      'detail': 'fn len(&self) -> usize',
      'documentation': 'Returns the length.',
    });
    expect(done.detail, 'fn len(&self) -> usize');
    expect(done.documentation, 'Returns the length.');
  });

  test('an auto-import above the completion: one undo step, the caret after the item', () {
    final c = CodeLineEditingController.fromText('fn main() {\n    let m = Ha\n}');
    c.selection = const CodeLineSelection.collapsed(index: 1, offset: 14);
    final at = applyEditsTracking(
      c,
      [
        (
          range: (start: (line: 1, character: 12), end: (line: 1, character: 14)),
          text: 'HashMap::new()',
        ),
        (
          range: (start: (line: 0, character: 0), end: (line: 0, character: 0)),
          text: 'use std::collections::HashMap;\n\n',
        ),
      ],
      (line: 1, character: 12),
    );
    expect(c.text, 'use std::collections::HashMap;\n\nfn main() {\n    let m = HashMap::new()\n}');
    // The item's start moved down two lines.
    expect(at, (line: 3, character: 12));
    c.undo();
    expect(c.text, 'fn main() {\n    let m = Ha\n}');
  });
}
