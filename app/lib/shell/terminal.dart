import 'dart:async';
import 'dart:io' show Platform;

import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../host/host_store.dart';
import '../host/proto.dart';
import '../patterns/session_panel.dart';
import '../transcript/inline.dart';
import '../transcript/safe_text.dart';
import '../ui/commands.dart';
import '../ui/controls.dart';
import '../ui/editable.dart';
import '../ui/icons.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';
import 'drafts.dart';
import 'start.dart';

/// Whether the host makes terminals here: not on Windows yet (decision 0022).
abstract final class Terminals {
  static bool get made => !Platform.isWindows;
}

/// What a key sends to a terminal; null: nothing of the terminal's.
String? keyBytes(KeyEvent e, {bool appCursor = false}) {
  if (e is KeyUpEvent) return null;
  final k = HardwareKeyboard.instance;
  final key = e.logicalKey;
  final arrow = appCursor ? '\x1bO' : '\x1b[';
  switch (key) {
    case LogicalKeyboardKey.enter || LogicalKeyboardKey.numpadEnter:
      return '\r';
    case LogicalKeyboardKey.backspace:
      return k.isAltPressed ? '\x1b\x7f' : '\x7f';
    case LogicalKeyboardKey.tab:
      return k.isShiftPressed ? '\x1b[Z' : '\t';
    case LogicalKeyboardKey.escape:
      return '\x1b';
    case LogicalKeyboardKey.arrowUp:
      return '${arrow}A';
    case LogicalKeyboardKey.arrowDown:
      return '${arrow}B';
    case LogicalKeyboardKey.arrowRight:
      return k.isAltPressed ? '\x1bf' : '${arrow}C';
    case LogicalKeyboardKey.arrowLeft:
      return k.isAltPressed ? '\x1bb' : '${arrow}D';
    case LogicalKeyboardKey.home:
      return '\x1b[H';
    case LogicalKeyboardKey.end:
      return '\x1b[F';
    case LogicalKeyboardKey.delete:
      return '\x1b[3~';
    case LogicalKeyboardKey.pageUp:
      return '\x1b[5~';
    case LogicalKeyboardKey.pageDown:
      return '\x1b[6~';
  }
  // Command is the window's; Control with a letter is the terminal's own.
  if (k.isMetaPressed) return null;
  if (k.isControlPressed) {
    final label = key.keyLabel;
    if (label.length == 1) {
      final c = label.toUpperCase().codeUnitAt(0);
      if (c >= 0x40 && c <= 0x5F) return String.fromCharCode(c - 0x40);
    }
    return null;
  }
  final ch = e.character;
  if (ch == null || ch.isEmpty || ch.codeUnitAt(0) < 0x20 && ch != '\t') return null;
  return k.isAltPressed && ch.length == 1 && ch.codeUnitAt(0) < 0x80 ? '\x1b$ch' : ch;
}

/// What is pasted, as the terminal takes it: marked as pasted where the program asks for that, and
/// with nothing in it that would end the mark.
String pasted(String text, {required bool bracketed}) {
  final clean = text.replaceAll('\r\n', '\n').replaceAll('\n', '\r').replaceAll('\x1b[201~', '');
  return bracketed ? '\x1b[200~$clean\x1b[201~' : clean;
}

/// The path that a press at [col] of [line] is on, and its line, if there is one.
(String, int?)? pathAt(String line, int col) {
  if (col < 0 || col >= line.length) return null;
  bool part(String c) => !RegExp(
    r'[\s"'
    "'"
    r'`()<>\[\]{},;]',
  ).hasMatch(c);
  var a = col, b = col;
  while (a > 0 && part(line[a - 1])) {
    a--;
  }
  while (b < line.length && part(line[b])) {
    b++;
  }
  if (a == b) return null;
  // A sentence's full stop is no part of it.
  var word = line.substring(a, b);
  while (word.endsWith('.') || word.endsWith(':')) {
    word = word.substring(0, word.length - 1);
  }
  return pathIn(word);
}

