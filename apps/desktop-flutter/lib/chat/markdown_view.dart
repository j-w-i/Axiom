// Markdown in Chat (ADR-0020 §3): the `markdown` package (BSD-3) parses; Axiom's widgets render.
// Headings, paragraphs, lists, quotes, rules, tables, inline code, emphasis, links, and fenced
// code coloured with the editor's palette (re_highlight) with a Copy button.

import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:markdown/markdown.dart' as md;

import 're_highlight_bridge.dart';

import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/surfaces.dart';
import '../ui/theme.dart';
import '../ui/tokens.dart';

class MarkdownView extends StatefulWidget {
  const MarkdownView(this.source, {super.key, this.dim = false, this.onOpenLink});
  final String source;

  /// A plan or a thought (an external agent's): quieter ink.
  final bool dim;

  /// An http(s) link was clicked (other links are shown, not followed).
  final ValueChanged<String>? onOpenLink;

  @override
  State<MarkdownView> createState() => _MarkdownViewState();
}

/// The link recognizers of one block of the source.
class _Links {
  _Links(this.onOpen);
  final ValueChanged<String>? onOpen;
  final list = <TapGestureRecognizer>[];

  TapGestureRecognizer? forHref(String? href) {
    final open = onOpen;
    if (open == null ||
        href == null ||
        !RegExp(r'^https?://', caseSensitive: false).hasMatch(href)) {
      return null;
    }
    final r = TapGestureRecognizer()..onTap = () => open(href);
    list.add(r);
    return r;
  }

  void dispose() {
    for (final r in list) {
      r.dispose();
    }
    list.clear();
  }
}

/// [source] in top-level pieces that parse on their own: split at blank lines outside fenced
/// code, unless the next line is indented (a list item's continuation). A streamed answer only
/// grows at its end, so every piece but the last stays the same from one delta to the next.
List<String> markdownPieces(String source) {
  final lines = source.split('\n');
  final out = <String>[];
  final cur = <String>[];
  String? fence;
  final fenceRe = RegExp(r'^ {0,3}(`{3,}|~{3,})');
  void flush() {
    while (cur.isNotEmpty && cur.last.trim().isEmpty) {
      cur.removeLast();
    }
    if (cur.isNotEmpty) out.add(cur.join('\n'));
    cur.clear();
  }

  for (var i = 0; i < lines.length; i++) {
    final l = lines[i];
    final f = fenceRe.firstMatch(l);
    if (fence == null && f != null) {
      fence = f.group(1)![0];
    } else if (fence != null && f != null && f.group(1)![0] == fence) {
      fence = null;
    }
    if (fence == null && l.trim().isEmpty && cur.isNotEmpty) {
      final next = lines.skip(i + 1).firstWhere((x) => x.trim().isNotEmpty, orElse: () => '');
      if (next.isNotEmpty && !next.startsWith(' ') && !next.startsWith('\t')) {
        flush();
        continue;
      }
    }
    cur.add(l);
  }
  flush();
  return out;
}

/// One piece's widgets (built once, reused while the piece's text is the same).
class _Piece {
  _Piece(String source, TextStyle base, ValueChanged<String>? onOpen) : links = _Links(onOpen) {
    final doc = md.Document(extensionSet: md.ExtensionSet.gitHubFlavored, encodeHtml: false);
    widgets = [for (final n in doc.parse(source)) _Block(n, base: base, links: links)];
  }
  final _Links links;
  late final List<Widget> widgets;
}

class _MarkdownViewState extends State<MarkdownView> {
  var _pieces = <String, _Piece>{};
  TextStyle? _base;
  ValueChanged<String>? _onOpen;

  void _clear() {
    for (final p in _pieces.values) {
      p.links.dispose();
    }
    _pieces = {};
  }

