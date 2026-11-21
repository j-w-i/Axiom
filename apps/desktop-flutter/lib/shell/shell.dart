// The shell (FL-SHELL-001): title row, rail, side bar, main area, status bar with the hint line;
// the palette and settings; keyboard shortcuts. Behaviour follows the Tauri shell (smoke.mjs and
// geometry.mjs are the specification); the look is ADR-0020 §4.

import 'dart:async';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/physics.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../app/app_state.dart';
import '../daemon/client.dart';
import '../files/explorer.dart';
import '../state/settings.dart';
import '../ui/controls.dart';
import '../ui/icon_data.dart';
import '../ui/icons.dart';
import '../ui/overlays.dart';
import '../ui/pressable.dart';
import '../ui/splitter.dart';
import '../ui/surfaces.dart';
import '../ui/theme.dart';
import '../ui/tokens.dart';
import 'palette.dart';
import 'settings_page.dart';

final _isMac = defaultTargetPlatform == TargetPlatform.macOS;

SingleActivator _mod(LogicalKeyboardKey k, {bool shift = false}) =>
    SingleActivator(k, control: !_isMac, meta: _isMac, shift: shift);

class Shell extends StatefulWidget {
  const Shell({super.key});

  @override
  State<Shell> createState() => _ShellState();
}