/// A terminal of the person's own: its screen as the host keeps it, typed into, scrolled, selected
/// from. A path in it opens the file; what is selected goes into the message as its output.
class TerminalView extends StatefulWidget {
  const TerminalView(
    this.store,
    this.terminal, {
    required this.onClose,
    super.key,
    this.draft,
    this.onPath,
    this.autofocus = true,
  });
  final HostStore store;
  final String terminal;
  final Draft? draft;
  final VoidCallback onClose;
  final void Function(String path, int? line)? onPath;
  final bool autofocus;
  @override
  State<TerminalView> createState() => _TerminalViewState();
}

class _TerminalViewState extends State<TerminalView> {
  final _focus = FocusNode(debugLabel: 'terminal');
  final _select = FocusNode(debugLabel: 'terminal text');
  Screen? _screen;
  String? _problem, _selected, _said;
  bool _gone = false, _closing = false;
  (int, int)? _size;
  Timer? _resize;
  String? _paste;
  Offset? _down;

  @override
  void initState() {
    super.initState();
    unawaited(_follow());
  }

  @override
  void dispose() {
    _gone = true;
    _resize?.cancel();
    _focus.dispose();
    _select.dispose();
    super.dispose();
  }

  Future<void> _follow() async {
    var seen = 0;
    while (!_gone) {
      try {
        final s = await widget.store.screen(widget.terminal, seen);
        if (_gone) return;
        seen = s.version;
        setState(() {
          _screen = s;
          _problem = null;
        });
        if (s.ended != null) return;
      } on Object catch (e) {
        if (_gone) return;
        setState(() => _problem = wordsOf(e));
        await Future<void>.delayed(const Duration(seconds: 2));
        if (e is Failed && e.code == 'not_found') return;
      }
    }
  }

  void _type(String text) {
    if (_screen?.ended != null) return;
    unawaited(
      widget.store.typeInto(widget.terminal, text).catchError((Object e) {
        if (!_gone) setState(() => _problem = wordsOf(e));
      }),
    );
  }

  KeyEventResult _key(FocusNode _, KeyEvent e) {
    final k = HardwareKeyboard.instance;
    final paste =
        e is! KeyUpEvent &&
        e.logicalKey == LogicalKeyboardKey.keyV &&
        (Command.isMac ? k.isMetaPressed : k.isControlPressed && k.isShiftPressed);
    if (paste) {
      unawaited(_fromClipboard());
      return KeyEventResult.handled;
    }
    final bytes = keyBytes(e, appCursor: _screen?.appCursor ?? false);
    if (bytes == null) return KeyEventResult.ignored;
    if ((_screen?.back ?? 0) > 0) unawaited(widget.store.scrollTerminal(widget.terminal, 0));
    _type(bytes);
    return KeyEventResult.handled;
  }

  Future<void> _fromClipboard() async {
    final text = (await Clipboard.getData(Clipboard.kTextPlain))?.text;
    if (text == null || text.isEmpty || _gone) return;
    final bracketed = _screen?.bracketed ?? false;
    // Lines pasted where the program does not mark them run as they arrive: asked first (T42).
    if (!bracketed && (text.contains('\n') || text.contains('\r'))) {
      setState(() => _paste = text);
      return;
    }
    _type(pasted(text, bracketed: bracketed));
  }

  void _scroll(PointerSignalEvent e) {
    if (e is! PointerScrollEvent) return;
    final s = _screen;
    if (s == null) return;
    final by = (e.scrollDelta.dy / 18).round();
    final back = (s.back - by).clamp(0, s.mostBack);
    if (back != s.back) unawaited(widget.store.scrollTerminal(widget.terminal, back));
  }

  Future<void> _close({bool force = false}) async {
    try {
      await widget.store.closeTerminal(widget.terminal, force: force);
      widget.onClose();
    } on Failed catch (e) {
      if (e.code == 'busy' && mounted) {
        setState(() => _closing = true);
      } else if (mounted) {
        setState(() => _problem = e.message);
      }
    }
  }