  @override
  void dispose() {
    _clear();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final base = t.type.body.copyWith(
      fontSize: 14,
      height: 1.55,
      color: widget.dim ? t.colors.ink2 : t.colors.ink,
    );
    if (base != _base || widget.onOpenLink != _onOpen) {
      _clear();
      _base = base;
      _onOpen = widget.onOpenLink;
    }
    final next = <String, _Piece>{};
    final children = <Widget>[];
    for (final src in markdownPieces(widget.source)) {
      final p = next[src] ?? _pieces.remove(src) ?? _Piece(src, base, widget.onOpenLink);
      next[src] = p;
      children.addAll(p.widgets);
    }
    for (final p in _pieces.values) {
      p.links.dispose();
    }
    _pieces = next;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: children);
  }
}

class _Block extends StatelessWidget {
  const _Block(this.node, {required this.base, required this.links});
  final md.Node node;
  final TextStyle base;
  final _Links links;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final n = node;
    if (n is md.Text) return Text(n.text, style: base);
    if (n is! md.Element) return const SizedBox.shrink();
    Widget gap(Widget w, [double bottom = 10]) => Padding(
      padding: EdgeInsets.only(bottom: bottom),
      child: w,
    );
    switch (n.tag) {
      case 'h1' || 'h2' || 'h3' || 'h4' || 'h5' || 'h6':
        final size = switch (n.tag) {
          'h1' => 20.0,
          'h2' => 17.0,
          'h3' => 15.0,
          _ => 14.0,
        };
        return gap(
          Semantics(
            header: true,
            child: Text.rich(
              _inline(
                context,
                links,
                n.children,
                base.copyWith(fontSize: size, fontWeight: FontWeight.w600),
              ),
            ),
          ),
          8,
        );
      case 'p':
        return gap(Text.rich(_inline(context, links, n.children, base)));
      case 'pre':
        final code = n.children?.firstOrNull;
        final lang = code is md.Element
            ? (code.attributes['class'] ?? '').replaceFirst('language-', '')
            : '';
        final text = (code is md.Element ? code.textContent : n.textContent).replaceAll(
          RegExp(r'\n$'),
          '',
        );
        return gap(CodeBlock(code: text, language: lang));
      case 'blockquote':
        return gap(
          DecoratedBox(
            decoration: BoxDecoration(
              border: Border(left: BorderSide(color: c.hairline, width: 3)),
            ),
            child: Padding(
              padding: const EdgeInsets.only(left: 12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  for (final ch in n.children ?? const <md.Node>[])
                    _Block(
                      ch,
                      base: base.copyWith(color: c.ink2),
                      links: links,
                    ),
                ],
              ),
            ),
          ),
        );
      case 'hr':
        return gap(const Hairline());
      case 'ul' || 'ol':
        final ordered = n.tag == 'ol';
        final start = int.tryParse(n.attributes['start'] ?? '1') ?? 1;
        final items = [
          for (final ch in n.children ?? const <md.Node>[])
            if (ch is md.Element && ch.tag == 'li') ch,
        ];
        return gap(
          Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              for (final (i, li) in items.indexed)
                Padding(
                  padding: const EdgeInsets.only(bottom: 4),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      SizedBox(
                        width: 22,
                        child: Text(
                          ordered ? '${start + i}.' : '•',
                          style: base.copyWith(color: c.ink2),
                        ),
                      ),
                      Expanded(
                        child: _ListItem(li, base: base, links: links),
                      ),
                    ],
                  ),
                ),
            ],
          ),
        );
      case 'table':
        return gap(_Table(n, base: base, links: links));
      default:
        return gap(Text.rich(_inline(context, links, n.children, base)));
    }
  }
}

class _ListItem extends StatelessWidget {
  const _ListItem(this.li, {required this.base, required this.links});
  final md.Element li;
  final TextStyle base;
  final _Links links;

  @override
  Widget build(BuildContext context) {
    final kids = li.children ?? const <md.Node>[];
    final blocks = kids.any(
      (k) => k is md.Element && const {'p', 'ul', 'ol', 'pre', 'blockquote'}.contains(k.tag),
    );
    if (!blocks) return Text.rich(_inline(context, links, kids, base));
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (final k in kids)
          if (k is md.Element && const {'p', 'ul', 'ol', 'pre', 'blockquote'}.contains(k.tag))
            _Block(k, base: base, links: links)
          else
            Text.rich(_inline(context, links, [k], base)),
      ],
    );
  }
}

