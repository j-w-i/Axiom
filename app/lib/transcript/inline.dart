import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';

import '../ui/tokens.dart';
import 'markdown.dart';

// Code first: nothing inside it is markup. Links show their words and never load anything.
final _re = RegExp(
  r'(`+)(.+?)\1(?!`)|\*\*(.+?)\*\*|__(.+?)__|(?<![\w*])\*([^*\s][^*]*?)\*(?!\w)|'
  r'(?<!\w)_([^_\s][^_]*?)_(?!\w)|~~(.+?)~~|!?\[([^\]]*)\]\(([^)\s]+)[^)]*\)|(https?://[^\s<>)\]]+)',
);

/// [onLink] is asked, never bypassed: a person sees the address before anything opens it.
TextSpan inlineSpan(
  String source,
  TextStyle base,
  TextStyle code,
  AxColors c, {
  void Function(String url)? onLink,
  List<GestureRecognizer>? recognizers,
}) {
  TextSpan link(String words, String url) {
    GestureRecognizer? r;
    if (onLink != null && recognizers != null) {
      r = TapGestureRecognizer()..onTap = () => onLink(url);
      recognizers.add(r);
    }
    return TextSpan(
      text: words.isEmpty ? url : words,
      recognizer: r,
      mouseCursor: r == null ? MouseCursor.defer : SystemMouseCursors.click,
      style: TextStyle(
        decoration: TextDecoration.underline,
        decorationColor: c.ink4,
        color: c.liveText,
      ),
    );
  }

  List<InlineSpan> walk(String s, int depth) {
    final out = <InlineSpan>[];
    var p = 0;
    for (final m in _re.allMatches(s)) {
      if (m.start > p) out.add(TextSpan(text: s.substring(p, m.start)));
      p = m.end;
      TextSpan styled(String t, TextStyle st) => depth > 3
          ? TextSpan(text: t, style: st)
          : TextSpan(style: st, children: walk(t, depth + 1));
      if (m[2] case final t?) {
        out.add(TextSpan(text: t, style: code));
      } else if (m[3] ?? m[4] case final t?) {
        out.add(styled(t, const TextStyle(fontVariations: [FontVariation('wght', 640)])));
      } else if (m[5] ?? m[6] case final t?) {
        out.add(styled(t, const TextStyle(fontStyle: FontStyle.italic)));
      } else if (m[7] case final t?) {
        out.add(styled(t, const TextStyle(decoration: TextDecoration.lineThrough)));
      } else if (m[9] case final url?) {
        out.add(link(m[8]!, url));
      } else if (m[10] case final url?) {
        out.add(link(url, url));
      }
    }
    if (p < s.length) out.add(TextSpan(text: s.substring(p)));
    return out;
  }

  return TextSpan(style: base, children: walk(visible(source), 0));
}
