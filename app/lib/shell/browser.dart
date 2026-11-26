import 'dart:async';
import 'dart:convert' show base64Decode;
import 'dart:io' show Platform;

import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../host/host_store.dart';
import '../host/proto.dart';
import '../transcript/safe_text.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';
import 'drafts.dart';
import 'start.dart';

/// Whether the host has browsers here: not on Windows yet (decision 0021).
abstract final class Browsers {
  static bool get made => !Platform.isWindows;
}

/// The sizes a page is looked at in, by name.
const browserSizes = [('Phone', 390, 844), ('Tablet', 820, 1180), ('Wide', 1280, 800)];

/// What a person pointed at, as their message is given it: marked as the page's (SI-17).
String pointedAt(Pointed p) {
  String clean(String s) => s.replaceAll('```', "'''");
  final styles = [
    for (final (n, v) in p.styles)
      if (v.isNotEmpty) '$n: $v',
  ].join('; ');
  return [
    'On the page ${p.url}, this (what it says is the page\'s):',
    '```',
    clean(p.selector),
    if (p.text.trim().isNotEmpty) clean(oneLine(p.text, 300)),
    if (styles.isNotEmpty) clean(styles),
    '```',
  ].join('\n');
}

/// An agent's browser, as its person sees it: a picture of the page (decision 0021). They may take
/// it to use it themselves, and point at what is on it; while they hold it, the agent waits.
class AgentPage extends StatefulWidget {
  const AgentPage(
    this.store,
    this.session,
    this.agent, {
    required this.onClose,
    super.key,
    this.draft,
    this.name,
  });
  final HostStore store;
  final String session, agent;

  /// The agent's name, if it has one.
  final String? name;
  final Draft? draft;
  final VoidCallback onClose;
  @override
  State<AgentPage> createState() => _AgentPageState();
}

class _AgentPageState extends State<AgentPage> {
  final _focus = FocusNode(debugLabel: 'page');
  BrowserView? _view;
  Uint8List? _picture;
  String? _problem, _said;
  bool _gone = false, _pointing = false, _refusedOpen = false;

  /// The press that pointed: its release is not the page's.
  bool _pointed = false;

  @override
  void initState() {
    super.initState();
    unawaited(_follow());
  }

  @override
  void dispose() {
    _gone = true;
    // What the person held, the agent gets back.
    if (_view?.person ?? false) {
      unawaited(widget.store.holdBrowser(widget.session, widget.agent, false).catchError((_) {}));
    }
    _focus.dispose();
    super.dispose();
  }

  Future<void> _follow() async {
    var seen = 0;
    while (!_gone) {
      try {
        final v = await widget.store.browserView(widget.session, widget.agent, seen);
        if (_gone) return;
        seen = v.version;
        final frame = v.frame;
        setState(() {
          if (frame != null && frame != _view?.frame) _picture = base64Decode(frame);
          _view = v;
          _problem = null;
        });
        if (v.ended != null) return;
      } on Object catch (e) {
        if (_gone) return;
        setState(() => _problem = wordsOf(e));
        await Future<void>.delayed(const Duration(seconds: 2));
        if (e is Failed && e.code == 'not_found') return;
      }
    }
  }

  void _do(Future<void> Function() f) {
    unawaited(
      f().catchError((Object e) {
        if (!_gone) setState(() => _problem = wordsOf(e));
      }),
    );
  }

  bool get _holding => _view?.person ?? false;

  void _hold(bool person) {
    setState(() {
      _pointing = false;
      _said = person ? 'The agent waits while you use the page.' : null;
    });
    _do(() => widget.store.holdBrowser(widget.session, widget.agent, person));
    if (person) _focus.requestFocus();
  }

  /// Where on the page a point of the picture is.
  Offset? _onPage(Offset local, Size shown) {
    final v = _view;
    if (v == null || v.width == 0 || shown.width == 0) return null;
    final k = v.width / shown.width;
    return Offset(local.dx * k, local.dy * k);
  }