  (double, double) _cell(AxTheme t) {
    final p = TextPainter(
      text: TextSpan(text: 'MMMMMMMMMM', style: t.type.codeSmall),
      textDirection: TextDirection.ltr,
    )..layout();
    final size = (p.width / 10, p.height);
    p.dispose();
    return size;
  }

  void _fit(int cols, int rows) {
    if (_size == (cols, rows)) return;
    _size = (cols, rows);
    _resize?.cancel();
    _resize = Timer(const Duration(milliseconds: 150), () {
      unawaited(widget.store.resizeTerminal(widget.terminal, cols, rows).catchError((Object _) {}));
    });
  }

  TextSpan _row(AxTheme t, List<Span> row) {
    final c = t.colors;
    return TextSpan(
      children: [
        for (final s in row)
          () {
            var fg = terminalColor(s.fg, c) ?? c.ink1;
            var bg = terminalColor(s.bg, c);
            if (s.style & 16 != 0) (fg, bg) = (bg ?? c.canvas.fill, fg);
            return TextSpan(
              text: s.text,
              style: TextStyle(
                color: s.style & 2 != 0 ? fg.withValues(alpha: .6) : fg,
                backgroundColor: bg,
                fontVariations: s.style & 1 != 0 ? const [FontVariation('wght', 640)] : null,
                fontStyle: s.style & 4 != 0 ? FontStyle.italic : null,
                decoration: s.style & 8 != 0 ? TextDecoration.underline : null,
              ),
            );
          }(),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors, s = _screen;
    final title = s == null || s.title.isEmpty ? 'Terminal' : oneLine(literal(s.title), 80);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(AxSpace.card, 4, 9, 4),
          child: Row(
            children: [
              AxButton.icon(
                AxIcon.close,
                label: 'Close the terminal',
                tone: AxTone.quiet,
                size: 26,
                onPressed: () => unawaited(_close()),
              ),
              const SizedBox(width: 6),
              Expanded(
                child: Semantics(
                  header: true,
                  child: Text(
                    s?.ended == null ? title : '$title, ${s!.ended}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: t.type.smallStrong.ink(c.ink2),
                  ),
                ),
              ),
              if (widget.draft case final d?)
                AxButton(
                  label: 'Add to message',
                  tone: AxTone.quiet,
                  onPressed: (_selected ?? '').trim().isEmpty
                      ? null
                      : () {
                          d.add(terminalOutput(title, _selected!));
                          setState(() => _said = 'Added to your message.');
                        },
                ),
            ],
          ),
        ),
        if (_closing)
          Padding(
            padding: const EdgeInsets.fromLTRB(AxSpace.card, 0, AxSpace.card, 8),
            child: Wrap(
              spacing: 8,
              runSpacing: 6,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                Text('Something still runs in it.', style: t.type.ui.ink(c.ink1)),
                AxButton(
                  label: 'Keep it',
                  autofocus: true,
                  onPressed: () => setState(() => _closing = false),
                ),
                AxButton(label: 'End it', onPressed: () => unawaited(_close(force: true))),
              ],
            ),
          ),
        if (_paste case final p?)
          Padding(
            padding: const EdgeInsets.fromLTRB(AxSpace.card, 0, AxSpace.card, 8),
            child: Wrap(
              spacing: 8,
              runSpacing: 6,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                Text(
                  'Paste ${'\n'.allMatches(p.trimRight()).length + 1} lines? Each runs as it arrives.',
                  style: t.type.ui.ink(c.ink1),
                ),
                AxButton(
                  label: 'Paste',
                  onPressed: () {
                    setState(() => _paste = null);
                    _type(pasted(p, bracketed: false));
                    _focus.requestFocus();
                  },
                ),
                AxButton(label: 'Cancel', onPressed: () => setState(() => _paste = null)),
              ],
            ),
          ),
        if ((_problem ?? _said) case final w?)
          Padding(
            padding: const EdgeInsets.fromLTRB(AxSpace.card, 0, AxSpace.card, 6),
            child: Text(
              visible(w),
              style: t.type.small.ink(_problem != null ? c.faultText : c.ink3),
            ),
          ),
        Expanded(
          child: Padding(
            padding: const EdgeInsets.fromLTRB(10, 0, 10, 10),
            child: ClipPath(
              clipper: const ContinuousClip(AxRadius.block),
              child: ColoredBox(
                color: c.block.fill,
                child: Padding(
                  padding: const EdgeInsets.all(8),
                  child: LayoutBuilder(builder: (context, box) => _grid(t, box)),
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }

  Widget _grid(AxTheme t, BoxConstraints box) {
    final c = t.colors, s = _screen;
    final (w, h) = _cell(t);
    final cols = (box.maxWidth / w).floor().clamp(2, 1000);
    final rows = (box.maxHeight / h).floor().clamp(2, 1000);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!_gone) _fit(cols, rows);
    });
    final lines = s?.rows ?? const <List<Span>>[];
    final cursor = s?.cursor;
    final grid = Stack(
      children: [
        for (final (i, r) in lines.indexed)
          Positioned(
            left: 0,
            right: 0,
            top: i * h,
            height: h,
            child: Text.rich(
              _row(t, r),
              style: t.type.codeSmall.ink(c.ink1),
              maxLines: 1,
              softWrap: false,
              overflow: TextOverflow.clip,
            ),
          ),
        if (cursor case (final r, final col))
          Positioned(
            left: col * w,
            top: r * h,
            width: w,
            height: h,
            child: IgnorePointer(
              child: ColoredBox(color: _focus.hasFocus ? c.ink2.withValues(alpha: .7) : c.ink4),
            ),
          ),
      ],
    );
    return Listener(
      onPointerSignal: _scroll,
      child: Focus(
        focusNode: _focus,
        autofocus: widget.autofocus,
        onKeyEvent: _key,
        onFocusChange: (_) => setState(() {}),
        child: Semantics(
          label: 'Terminal',
          value: s?.text ?? '',
          // Under whatever selecting text takes: a press that did not move.
          child: Listener(
            onPointerDown: (d) => _down = d.localPosition,
            onPointerUp: (d) {
              final from = _down;
              _down = null;
              if (from == null || (d.localPosition - from).distance > 4) return;
              _focus.requestFocus();
              final r = (d.localPosition.dy / h).floor(), col = (d.localPosition.dx / w).floor();
              if (r < 0 || r >= lines.length) return;
              final line = lines[r].map((x) => x.text).join();
              if (pathAt(line, col) case (final path, final at)) widget.onPath?.call(path, at);
            },
            child: SelectableRegion(
              focusNode: _select,
              selectionControls: axSelectionControls,
              onSelectionChanged: (v) => setState(() => _selected = v?.plainText),
              contextMenuBuilder: (context, st) =>
                  AxContextMenu(anchors: st.contextMenuAnchors, items: st.contextMenuButtonItems),
              child: SizedBox(width: cols * w, height: rows * h, child: grid),
            ),
          ),
        ),
      ),
    );
  }
}

/// What was selected in a terminal, as it goes into a message: marked as output, so that it is not
/// taken for the person's own words (SI-17).
String terminalOutput(String title, String text) =>
    'Output of the terminal "$title":\n```\n${text.trimRight().replaceAll('```', "'''")}\n```';

/// The terminals of a session, and the folder's commands, in the list of the panel.
class TerminalsPart extends StatefulWidget {
  const TerminalsPart(
    this.store,
    this.session, {
    required this.onOpen,
    super.key,
    this.commands = const [],
  });
  final HostStore store;
  final String session;

