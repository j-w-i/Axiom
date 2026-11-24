/// Inline Markdown, by a hand-written scanner: linear in practice, bounded in the worst case, and
/// never in a position to load anything.
library;

import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';

import '../ui/tokens.dart';
import 'markdown.dart';
import 'safe_text.dart';

const _code = 1, _strong = 2, _em = 4, _del = 8;

/// A stretch of text with one style. [url] is set inside a link.
class InlineRun {
  InlineRun(this.text, this.style, [this.url]);
  final String text;
  int style;
  String? url;
  bool get code => style & _code != 0;
  bool get strong => style & _strong != 0;
  bool get em => style & _em != 0;
  bool get del => style & _del != 0;
}

class _Delimiter {
  _Delimiter(this.char, this.count, this.at, {required this.opens, required this.closes});
  final String char;
  int count;

  /// Index in the runs of its own literal text.
  final int at;
  final bool opens, closes;
}

const _maxOpen = 64, _maxUrl = 2000;
const _entities = {'lt': '<', 'gt': '>', 'amp': '&', 'quot': '"', 'apos': "'", 'nbsp': ' '};
bool _space(String? c) => c == null || c.trim().isEmpty;
final _word = RegExp(r'[\p{L}\p{N}_]', unicode: true);
bool _punct(String? c) => c != null && !_space(c) && !_word.hasMatch(c);

/// The runs of one block's text.
List<InlineRun> inlineRuns(String source) {
  final s = flow(visible(source));
  final out = <InlineRun>[];
  final lit = StringBuffer();
  final stack = <_Delimiter>[];
  final brackets = <(int, int)>[]; // index in out, position in s
  final noCloser = <int>{};

  void flush() {
    if (lit.isEmpty) return;
    out.add(InlineRun(lit.toString(), 0));
    lit.clear();
  }

  void mark(int from, int bit) {
    for (var k = from; k < out.length; k++) {
      out[k].style |= bit;
    }
  }

  var i = 0;
  while (i < s.length) {
    final c = s[i];
    switch (c) {
      case r'\'
          when i + 1 < s.length &&
              r'!"#$%&'
                      "'"
                      r'()*+,-./:;<=>?@[\]^_`{|}~'
                  .contains(s[i + 1]):
        lit.write(s[i + 1]);
        i += 2;
      case '`':
        var n = i;
        while (n < s.length && s[n] == '`') {
          n++;
        }
        final len = n - i;
        var close = -1;
        if (!noCloser.contains(len)) {
          var k = n;
          while (k < s.length) {
            k = s.indexOf('`', k);
            if (k < 0) break;
            var e = k;
            while (e < s.length && s[e] == '`') {
              e++;
            }
            if (e - k == len) {
              close = k;
              break;
            }
            k = e;
          }
          if (close < 0) noCloser.add(len);
        }
        if (close < 0) {
          lit.write(s.substring(i, n));
          i = n;
        } else {
          flush();
          var body = s.substring(n, close);
          if (body.length > 2 &&
              body.startsWith(' ') &&
              body.endsWith(' ') &&
              body.trim().isNotEmpty) {
            body = body.substring(1, body.length - 1);
          }
          out.add(InlineRun(literal(body), _code));
          i = close + len;
        }
      case '*' || '_' || '~':
        var n = i;
        while (n < s.length && s[n] == c) {
          n++;
        }
        final len = n - i;
        final before = i == 0 ? null : s[i - 1], after = n < s.length ? s[n] : null;
        if (c == '~' && len != 2) {
          lit.write(s.substring(i, n));
          i = n;
          break;
        }
        final left = !_space(after) && (!_punct(after) || _space(before) || _punct(before));
        final right = !_space(before) && (!_punct(before) || _space(after) || _punct(after));
        var opens = left, closes = right;
        if (c == '_') {
          opens = left && (!right || _punct(before));
          closes = right && (!left || _punct(after));
        }
        flush();
        var rest = len;
        if (closes) {
          for (var k = stack.length - 1; k >= 0 && rest > 0; k--) {
            final o = stack[k];
            if (o.char != c || !o.opens) continue;
            // CommonMark's rule of three.
            if ((o.closes || opens) &&
                (o.count + len) % 3 == 0 &&
                (o.count % 3 != 0 || len % 3 != 0)) {
              continue;
            }
            while (rest > 0 && o.count > 0) {
              final two = rest >= 2 && o.count >= 2;
              if (c == '~' && !two) break;
              mark(
                o.at + 1,
                c == '~'
                    ? _del
                    : two
                    ? _strong
                    : _em,
              );
              rest -= two ? 2 : 1;
              o.count -= two ? 2 : 1;
            }
            stack.length = o.count > 0 ? k + 1 : k;
            out[o.at] = InlineRun(c * o.count, out[o.at].style);
          }
        }
        if (rest > 0) {
          out.add(InlineRun(c * rest, 0));
          if (opens && stack.length < _maxOpen) {
            stack.add(_Delimiter(c, rest, out.length - 1, opens: opens, closes: closes));
          }
        }
        i = n;
      case '!' when i + 1 < s.length && s[i + 1] == '[':
        i++; // an image is shown as a link with its words
      case '[':
        flush();
        out.add(InlineRun('[', 0));
        if (brackets.length < _maxOpen) brackets.add((out.length - 1, i));
        i++;
      case ']':
        if (brackets.isEmpty) {
          lit.write(c);
          i++;
          break;
        }
        final (open, _) = brackets.removeLast();
        final url = i + 1 < s.length && s[i + 1] == '(' ? _destination(s, i + 2) : null;
        if (url == null) {
          lit.write(c);
          i++;
          break;
        }
        flush();
        out[open] = InlineRun('', 0);
        var words = false;
        for (var k = open + 1; k < out.length; k++) {
          out[k].url = url.$1;
          words = words || out[k].text.isNotEmpty;
        }
        if (!words) out.add(InlineRun(url.$1, 0, url.$1));
        // Emphasis does not reach out of a link.
        stack.removeWhere((d) => d.at > open);
        i = url.$2;
      case '&':
        final e = s.indexOf(';', i);
        final name = e > i && e - i < 10 ? s.substring(i + 1, e) : '';
        final num = name.startsWith('#')
            ? int.tryParse(
                name.startsWith('#x') ? name.substring(2) : name.substring(1),
                radix: name.startsWith('#x') ? 16 : 10,
              )
            : null;
        if (_entities[name] case final t?) {
          lit.write(t);
          i = e + 1;
        } else if (num != null && num > 0 && num <= 0x10FFFF) {
          lit.write(visible(String.fromCharCode(num)));
          i = e + 1;
        } else {
          lit.write(c);
          i++;
        }
      case '<':
        final e = s.indexOf('>', i);
        final body = e > i && e - i < _maxUrl ? s.substring(i + 1, e) : '';
        if (RegExp(r'^https?://\S+$').hasMatch(body)) {
          flush();
          out.add(InlineRun(body, 0, body));
          i = e + 1;
        } else if (body == 'br' || body == 'br/' || body == 'br /') {
          lit.write('\n');
          i = e + 1;
        } else {
          lit.write(c);
          i++;
        }
      case 'h'
          when (s.startsWith('https://', i) || s.startsWith('http://', i)) &&
              (i == 0 || !_word.hasMatch(s[i - 1])) &&
              brackets.isEmpty:
        var n = i;
        var paren = 0;
        while (n < s.length && n - i < _maxUrl && !_space(s[n]) && !'<>"`'.contains(s[n])) {
          if (s[n] == '(') paren++;
          if (s[n] == ')' && --paren < 0) break;
          n++;
        }
        while (n > i + 8 && '.,;:!?\'"*_'.contains(s[n - 1])) {
          n--;
        }
        flush();
        final url = s.substring(i, n);
        out.add(InlineRun(url, 0, url));
        i = n;
      case _:
        lit.write(c);
        i++;
    }
  }
  flush();
  return out.where((r) => r.text.isNotEmpty).toList();
}

