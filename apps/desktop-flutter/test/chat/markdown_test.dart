// Markdown in Chat: the pieces a streamed answer is split into (each parsed and built once), and
// the rendering of the constructs agents use.

import 'package:axiom_desktop/chat/markdown_view.dart';
import 'package:axiom_desktop/main.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('pieces split at blank lines, never inside fenced code or a list item', () {
    expect(markdownPieces('# Title\n\nOne.\nTwo.\n\n- a\n- b\n'), [
      '# Title',
      'One.\nTwo.',
      '- a\n- b',
    ]);
    // A blank line inside a fence stays in the fence.
    expect(markdownPieces('Code:\n\n```rust\nfn a() {}\n\nfn b() {}\n```\n\nAfter.'), [
      'Code:',
      '```rust\nfn a() {}\n\nfn b() {}\n```',
      'After.',
    ]);
    // A list item's indented continuation after a blank line stays with the item.
    expect(markdownPieces('1. first\n\n   more of it\n2. second'), [
      '1. first\n\n   more of it\n2. second',
    ]);
    // An unclosed fence (still streaming) takes the rest.
    expect(markdownPieces('Text.\n\n```\nlet a;\n\nlet b;'), ['Text.', '```\nlet a;\n\nlet b;']);
  });

  test('a growing answer keeps its earlier pieces', () {
    const head = 'Intro paragraph.\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n';
    final a = markdownPieces('${head}Partial sen');
    final b = markdownPieces('${head}Partial sentence, longer now.');
    expect(a.take(2), b.take(2));
    expect(a.last, 'Partial sen');
  });

  testWidgets('an image is its alt text (never loaded); raw HTML is text', (tester) async {
    await tester.pumpWidget(
      const AxiomApp(
        home: SingleChildScrollView(
          child: MarkdownView('See ![a chart](https://example.com/x.png) and <b>bold</b>.'),
        ),
      ),
    );
    expect(find.textContaining('[image: a chart]', findRichText: true), findsOneWidget);
    expect(find.textContaining('<b>bold</b>', findRichText: true), findsOneWidget);
  });

  testWidgets('renders headings, lists, a table, inline code and a code block', (tester) async {
    await tester.pumpWidget(
      const AxiomApp(
        home: SingleChildScrollView(
          child: MarkdownView(
            '## Plan\n\n1. read `main.rs`\n2. fix it\n\n| k | v |\n|---|---|\n| x | 1 |\n\n'
            '```rust\nfn main() {}\n```\n',
          ),
        ),
      ),
    );
    expect(find.text('Plan'), findsOneWidget);
    expect(find.text('1.'), findsOneWidget);
    expect(find.text('2.'), findsOneWidget);
    expect(find.byType(Table), findsOneWidget);
    expect(find.byType(CodeBlock), findsOneWidget);
    expect(tester.widget<CodeBlock>(find.byType(CodeBlock)).language, 'rust');
  });
}