class _ShellState extends State<Shell> {
  late final AppState app = AppScope.of(context);
  final _sideSplitter = FocusNode(debugLabel: 'side bar splitter');
  bool _paletteOpen = false;
  String _paletteSeed = '';

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      app.openPalette = () => _showPalette();
      app.openSettings = _showSettings;
      app.pickFolder = _pickFolder;
    });
  }

  @override
  void dispose() {
    _sideSplitter.dispose();
    super.dispose();
  }

  Future<void> _pickFolder() async {
    final dir = await getDirectoryPath(confirmButtonText: 'Open');
    if (dir != null) await app.workspace.openFolder(dir);
  }

  void _showPalette([String seed = '']) => setState(() {
    _paletteOpen = true;
    _paletteSeed = seed;
  });

  void _showSettings() {
    unawaited(
      showAxDialog<void>(
        context: context,
        label: 'Settings',
        builder: (_) => SettingsPage(app: app),
      ),
    );
  }

  List<Command> commands() {
    final ws = app.workspace;
    final s = app.settings;
    return [
      Command(
        id: 'open-folder',
        title: 'Open Folder…',
        shortcut: 'Mod+O',
        category: 'File',
        run: () => unawaited(_pickFolder()),
      ),
      Command(
        id: 'close-folder',
        title: 'Close Folder',
        category: 'File',
        enabled: () => ws.isOpen,
        run: () => unawaited(ws.close()),
      ),
      for (final r in ws.recent.take(8))
        Command(
          id: 'recent:${r.root}',
          title: 'Open Recent: ${r.name}',
          category: 'File',
          run: () => unawaited(ws.openFolder(r.root)),
        ),
      Command(
        id: 'toggle-side',
        title: 'Toggle Side Bar',
        shortcut: 'Mod+B',
        category: 'View',
        run: () => setState(() => app.sideVisible = !app.sideVisible),
      ),
      Command(
        id: 'files',
        title: 'Show Files',
        category: 'View',
        run: () => _go(Destination.files),
      ),
      Command(
        id: 'settings',
        title: 'Settings',
        shortcut: 'Mod+,',
        category: 'Preferences',
        run: _showSettings,
      ),
      for (final th in ThemeSetting.values)
        Command(
          id: 'theme:${th.name}',
          title: 'Theme: ${th.name[0].toUpperCase()}${th.name.substring(1)}',
          category: 'Preferences',
          run: () => unawaited(s.setValue('appearance.theme', th.name)),
        ),
      for (final m in MotionMode.values)
        Command(
          id: 'motion:${m.name}',
          title: 'Motion: ${m.name[0].toUpperCase()}${m.name.substring(1)}',
          category: 'Preferences',
          run: () => unawaited(s.setValue('appearance.motion', m.name)),
        ),
      Command(
        id: 'restart-backend',
        title: 'Restart the Backend',
        category: 'Help',
        run: () => unawaited(app.restartBackend()),
      ),
    ];
  }

  void _go(Destination d) => setState(() {
    if (app.destination == d && app.sideVisible) return;
    app.destination = d;
    app.sideVisible = true;
  });

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Shortcuts(
      shortcuts: {
        _mod(LogicalKeyboardKey.keyK): const _CommandIntent('palette'),
        _mod(LogicalKeyboardKey.keyP, shift: true): const _CommandIntent('palette'),
        _mod(LogicalKeyboardKey.keyB): const _CommandIntent('toggle-side'),
        _mod(LogicalKeyboardKey.comma): const _CommandIntent('settings'),
        _mod(LogicalKeyboardKey.keyO): const _CommandIntent('open-folder'),
      },
      child: Actions(
        actions: {
          _CommandIntent: CallbackAction<_CommandIntent>(
            onInvoke: (i) {
              if (i.id == 'palette') {
                _showPalette();
              } else {
                commands().firstWhere((x) => x.id == i.id).run();
              }
              return null;
            },
          ),
        },
        child: FocusScope(
          autofocus: true,
          child: Stack(
            children: [
              ColoredBox(
                color: c.ground,
                child: Column(
                  children: [
                    _TitleRow(commands: commands, onSearch: _showPalette),
                    const Groove(),
                    Expanded(
                      child: ListenableBuilder(
                        listenable: app.prefs,
                        builder: (context, _) => Row(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            _Rail(
                              selected: app.sideVisible ? app.destination : null,
                              onSelect: (d) {
                                setState(() {
                                  if (app.destination == d && app.sideVisible) {
                                    app.sideVisible = false;
                                  } else {
                                    app.destination = d;
                                    app.sideVisible = true;
                                  }
                                });
                              },
                            ),
                            const Groove(vertical: true),
                            if (app.sideVisible) ...[
                              SizedBox(
                                width: app.sideWidth,
                                child: _SideBar(destination: app.destination),
                              ),
                              AxSplitter(
                                label: 'Resize the side bar',
                                value: app.sideWidth,
                                min: 180,
                                max: 520,
                                defaultValue: AppState.sideDefault,
                                focusNode: _sideSplitter,
                                onChanged: (v) => app.sideWidth = v,
                              ),
                            ],
                            const Expanded(child: _Main()),
                          ],
                        ),
                      ),
                    ),
                    const Groove(),
                    const _StatusBar(),
                  ],
                ),
              ),
              const Positioned(right: 0, bottom: 0, child: SheetWarmup()),
              if (_paletteOpen)
                Palette(
                  commands: commands(),
                  seed: _paletteSeed,
                  onClose: () => setState(() => _paletteOpen = false),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

class _CommandIntent extends Intent {
  const _CommandIntent(this.id);
  final String id;
}

// ---- title row ------------------------------------------------------------------------------

class _TitleRow extends StatelessWidget {
  const _TitleRow({required this.commands, required this.onSearch});
  final List<Command> Function() commands;
  final void Function([String seed]) onSearch;

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final t = context.axiom;
    final c = t.colors;
    Widget menu(String name, String category) => AxMenu(
      label: '$name menu',
      hint: '$name menu',
      entries: () => [
        for (final cmd in commands().where((x) => x.category == category))
          MenuEntry.item(cmd.title, cmd.run, shortcut: cmd.shortcut, enabled: cmd.isEnabled),
      ],
      trigger: (context, s, open) => FocusRing(
        visible: s.focused,
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: open ? c.pressed : (s.hovered ? c.hover : null),
            borderRadius: BorderRadius.circular(Metrics.radiusControl),
          ),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
            child: Text(
              name,
              style: t.type.label.copyWith(color: open || s.hovered ? c.ink : c.ink2),
            ),
          ),
        ),
      ),
    );
    return SizedBox(
      height: Metrics.titleRowHeight,
      child: ColoredBox(
        color: c.bar,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: Metrics.s4),
          child: Row(
            children: [
              AxiomIcon(AxiomIcons.logo, size: 18, color: c.accent),
              const SizedBox(width: Metrics.s4),
              menu('File', 'File'),
              menu('View', 'View'),
              menu('Preferences', 'Preferences'),
              menu('Help', 'Help'),
              const Spacer(),
              Pressable(
                label: 'Search files, run commands',
                hint: 'Search files, run commands — Mod+K',
                onPressed: onSearch,
                builder: (context, s) => FocusRing(
                  visible: s.focused,
                  child: Container(
                    width: 380,
                    height: 26,
                    padding: const EdgeInsets.symmetric(horizontal: 8),
                    decoration: BoxDecoration(
                      color: s.hovered ? Color.alphaBlend(c.hover, c.recessed) : c.recessed,
                      borderRadius: BorderRadius.circular(Metrics.radiusControl),
                      border: Border.all(color: c.hairline),
                    ),
                    child: Row(
                      children: [
                        AxiomIcon(AxiomIcons.search, size: 14, color: c.ink3),
                        const SizedBox(width: 6),
                        Expanded(
                          child: ListenableBuilder(
                            listenable: app.workspace,
                            builder: (context, _) => Text(
                              app.workspace.isOpen
                                  ? 'Search ${app.workspace.name}, run commands'
                                  : 'Search files, run commands',
                              style: t.type.small.copyWith(color: c.ink3),
                              overflow: TextOverflow.ellipsis,
                            ),
                          ),
                        ),
                        const Keycaps('Mod+K'),
                      ],
                    ),
                  ),
                ),
              ),
              const Spacer(),
              AxIconButton(
                icon: AxiomIcons.sidebar,
                label: 'Toggle side bar',
                shortcut: 'Mod+B',
                onPressed: () => app.sideVisible = !app.sideVisible,
              ),
              AxIconButton(
                icon: AxiomIcons.gear,
                label: 'Settings',
                shortcut: 'Mod+,',
                onPressed: () => app.openSettings?.call(),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

// ---- rail ------------------------------------------------------------------------------------

/// The destinations that exist so far (a destination appears with its page).
const _railItems = [(Destination.files, 'Files', AxiomIcons.files, 'Mod+Shift+E')];

class _Rail extends StatefulWidget {
  const _Rail({required this.selected, required this.onSelect});
  final Destination? selected;
  final ValueChanged<Destination> onSelect;

  @override
  State<_Rail> createState() => _RailState();
}

class _RailState extends State<_Rail> with SingleTickerProviderStateMixin {
  late final AnimationController _y = AnimationController.unbounded(vsync: this);
  bool _placed = false;
  static const _itemHeight = 40.0;

  double _target() =>
      _railItems.indexWhere((x) => x.$1 == widget.selected) * _itemHeight + _itemHeight - 7;

  @override
  void didUpdateWidget(_Rail old) {
    super.didUpdateWidget(old);
    if (widget.selected == null || old.selected == widget.selected) return;
    final m = context.axiom.motion;
    if (!_placed || !m.animates) {
      _y.value = _target();
    } else {
      _y.animateWith(SpringSimulation(m.settle, _y.value, _target(), _y.velocity));
    }
    _placed = true;
  }

  @override
  void dispose() {
    _y.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    if (!_placed && widget.selected != null) {
      _y.value = _target();
      _placed = true;
    }
    return SizedBox(
      width: Metrics.railWidth,
      child: ColoredBox(
        color: c.bar,
        child: Semantics(
          label: 'Activity',
          container: true,
          child: Stack(
            children: [
              Column(
                children: [
                  for (final (d, label, icon, shortcut) in _railItems)
                    SizedBox(
                      height: _itemHeight,
                      child: Center(
                        child: AxIconButton(
                          icon: icon,
                          label: label,
                          shortcut: shortcut,
                          selected: widget.selected == d,
                          size: 32,
                          onPressed: () => widget.onSelect(d),
                        ),
                      ),
                    ),
                ],
              ),
              if (widget.selected != null)
                AnimatedBuilder(
                  animation: _y,
                  builder: (context, _) => Positioned(
                    top: _y.value,
                    left: (Metrics.railWidth - Metrics.underlineWidth) / 2,
                    width: Metrics.underlineWidth,
                    height: Metrics.underlineHeight,
                    child: IgnorePointer(
                      child: DecoratedBox(
                        decoration: BoxDecoration(
                          color: c.ink,
                          borderRadius: BorderRadius.circular(1),
                        ),
                      ),
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

// ---- side bar --------------------------------------------------------------------------------

class _SideBar extends StatelessWidget {
  const _SideBar({required this.destination});
  final Destination destination;

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final t = context.axiom;
    final c = t.colors;
    return ColoredBox(
      color: c.bar,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          SizedBox(
            height: 32,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: Metrics.s5),
              child: Row(
                children: [
                  Expanded(
                    child: ListenableBuilder(
                      listenable: app.workspace,
                      builder: (context, _) => Semantics(
                        header: true,
                        child: Text(
                          app.workspace.isOpen ? app.workspace.name : 'Files',
                          style: t.type.label,
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
          const Expanded(child: FilesPage()),
        ],
      ),
    );
  }
}

// ---- main area -------------------------------------------------------------------------------

class _Main extends StatelessWidget {
  const _Main();

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final t = context.axiom;
    return ListenableBuilder(
      listenable: Listenable.merge([app.workspace, app.client.state]),
      builder: (context, _) {
        final ws = app.workspace;
        final daemon = app.client.state.value;
        if (daemon is DaemonFailed) {
          return _Centered(
            children: [
              const AnswerLine('The backend stopped'),
              const SizedBox(height: Metrics.s4),
              ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 640),
                child: Text(daemon.message, style: t.type.codeSmall),
              ),
              const SizedBox(height: Metrics.s5),
              AxButton(
                label: 'Restart the Backend',
                onPressed: () => unawaited(app.restartBackend()),
              ),
            ],
          );
        }
        if (ws.error.isNotEmpty) {
          return _Centered(
            children: [
              AnswerLine(ws.error),
              const SizedBox(height: Metrics.s5),
              AxButton(label: 'Dismiss', kind: ButtonKind.quiet, onPressed: ws.dismissError),
            ],
          );
        }
        if (ws.isOpen) {
          return _Centered(
            children: [
              AnswerLine(ws.name, detail: ws.root),
              const SizedBox(height: Metrics.s5),
              Text('Open a file from the explorer.', style: t.type.small),
            ],
          );
        }
        return _Centered(
          children: [
            AnswerLine(
              ws.opening ? 'Opening the folder…' : 'No folder open',
              detail: 'Axiom opens folders through its backend, axiomd.',
            ),
            const SizedBox(height: Metrics.s5),
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                AxButton(
                  label: 'Open Folder…',
                  icon: AxiomIcons.folderOpen,
                  hint: 'Open Folder… — Mod+O',
                  onPressed: daemon is DaemonReady && !ws.opening
                      ? () => unawaited(app.pickFolder?.call())
                      : null,
                ),
                const SizedBox(width: Metrics.s4),
                const Keycaps('Mod+O'),
              ],
            ),
            const SizedBox(height: Metrics.s7),
            const RecentList(),
          ],
        );
      },
    );
  }
}

class _Centered extends StatelessWidget {
  const _Centered({required this.children});
  final List<Widget> children;

  @override
  Widget build(BuildContext context) => Align(
    alignment: const Alignment(-0.2, -0.35),
    child: Padding(
      padding: const EdgeInsets.all(Metrics.s7),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: children,
      ),
    ),
  );
}

// ---- status bar ------------------------------------------------------------------------------

class _StatusBar extends StatelessWidget {
  const _StatusBar();

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final t = context.axiom;
    final c = t.colors;
    return SizedBox(
      height: Metrics.statusBarHeight,
      child: ColoredBox(
        color: c.bar,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: Metrics.s4),
          child: Row(
            children: [
              ValueListenableBuilder(
                valueListenable: app.client.state,
                builder: (context, s, _) {
                  final (dot, text) = switch (s) {
                    DaemonReady(:final serverVersion) => (
                      DotState.ok,
                      'Connected — axiomd $serverVersion',
                    ),
                    DaemonStarting() => (DotState.running, 'Starting the backend…'),
                    DaemonFailed() => (DotState.error, 'The backend stopped'),
                    DaemonStopped() => (DotState.off, 'Backend stopped'),
                  };
                  return Semantics(
                    liveRegion: true,
                    label: text,
                    excludeSemantics: true,
                    child: Row(
                      children: [
                        StatusDot(dot),
                        const SizedBox(width: Metrics.s2),
                        Text(text, style: t.type.tiny),
                      ],
                    ),
                  );
                },
              ),
              const SizedBox(width: Metrics.s6),
              // The hint line: what the hovered or focused control does, and its shortcut.
              Expanded(
                child: ExcludeSemantics(
                  child: ValueListenableBuilder(
                    valueListenable: app.hint,
                    builder: (context, hint, _) {
                      if (hint == null) return const SizedBox.shrink();
                      final i = hint.lastIndexOf(' — ');
                      final what = i < 0 ? hint : hint.substring(0, i);
                      final keys = i < 0 ? null : hint.substring(i + 3);
                      final isKeys =
                          keys != null &&
                          RegExp(
                            r'^(Mod|Ctrl|Shift|Alt|F\d+|Enter|Esc|Tab|Home|End|[A-Z0-9,`\\/.]|[+ ])+$',
                          ).hasMatch(keys);
                      return Row(
                        key: const ValueKey('hint'),
                        children: [
                          Flexible(
                            child: Text(
                              isKeys ? what : hint,
                              style: t.type.tiny,
                              overflow: TextOverflow.ellipsis,
                            ),
                          ),
                          if (isKeys) ...[const SizedBox(width: 6), Keycaps(keys)],
                        ],
                      );
                    },
                  ),
                ),
              ),
              ListenableBuilder(
                listenable: app.workspace,
                builder: (context, _) => app.workspace.isOpen
                    ? Text(app.workspace.root, style: t.type.tiny, overflow: TextOverflow.ellipsis)
                    : const SizedBox.shrink(),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
