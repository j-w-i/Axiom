// The Terminal view (FL-CONSOLE-001; ADR-0018 §4): a tab per terminal, New and Close, and the
// shown terminal's screen (`xterm`), themed from the tokens, sized to the console so its columns
// and rows reach the shell. Keys typed in the screen go to the shell, except the shell's own
// shortcuts (the palette, the console, the side bar), which reach the window.

import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:xterm/xterm.dart';

import '../app/app_state.dart';
import '../state/terminals.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/tabs.dart';
import '../ui/theme.dart';
import '../ui/tokens.dart';

TerminalTheme terminalTheme(AxiomColors c) {
  final a = c.isDark ? AnsiColors.dark : AnsiColors.light;
  return TerminalTheme(
    cursor: c.accent,
    selection: c.selection,
    foreground: c.ink,
    background: c.recessed,
    black: a[0],
    red: a[1],
    green: a[2],
    yellow: a[3],
    blue: a[4],
    magenta: a[5],
    cyan: a[6],
    white: a[7],
    brightBlack: a[8],
    brightRed: a[9],
    brightGreen: a[10],
    brightYellow: a[11],
    brightBlue: a[12],
    brightMagenta: a[13],
    brightCyan: a[14],
    brightWhite: a[15],
    searchHitBackground: c.infoSoft,
    searchHitBackgroundCurrent: c.info,
    searchHitForeground: c.ink,
  );
}

class TerminalPane extends StatefulWidget {
  const TerminalPane({super.key});

  @override
  State<TerminalPane> createState() => TerminalPaneState();
}

class TerminalPaneState extends State<TerminalPane> {
  late final AppState app = AppScope.of(context);
  final _focus = <String, FocusNode>{};

  FocusNode focusFor(String id) =>
      _focus.putIfAbsent(id, () => FocusNode(debugLabel: 'terminal $id'));

  /// Drops the nodes of closed terminals (AUD-083).
  void _prune(Iterable<String> live) {
    final keep = live.toSet();
    if (_focus.keys.every(keep.contains)) return;
    // After the frame: the closed terminal's screen lets go of its node in this build.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      _focus.removeWhere((id, n) {
        if (keep.contains(id) || n.hasFocus || (n.context?.mounted ?? false)) return false;
        n.dispose();
        return true;
      });
    });
  }

  @override
  void initState() {
    super.initState();
    app.focusTerminal = focusShown;
  }

  @override
  void dispose() {
    if (app.focusTerminal == focusShown) app.focusTerminal = null;
    for (final f in _focus.values) {
      f.dispose();
    }
    super.dispose();
  }

  /// Focuses the shown terminal, creating one when there is none.
  Future<void> focusShown() async {
    final s = app.terminals;
    if (s.activeTab == null) await s.create();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      final id = s.active;
      if (mounted && id != null) focusFor(id).requestFocus();
    });
  }

  Future<void> _new() async {
    final id = await app.terminals.create();
    if (id != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) focusFor(id).requestFocus();
      });
    }
  }

  /// The window's chords, which pass through the screen to the shell (as in the Tauri shell):
  /// Mod+K palette, Mod+P files, Mod+J console, Mod+B side bar, Mod+` and Mod+Shift+` terminal,
  /// Mod+, settings, Mod+L dock. On macOS every Cmd chord is the window's; Ctrl chords go to the
  /// shell.
  static bool isAppChord(KeyEvent e, {bool? mac}) {
    final k = HardwareKeyboard.instance;
    final isMac = mac ?? defaultTargetPlatform == TargetPlatform.macOS;
    final mod = isMac ? k.isMetaPressed : k.isControlPressed;
    if (!mod || k.isAltPressed) return false;
    if (isMac) return true;
    final l = e.logicalKey;
    if (l == LogicalKeyboardKey.backquote) return true;
    return !k.isShiftPressed &&
        {
          LogicalKeyboardKey.keyK,
          LogicalKeyboardKey.keyP,
          LogicalKeyboardKey.keyJ,
          LogicalKeyboardKey.keyB,
          LogicalKeyboardKey.comma,
          LogicalKeyboardKey.keyL,
        }.contains(l);
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final s = app.terminals;
    return ListenableBuilder(
      listenable: Listenable.merge([s, app.workspace]),
      builder: (context, _) {
        if (!app.workspace.isOpen) {
          return Padding(
            padding: const EdgeInsets.all(Metrics.s4),
            child: Text('Open a folder to use a terminal.', style: t.type.small),
          );
        }
        final shown = s.activeTab;
        _prune([for (final x in s.terminals) x.info.terminalId]);
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            SizedBox(
              height: context.scaled(28, fontSize: 12, lineHeight: 16),
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: Metrics.s3),
                child: Row(
                  children: [
                    if (s.terminals.isNotEmpty)
                      Flexible(
                        child: AxTabs(
                          label: 'Terminals',
                          height: 28,
                          tabs: [
                            for (final x in s.terminals)
                              TabItem(id: x.info.terminalId, label: s.label(x)),
                          ],
                          selected: s.active ?? '',
                          onSelect: (id) {
                            s.activate(id);
                            focusFor(id).requestFocus();
                          },
                        ),
                      ),
                    AxIconButton(
                      icon: AxiomIcons.plus,
                      label: 'New terminal',
                      shortcut: 'Mod+Shift+`',
                      size: 24,
                      onPressed: () => unawaited(_new()),
                    ),
                    if (shown != null)
                      AxIconButton(
                        icon: AxiomIcons.trash,
                        label: 'Close ${s.label(shown)}',
                        size: 24,
                        onPressed: () => unawaited(s.close(shown.info.terminalId)),
                      ),
                    const Spacer(),
                    if (s.error.isNotEmpty)
                      Flexible(
                        child: Text(
                          s.error,
                          style: t.type.small.copyWith(color: c.dangerText),
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                  ],
                ),
              ),
            ),
            Expanded(
              child: shown == null
                  ? Center(
                      child: AxButton(
                        label: 'New terminal',
                        icon: AxiomIcons.terminal,
                        onPressed: () => unawaited(_new()),
                      ),
                    )
                  : _Screen(
                      key: ValueKey(shown.info.terminalId),
                      tab: shown,
                      focusNode: focusFor(shown.info.terminalId),
                      label: s.label(shown),
                    ),
            ),
          ],
        );
      },
    );
  }
}