/// A link's destination from [at] (after the parenthesis), and where the link ends.
(String, int)? _destination(String s, int at) {
  var i = at;
  while (i < s.length && s[i] == ' ') {
    i++;
  }
  final from = i;
  var depth = 0;
  while (i < s.length && i - from < _maxUrl) {
    final c = s[i];
    if (c == '(') depth++;
    if (c == ')' && depth-- == 0) break;
    if (_space(c)) break;
    i++;
  }
  final url = s.substring(from, i);
  // An optional title, which is not shown.
  final end = s.indexOf(')', i);
  if (url.isEmpty || end < 0 || end - i > 300) return null;
  return (url, end + 1);
}

/// [onLink] is asked, never bypassed: a person sees the address before anything opens it.
TextSpan inlineSpan(
  String source,
  TextStyle base,
  TextStyle code,
  AxColors c, {
  void Function(String url)? onLink,
  List<GestureRecognizer>? recognizers,
}) => TextSpan(
  style: base,
  children: [
    for (final r in inlineRuns(source))
      TextSpan(
        text: r.text,
        recognizer: r.url == null || onLink == null || recognizers == null
            ? null
            : (() {
                final t = TapGestureRecognizer()..onTap = () => onLink(r.url!);
                recognizers.add(t);
                return t;
              })(),
        mouseCursor: r.url == null || onLink == null ? null : SystemMouseCursors.click,
        semanticsLabel: r.url == null ? null : '${r.text}, link',
        style: (r.code ? code : const TextStyle()).copyWith(
          fontVariations: r.strong && !r.code ? const [FontVariation('wght', 640)] : null,
          fontStyle: r.em ? FontStyle.italic : null,
          decoration: r.url != null
              ? TextDecoration.underline
              : r.del
              ? TextDecoration.lineThrough
              : null,
          decorationColor: r.url != null ? c.ink4 : null,
          color: r.url != null ? c.liveText : null,
        ),
      ),
  ],
);
