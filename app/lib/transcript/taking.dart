import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../ui/controls.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';
import 'safe_text.dart';

/// Most of a passage that is quoted.
const _most = 8000;

/// What somebody said, for a draft: every line of it marked, and whose it is said first. What an
/// agent wrote stays the agent's, whoever sends it (SI-17).
String quoted(String whose, String text) {
  final all = text.replaceAll('\r\n', '\n').trim();
  final cut = cutTo(all, _most);
  final lines = cut.split(RegExp('[\n\r  ]')).map((l) => l.isEmpty ? '>' : '> $l');
  return [
    'Quoted from $whose:',
    ...lines,
    if (cut.length < all.length) '> (${all.length - cut.length} more characters)',
    '',
    '',
  ].join('\n');
}

/// A table as CSV. What a spreadsheet would take for a formula is made words.
String csvOf(List<List<String>> rows) {
  final number = RegExp(r'^[+-]?[\d.,]+%?$');
  String cell(String s) {
    final t = s.isNotEmpty && '=+-@\t\r'.contains(s[0]) && !number.hasMatch(s) ? "'$s" : s;
    return t.contains(RegExp('[",\n\r]')) ? '"${t.replaceAll('"', '""')}"' : t;
  }

  return [for (final r in rows) r.map(cell).join(',')].join('\n');
}

/// What can be done with something said, over its corner: while the pointer is on it, or the
/// keyboard in it.
class Taking extends StatefulWidget {
  const Taking({
    required this.child,
    required this.text,
    super.key,
    this.copy = 'Copy',
    this.onQuote,
    this.stops = true,
    this.low = false,
  });
  final Widget child;

  /// What is copied, as it was written.
  final String Function() text;

  /// What the key says.
  final String copy;
  final VoidCallback? onQuote;

  /// The keyboard stops here: once for each message.
  final bool stops;

  /// At its lower corner: the upper one is the message's.
  final bool low;
  @override
  State<Taking> createState() => _TakingState();
}

class _TakingState extends State<Taking> {
  bool _over = false, _within = false;
  String? _done;

  Future<void> _copy() async {
    final s = widget.text();
    await Clipboard.setData(ClipboardData(text: s));
    final n = hiddenIn(s);
    final what = n == 1 ? 'a hidden character' : '$n hidden characters';
    if (mounted) setState(() => _done = n == 0 ? 'Copied' : 'Copied, with $what');
  }

  @override
  Widget build(BuildContext context) {
    final shown = _over || _within;
    return MouseRegion(
      onEnter: (_) => setState(() => _over = true),
      onExit: (_) => setState(() {
        _over = false;
        _done = null;
      }),
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          widget.child,
          if (shown || widget.stops)
            Positioned(
              top: widget.low ? null : 0,
              bottom: widget.low ? 0 : null,
              right: 0,
              child: SelectionContainer.disabled(
                child: Focus(
                  canRequestFocus: false,
                  skipTraversal: true,
                  onFocusChange: (v) => setState(() => _within = v),
                  child: Opacity(
                    opacity: shown ? 1 : 0,
                    child: Surface(
                      Ax.of(context).colors.overlay,
                      radius: AxRadius.card,
                      child: Padding(
                        padding: const EdgeInsets.all(2),
                        child: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            AxButton(
                              label: _done ?? widget.copy,
                              tone: AxTone.quiet,
                              size: 24,
                              onPressed: _copy,
                            ),
                            if (widget.onQuote case final quote?)
                              AxButton(
                                label: 'Quote',
                                tone: AxTone.quiet,
                                size: 24,
                                onPressed: quote,
                              ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}