class _Table extends StatelessWidget {
  const _Table(this.table, {required this.base, required this.links});
  final md.Element table;
  final TextStyle base;
  final _Links links;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    final rows = <md.Element>[];
    void collect(md.Node n) {
      if (n is md.Element) {
        if (n.tag == 'tr') {
          rows.add(n);
        } else {
          for (final ch in n.children ?? const <md.Node>[]) {
            collect(ch);
          }
        }
      }
    }

    collect(table);
    return Table(
      defaultColumnWidth: const IntrinsicColumnWidth(),
      border: TableBorder(horizontalInside: BorderSide(color: c.hairline)),
      children: [
        for (final (i, r) in rows.indexed)
          TableRow(
            children: [
              for (final cell in (r.children ?? const <md.Node>[]).whereType<md.Element>())
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                  child: Text.rich(
                    _inline(
                      context,
                      links,
                      cell.children,
                      i == 0 ? base.copyWith(fontWeight: FontWeight.w600) : base,
                    ),
                  ),
                ),
            ],
          ),
      ],
    );
  }
}

/// Inline nodes as spans: emphasis, strong, code, links (shown with their target).
TextSpan _inline(BuildContext context, _Links links, List<md.Node>? nodes, TextStyle style) {
  final t = context.axiom;
  final c = t.colors;
  final out = <InlineSpan>[];
  void walk(md.Node n, TextStyle s, [TapGestureRecognizer? link]) {
    if (n is md.Text) {
      out.add(TextSpan(text: n.text, style: s, recognizer: link));
    } else if (n is md.Element) {
      final next = switch (n.tag) {
        'strong' => s.copyWith(fontWeight: FontWeight.w600),
        'em' => s.copyWith(fontStyle: FontStyle.italic),
        'del' => s.copyWith(decoration: TextDecoration.lineThrough),
        'a' => s.copyWith(
          color: c.infoText,
          decoration: TextDecoration.underline,
          decorationColor: c.infoText,
        ),
        _ => s,
      };
      if (n.tag == 'code') {
        out.add(
          TextSpan(
            text: n.textContent,
            style: s.copyWith(
              fontFamily: AxiomType.mono,
              fontSize: (s.fontSize ?? 14) * 0.9,
              backgroundColor: c.recessed,
            ),
          ),
        );
        return;
      }
      if (n.tag == 'img') {
        // Remote images are not loaded: their alt text stands in, as in the Tauri shell.
        final alt = n.attributes['alt'] ?? '';
        out.add(
          TextSpan(
            text: '[image: ${alt.isEmpty ? 'no description' : alt}]',
            style: s.copyWith(color: c.ink2),
          ),
        );
        return;
      }
      if (n.tag == 'br') {
        out.add(TextSpan(text: '\n', style: s));
        return;
      }
      final l = n.tag == 'a' ? links.forHref(n.attributes['href']) : link;
      for (final ch in n.children ?? const <md.Node>[]) {
        walk(ch, next, l);
      }
    }
  }

  for (final n in nodes ?? const <md.Node>[]) {
    walk(n, style);
  }
  return TextSpan(children: out, style: style);
}

/// Fenced code: coloured with the editor's palette, with a Copy button.
class CodeBlock extends StatelessWidget {
  const CodeBlock({super.key, required this.code, this.language = ''});
  final String code;
  final String language;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final style = t.type.code.copyWith(fontSize: 12.5, color: c.syntax.text);
    return Well(
      radius: Metrics.radiusRaised,
      child: Stack(
        children: [
          SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            padding: const EdgeInsets.fromLTRB(12, 10, 44, 10),
            child: Text.rich(highlightCode(code, language, style, c.syntax)),
          ),
          Positioned(
            right: 4,
            top: 4,
            child: AxIconButton(
              icon: AxiomIcons.copy,
              label: 'Copy the code',
              size: 24,
              onPressed: () => Clipboard.setData(ClipboardData(text: code)),
            ),
          ),
        ],
      ),
    );
  }
}