  /// A terminal is to be shown.
  final ValueChanged<String> onOpen;
  final List<FolderCommand> commands;
  @override
  State<TerminalsPart> createState() => _TerminalsPartState();
}

class _TerminalsPartState extends State<TerminalsPart> {
  List<TerminalInfo> _all = const [];
  Timer? _every;
  String? _problem;
  bool _busy = false, _folded = false;

  @override
  void initState() {
    super.initState();
    unawaited(_look());
    _every = Timer.periodic(const Duration(seconds: 2), (_) => unawaited(_look()));
  }

  @override
  void dispose() {
    _every?.cancel();
    super.dispose();
  }

  Future<void> _look() async {
    try {
      final all = await widget.store.terminals();
      if (!mounted) return;
      setState(
        () => _all = [
          for (final x in all)
            if (x.session == widget.session) x,
        ],
      );
    } on Object {
      // Asked again soon.
    }
  }

  Future<void> _open({String? command}) async {
    setState(() {
      _busy = true;
      _problem = null;
    });
    try {
      final id = await widget.store.openTerminal(widget.session, 100, 30, command: command);
      widget.onOpen(id);
    } on Object catch (e) {
      if (mounted) setState(() => _problem = wordsOf(e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return PanelGroup(
      'Terminals',
      AxButton.icon(
        AxIcon.plus,
        label: 'New terminal',
        tone: AxTone.quiet,
        size: 26,
        onPressed: _busy ? null : () => unawaited(_open()),
      ),
      acts: true,
      folded: _folded,
      onFold: () => setState(() => _folded = !_folded),
      [
        if (_problem case final p?)
          Padding(
            padding: const EdgeInsets.fromLTRB(AxSpace.card, 4, AxSpace.card, 4),
            child: Text(visible(p), style: t.type.small.ink(c.faultText)),
          ),
        for (final x in _all)
          PanelRow(
            AxIcon.terminal,
            oneLine(literal(x.title), 80),
            state: x.ended ?? (x.busy ? 'running' : null),
            onPressed: () => widget.onOpen(x.terminal),
          ),
        if (widget.commands.isNotEmpty)
          Padding(
            padding: const EdgeInsets.fromLTRB(10, 6, 10, 6),
            child: Wrap(
              spacing: 6,
              runSpacing: 6,
              children: [
                for (final (i, cmd) in widget.commands.indexed)
                  AxButton(
                    label: oneLine(literal(cmd.name), 40),
                    icon: iconNamed(cmd.icon),
                    shortcut: commandKeys(cmd.key ?? i + 1),
                    onPressed: _busy ? null : () => unawaited(_open(command: cmd.command)),
                  ),
              ],
            ),
          ),
        if (_all.isEmpty && widget.commands.isEmpty && _problem == null)
          Padding(
            padding: const EdgeInsets.fromLTRB(AxSpace.card, 4, AxSpace.card, 8),
            child: Text(
              'Your own shell, in this folder. Agents cannot use it.',
              style: t.type.small.ink(c.ink3),
            ),
          ),
      ],
    );
  }
}

/// A folder's command's icon, by its name.
AxIcon iconNamed(String? name) => switch (name) {
  'terminal' => AxIcon.terminal,
  'check' => AxIcon.check,
  'list' => AxIcon.list,
  'web' => AxIcon.web,
  'branch' => AxIcon.branch,
  'file' => AxIcon.file,
  'refresh' => AxIcon.refresh,
  _ => AxIcon.play,
};

/// How a folder's command [n] (1 to 9) is run from the keyboard.
String? commandKeys(int n) => n < 1 || n > 9
    ? null
    : Command.isMac
    ? '⌥⌘$n'
    : 'Ctrl+Alt+$n';

/// The folder's command whose keys were pressed, from 1; null: none of these.
int? commandKey(KeyEvent e) {
  if (e is! KeyDownEvent) return null;
  final k = HardwareKeyboard.instance;
  final held = Command.isMac
      ? k.isMetaPressed && k.isAltPressed && !k.isControlPressed
      : k.isControlPressed && k.isAltPressed && !k.isMetaPressed;
  if (!held || k.isShiftPressed) return null;
  final at = e.logicalKey.keyId - LogicalKeyboardKey.digit0.keyId;
  return at >= 1 && at <= 9 ? at : null;
}