class _Screen extends StatefulWidget {
  const _Screen({super.key, required this.tab, required this.focusNode, required this.label});
  final TerminalTab tab;
  final FocusNode focusNode;
  final String label;

  @override
  State<_Screen> createState() => _ScreenState();
}

class _ScreenState extends State<_Screen> {
  final _controller = TerminalController();

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  /// Copy (the selection) and paste: Cmd+C/V on macOS, Ctrl+Shift+C/V elsewhere (Ctrl+C stays
  /// the interrupt).
  KeyEventResult _key(FocusNode node, KeyEvent e) {
    if (e is! KeyDownEvent && e is! KeyRepeatEvent) return KeyEventResult.ignored;
    if (TerminalPaneState.isAppChord(e)) {
      final app = AppScope.of(context);
      return (app.shellShortcut?.call(e) ?? false)
          ? KeyEventResult.handled
          : KeyEventResult.ignored;
    }
    final k = HardwareKeyboard.instance;
    final mac = defaultTargetPlatform == TargetPlatform.macOS;
    final clip = mac
        ? k.isMetaPressed && !k.isControlPressed
        : k.isControlPressed && k.isShiftPressed;
    if (!clip) return KeyEventResult.ignored;
    final screen = widget.tab.screen;
    if (e.logicalKey == LogicalKeyboardKey.keyC) {
      final sel = _controller.selection;
      if (sel != null) {
        unawaited(Clipboard.setData(ClipboardData(text: screen.buffer.getText(sel))));
        _controller.clearSelection();
      }
      return KeyEventResult.handled;
    }
    if (e.logicalKey == LogicalKeyboardKey.keyV) {
      unawaited(
        Clipboard.getData(Clipboard.kTextPlain).then((d) {
          final text = d?.text;
          if (text != null && text.isNotEmpty) screen.paste(text);
        }),
      );
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final app = AppScope.of(context);
    return Semantics(
      container: true,
      label: 'Terminal ${widget.label}',
      // The well runs edge to edge under the tab row: the terminal's own background, with no
      // seam of the panel's tone around it (AUD-084).
      child: ColoredBox(
        color: t.colors.recessed,
        child: TerminalView(
          padding: const EdgeInsets.fromLTRB(Metrics.s3, Metrics.s2, Metrics.s2, Metrics.s2),
          widget.tab.screen,
          controller: _controller,
          focusNode: widget.focusNode,
          theme: terminalTheme(t.colors),
          textStyle: TerminalStyle(
            fontSize: app.settings.editorFontSize.toDouble() - 1,
            fontFamily: AxiomType.mono,
          ),
          hardwareKeyboardOnly: true,
          onKeyEvent: _key,
        ),
      ),
    );
  }
}
