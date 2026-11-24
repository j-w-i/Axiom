import 'package:flutter/widgets.dart';

import '../ui/tokens.dart';

// A single-pass scanner good enough for short excerpts in any brace language. Full grammars arrive
// with the tree-sitter work (M4); this never throws and never drops a character (tested).
final _re = RegExp(
  r'''(//.*|#(?=\s).*)|("(?:\\.|[^"\\])*"?|'(?:\\.|[^'\\])*'?|`(?:\\.|[^`\\])*`?)|'''
  r'\b(async|await|break|case|catch|class|const|continue|def|else|enum|export|extends|final|fn|for|'
  r'from|func|if|impl|import|in|interface|let|match|mut|new|null|pub|return|self|static|struct|'
  r'switch|this|throw|true|false|try|type|use|using|var|while|yield)\b|'
  r'\b(\d[\d_.xA-Fa-f]*)\b|\b([A-Z]\w*)\b|\b([a-z_]\w*)(?=\()',
);

TextSpan highlight(String line, TextStyle base, AxColors c) {
  final out = <TextSpan>[];
  var p = 0;
  for (final m in _re.allMatches(line)) {
    if (m.start > p) out.add(TextSpan(text: line.substring(p, m.start)));
    out.add(
      TextSpan(
        text: m[0],
        style: TextStyle(
          color: m[1] != null
              ? c.codeComment
              : m[2] != null
              ? c.codeString
              : m[3] != null
              ? c.codeKeyword
              : m[4] != null
              ? c.codeNumber
              : m[5] != null
              ? c.codeType
              : c.codeCall,
        ),
      ),
    );
    p = m.end;
  }
  if (p < line.length) out.add(TextSpan(text: line.substring(p)));
  return TextSpan(style: base.ink(c.codePlain), children: out);
}