  void _mouse(String event, PointerEvent e, Size shown) {
    final at = _onPage(e.localPosition, shown);
    if (at == null) return;
    final button = event == 'moved'
        ? 'none'
        : e.buttons & kSecondaryMouseButton != 0
        ? 'right'
        : 'left';
    _do(
      () => widget.store.browserInput(widget.session, widget.agent, {
        'kind': 'mouse',
        'event': event,
        'x': at.dx,
        'y': at.dy,
        'button': button,
        'clicks': event == 'moved' ? 0 : 1,
        'modifiers': _modifiers(),
      }),
    );
  }

  int _modifiers() {
    final k = HardwareKeyboard.instance;
    return (k.isAltPressed ? 1 : 0) |
        (k.isControlPressed ? 2 : 0) |
        (k.isMetaPressed ? 4 : 0) |
        (k.isShiftPressed ? 8 : 0);
  }

  Future<void> _point(Offset local, Size shown) async {
    final at = _onPage(local, shown), d = widget.draft;
    if (at == null || d == null) return;
    setState(() => _pointing = false);
    try {
      final p = await widget.store.pointAt(widget.session, widget.agent, at.dx, at.dy);
      if (_gone) return;
      d.add(pointedAt(p));
      setState(() => _said = 'Added to your message.');
    } on Object catch (e) {
      if (!_gone) setState(() => _problem = wordsOf(e));
    }
  }

  KeyEventResult _key(FocusNode _, KeyEvent e) {
    if (!_holding) return KeyEventResult.ignored;
    final text = e is KeyUpEvent ? '' : (e.character ?? '');
    final printable = text.isNotEmpty && text.runes.every((r) => r >= 0x20 && r != 0x7f);
    _do(
      () => widget.store.browserInput(widget.session, widget.agent, {
        'kind': 'key',
        'event': e is KeyUpEvent ? 'up' : 'down',
        'key': e.logicalKey.keyLabel.isEmpty ? 'Unidentified' : _keyName(e),
        'code': e.physicalKey.debugName?.replaceAll(' ', '') ?? '',
        'text': printable ? text : '',
        'modifiers': _modifiers(),
      }),
    );
    return KeyEventResult.handled;
  }

  /// As the DOM names a key.
  String _keyName(KeyEvent e) => switch (e.logicalKey) {
    LogicalKeyboardKey.enter => 'Enter',
    LogicalKeyboardKey.tab => 'Tab',
    LogicalKeyboardKey.escape => 'Escape',
    LogicalKeyboardKey.backspace => 'Backspace',
    LogicalKeyboardKey.delete => 'Delete',
    LogicalKeyboardKey.arrowUp => 'ArrowUp',
    LogicalKeyboardKey.arrowDown => 'ArrowDown',
    LogicalKeyboardKey.arrowLeft => 'ArrowLeft',
    LogicalKeyboardKey.arrowRight => 'ArrowRight',
    LogicalKeyboardKey.space => ' ',
    _ => e.character ?? e.logicalKey.keyLabel,
  };

  void _size(int w, int h, bool dark) =>
      _do(() => widget.store.sizeBrowser(widget.session, widget.agent, w, h, dark));

  Future<void> _close() async {
    try {
      await widget.store.closeBrowser(widget.session, widget.agent);
      widget.onClose();
    } on Object catch (e) {
      if (mounted) setState(() => _problem = wordsOf(e));
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors, v = _view;
    final title = v == null
        ? 'Browser'
        : v.title.isNotEmpty
        ? oneLine(literal(v.title), 80)
        : oneLine(literal(v.url), 80);
    final width = v?.width ?? 0, dark = v?.dark ?? false;
    final refused = v?.refused ?? const <String>[];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(AxSpace.card, 4, 9, 4),
          child: Row(
            children: [
              AxButton.icon(
                AxIcon.close,
                label: 'Close the browser',
                tone: AxTone.quiet,
                size: 26,
                onPressed: () => unawaited(_close()),
              ),
              const SizedBox(width: 6),
              Expanded(
                child: Semantics(
                  header: true,
                  child: Text(
                    v?.ended == null ? title : '$title, ${v!.ended}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: t.type.smallStrong.ink(c.ink2),
                  ),
                ),
              ),
              if (widget.draft != null)
                AxButton(
                  label: 'Point',
                  tone: AxTone.quiet,
                  engaged: _pointing,
                  onPressed: v?.frame == null
                      ? null
                      : () => setState(() {
                          _pointing = !_pointing;
                          _said = _pointing ? 'Press what you mean on the page.' : null;
                        }),
                ),
              AxButton(
                label: _holding ? 'Give back' : 'Use it',
                tone: AxTone.quiet,
                engaged: _holding,
                onPressed: v == null || v.ended != null ? null : () => _hold(!_holding),
              ),
            ],
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(AxSpace.card, 0, 9, 6),
          child: Wrap(
            spacing: 4,
            runSpacing: 4,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              for (final (name, w, h) in browserSizes)
                AxButton(
                  label: name,
                  tone: AxTone.quiet,
                  engaged: width == w,
                  onPressed: v == null ? null : () => _size(w, h, dark),
                ),
              AxButton(
                label: 'Dark',
                tone: AxTone.quiet,
                engaged: dark,
                onPressed: v == null || width == 0
                    ? null
                    : () => _size(width, v.height == 0 ? 800 : v.height, !dark),
              ),
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
        if (refused.isNotEmpty)
          Padding(
            padding: const EdgeInsets.fromLTRB(AxSpace.card, 0, AxSpace.card, 6),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                AxButton(
                  label: refused.length == 1
                      ? 'Refused 1 address'
                      : 'Refused ${refused.length} addresses',
                  tone: AxTone.quiet,
                  engaged: _refusedOpen,
                  onPressed: () => setState(() => _refusedOpen = !_refusedOpen),
                ),
                if (_refusedOpen)
                  for (final r in refused)
                    Text(
                      oneLine(literal(r), 120),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: t.type.codeSmall.ink(c.ink3),
                    ),
              ],
            ),
          ),
        Expanded(
          child: Padding(
            padding: const EdgeInsets.fromLTRB(10, 0, 10, 10),
            child: ClipPath(
              clipper: const ContinuousClip(AxRadius.block),
              child: ColoredBox(color: c.block.fill, child: _page(t)),
            ),
          ),
        ),
      ],
    );
  }

  Widget _page(AxTheme t) {
    final v = _view, picture = _picture;
    if (v == null || picture == null || v.width == 0) {
      return Center(
        child: Text(
          v?.ended ?? (v == null ? 'Opening…' : 'Nothing is shown yet.'),
          style: t.type.small.ink(t.colors.ink3),
        ),
      );
    }
    return LayoutBuilder(
      builder: (context, box) {
        // As wide as there is room, as the page is shaped; scrolled down if it is taller.
        final shown = Size(box.maxWidth, box.maxWidth * v.height / v.width);
        final image = Image.memory(
          picture,
          width: shown.width,
          height: shown.height,
          fit: BoxFit.fill,
          gaplessPlayback: true,
          semanticLabel: 'The page ${v.title.isEmpty ? v.url : v.title}',
        );
        return SingleChildScrollView(
          physics: _holding ? const NeverScrollableScrollPhysics() : null,
          child: Focus(
            focusNode: _focus,
            onKeyEvent: _key,
            child: MouseRegion(
              cursor: _pointing ? SystemMouseCursors.precise : SystemMouseCursors.basic,
              child: Listener(
                onPointerDown: (e) {
                  if (_pointing) {
                    _pointed = true;
                    unawaited(_point(e.localPosition, shown));
                  } else if (_holding) {
                    _focus.requestFocus();
                    _mouse('pressed', e, shown);
                  }
                },
                onPointerUp: (e) {
                  if (_pointed) {
                    _pointed = false;
                  } else if (_holding && !_pointing) {
                    _mouse('released', e, shown);
                  }
                },
                // Only while a button is down: a drag, a selection. Hovering is not sent.
                onPointerMove: (e) {
                  if (_holding && !_pointing) _mouse('moved', e, shown);
                },
                onPointerSignal: (e) {
                  if (!_holding || e is! PointerScrollEvent) return;
                  final at = _onPage(e.localPosition, shown);
                  if (at == null) return;
                  _do(
                    () => widget.store.browserInput(widget.session, widget.agent, {
                      'kind': 'wheel',
                      'x': at.dx,
                      'y': at.dy,
                      'dx': e.scrollDelta.dx,
                      'dy': e.scrollDelta.dy,
                    }),
                  );
                },
                child: image,
              ),
            ),
          ),
        );
      },
    );
  }
}
