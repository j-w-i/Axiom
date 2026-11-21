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
import '../daemon/protocol.g.dart' show Methods, SearchMode, SearchParams;
import '../editor/code_view.dart';
import '../console/console.dart';
import '../editor/editor_area.dart';
import '../files/explorer.dart';
import '../state/documents.dart';
import '../state/settings.dart';
import '../ui/controls.dart';
import '../ui/icon_data.dart';
import '../ui/icons.dart';
import '../ui/overlays.dart';
import '../ui/pressable.dart';
import '../ui/splitter.dart';
import '../ui/surfaces.dart';
import '../ui/text_input.dart';
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
  final _editor = GlobalKey<EditorAreaState>();
  bool _paletteOpen = false;
  String _paletteSeed = '';
  bool _filesMode = false;
  bool _promptShown = false;
  bool _conflictShown = false;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      app.openPalette = () => _showPalette();
      app.openSettings = _showSettings;
      app.pickFolder = _pickFolder;
      app.openGoToFile = _goToFile;
      app.documents.addListener(_onDocuments);
    });
  }

  /// Unsaved-work questions and save conflicts are dialogs.
  void _onDocuments() {
    final d = app.documents;
    if (d.prompt != null && !_promptShown) {
      _promptShown = true;
      final p = d.prompt!;
      final what = p.names.length == 1 ? p.names.first : '${p.names.length} files';
      unawaited(
        showAxDialog<String>(
          context: context,
          label: 'Unsaved changes',
          builder: (context) => _Question(
            title: 'Save the changes to $what?',
            detail: switch (p.kind) {
              'quit' => 'Your changes are lost if you quit without saving.',
              'folder' => 'Your changes are lost if you switch folders without saving.',
              _ => 'Your changes are lost if you close it without saving.',
            },
            actions: const [('save', 'Save'), ('discard', "Don't Save"), ('cancel', 'Cancel')],
          ),
        ).then((a) {
          _promptShown = false;
          unawaited(d.resolvePrompt(a ?? 'cancel'));
        }),
      );
    }
    final conflict = d.conflictName;
    if (conflict != null && !_conflictShown) {
      _conflictShown = true;
      unawaited(
        showAxDialog<String>(
          context: context,
          label: 'Changed on disk',
          builder: (context) => _Question(
            title: '$conflict changed on disk',
            detail: 'Another program changed the file since you opened it. Overwrite it with your version, or reload the version on disk (your changes are lost).',
            actions: const [('overwrite', 'Overwrite'), ('reload', 'Reload'), ('cancel', 'Cancel')],
          ),
        ).then((a) {
          _conflictShown = false;
          unawaited(d.resolveConflict(a ?? 'cancel'));
        }),
      );
    }
  }

  @override
  void dispose() {
    app.documents.removeListener(_onDocuments);
    _sideSplitter.dispose();
    super.dispose();
  }

  Future<void> _pickFolder() async {
    final dir = await getDirectoryPath(confirmButtonText: 'Open');
    if (dir != null) app.documents.requestOpenFolder(dir);
  }

  void _showPalette([String seed = '']) => setState(() {
    _paletteOpen = true;
    _filesMode = false;
    _paletteSeed = seed;
  });

  void _goToFile() {
    if (!app.workspace.isOpen) return;
    setState(() {
      _paletteOpen = true;
      _filesMode = true;
      _paletteSeed = '';
    });
  }

  Future<List<Command>> _files() async {
    try {
      final r = await app.client.call(
        Methods.workspaceSearch,
        SearchParams(workspaceId: app.workspace.id, mode: SearchMode.files, limit: 20000),
      );
      return [
        for (final f in r.files ?? const <String>[])
          Command(id: 'file:$f', title: f, run: () => unawaited(app.documents.open(f))),
      ];
    } on DaemonException {
      return const [];
    }
  }

  void _goToLine() {
    final doc = app.documents.active;
    if (doc == null) return;
    unawaited(
      showAxDialog<int>(
        context: context,
        label: 'Go to line',
        builder: (context) => _GoToLine(lines: doc.controller.lineCount),
      ).then((line) {
        if (line != null) unawaited(app.documents.open(doc.path, line: line, column: 1));
      }),
    );
  }

  Doc? get _focusedDoc {
    final d = app.documents;
    final f = FocusManager.instance.primaryFocus?.debugLabel ?? '';
    if (f.startsWith('editor 2:') && d.split != null) return d.doc(d.split!.active);
    return d.active;
  }

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
        run: app.documents.requestCloseFolder,
      ),
      for (final r in ws.recent.take(8))
        Command(
          id: 'recent:${r.root}',
          title: 'Open Recent: ${r.name}',
          category: 'File',
          run: () => app.documents.requestOpenFolder(r.root),
        ),
      Command(
        id: 'toggle-side',
        title: 'Toggle Side Bar',
        shortcut: 'Mod+B',
        category: 'View',
        run: () => setState(() => app.sideVisible = !app.sideVisible),
      ),
      Command(
        id: 'toggle-console',
        title: 'Toggle Console',
        shortcut: 'Mod+J',
        category: 'View',
        run: () => app.consoleVisible = !app.consoleVisible,
      ),
      Command(
        id: 'problems',
        title: 'Show Problems',
        category: 'View',
        run: () {
          app.prefs.set('console.tab', 'problems');
          app.consoleVisible = true;
        },
      ),
      Command(
        id: 'files',
        title: 'Show Files',
        category: 'View',
        run: () => _go(Destination.files),
      ),
      Command(
        id: 'go-to-file',
        title: 'Go to File…',
        shortcut: 'Mod+P',
        category: 'Go',
        enabled: () => ws.isOpen,
        run: _goToFile,
      ),
      Command(
        id: 'go-to-line',
        title: 'Go to Line…',
        shortcut: 'Ctrl+G',
        category: 'Go',
        enabled: () => app.documents.active != null,
        run: _goToLine,
      ),
      Command(
        id: 'save',
        title: 'Save',
        shortcut: 'Mod+S',
        category: 'File',
        enabled: () => _focusedDoc != null,
        run: () => unawaited(app.documents.save(_focusedDoc!.path)),
      ),
      Command(
        id: 'save-all',
        title: 'Save All',
        shortcut: 'Mod+Alt+S',
        category: 'File',
        enabled: () => app.documents.dirtyPaths.isNotEmpty,
        run: () => unawaited(app.documents.saveAll()),
      ),
      Command(
        id: 'close-editor',
        title: 'Close Editor',
        shortcut: 'Mod+W',
        category: 'File',
        enabled: () => app.documents.active != null,
        run: () => app.documents.close(app.documents.activePath),
      ),
      Command(
        id: 'next-tab',
        title: 'Next Tab',
        shortcut: 'Ctrl+Tab',
        category: 'View',
        run: () => app.documents.cycle(1),
      ),
      Command(
        id: 'prev-tab',
        title: 'Previous Tab',
        shortcut: 'Ctrl+Shift+Tab',
        category: 'View',
        run: () => app.documents.cycle(-1),
      ),
      Command(
        id: 'move-tab-left',
        title: 'Move Tab Left',
        shortcut: 'Ctrl+Shift+PgUp',
        category: 'View',
        run: () {
          final d = app.documents;
          d.moveTab(d.activePath, d.docs.indexWhere((x) => x.path == d.activePath) - 1);
        },
      ),
      Command(
        id: 'move-tab-right',
        title: 'Move Tab Right',
        shortcut: 'Ctrl+Shift+PgDn',
        category: 'View',
        run: () {
          final d = app.documents;
          d.moveTab(d.activePath, d.docs.indexWhere((x) => x.path == d.activePath) + 1);
        },
      ),
      Command(
        id: 'split-right',
        title: 'Split Editor Right',
        shortcut: 'Mod+\\',
        category: 'View',
        enabled: () => app.documents.active != null,
        run: () {
          app.documents.splitRight();
          WidgetsBinding.instance.addPostFrameCallback((_) => _editor.currentState?.focusGroup(2));
        },
      ),
      Command(
        id: 'close-right-group',
        title: 'Close Right Group',
        category: 'View',
        enabled: () => app.documents.split != null,
        run: app.documents.unsplit,
      ),
      Command(
        id: 'focus-left',
        title: 'Focus the Left Editor Group',
        shortcut: 'Mod+1',
        category: 'View',
        run: () => _editor.currentState?.focusGroup(1),
      ),
      Command(
        id: 'focus-right',
        title: 'Focus the Right Editor Group',
        shortcut: 'Mod+2',
        category: 'View',
        run: () => _editor.currentState?.focusGroup(2),
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
        _mod(LogicalKeyboardKey.keyJ): const _CommandIntent('toggle-console'),
        _mod(LogicalKeyboardKey.comma): const _CommandIntent('settings'),
        _mod(LogicalKeyboardKey.keyO): const _CommandIntent('open-folder'),
        _mod(LogicalKeyboardKey.keyP): const _CommandIntent('go-to-file'),
        _mod(LogicalKeyboardKey.keyS): const _CommandIntent('save'),
        SingleActivator(LogicalKeyboardKey.keyS, control: !_isMac, meta: _isMac, alt: true):
            const _CommandIntent('save-all'),
        _mod(LogicalKeyboardKey.keyW): const _CommandIntent('close-editor'),
        const SingleActivator(LogicalKeyboardKey.keyG, control: true): const _CommandIntent(
          'go-to-line',
        ),
        const SingleActivator(LogicalKeyboardKey.tab, control: true): const _CommandIntent(
          'next-tab',
        ),
        const SingleActivator(LogicalKeyboardKey.tab, control: true, shift: true):
            const _CommandIntent('prev-tab'),
        const SingleActivator(LogicalKeyboardKey.pageUp, control: true, shift: true):
            const _CommandIntent('move-tab-left'),
        const SingleActivator(LogicalKeyboardKey.pageDown, control: true, shift: true):
            const _CommandIntent('move-tab-right'),
        _mod(LogicalKeyboardKey.backslash): const _CommandIntent('split-right'),
        _mod(LogicalKeyboardKey.digit1): const _CommandIntent('focus-left'),
        _mod(LogicalKeyboardKey.digit2): const _CommandIntent('focus-right'),
      },
      child: Actions(
        actions: {
          _CommandIntent: CallbackAction<_CommandIntent>(
            onInvoke: (i) {
              if (i.id == 'palette') {
                _showPalette();
              } else {
                final cmd = commands().firstWhere((x) => x.id == i.id);
                if (cmd.isEnabled) cmd.run();
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
                            // Kept mounted while hidden: showing it again costs no rebuild of
                            // the explorer (measured: 20 ms otherwise, with 3000 rows listed).
                            Offstage(
                              offstage: !app.sideVisible,
                              child: SizedBox(
                                width: app.sideVisible ? app.sideWidth : 0,
                                child: TickerMode(
                                  enabled: app.sideVisible,
                                  child: ExcludeFocus(
                                    excluding: !app.sideVisible,
                                    child: _SideBar(destination: app.destination),
                                  ),
                                ),
                              ),
                            ),
                            if (app.sideVisible) ...[
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
                            Expanded(
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.stretch,
                                children: [
                                  Expanded(child: _Main(editor: _editor)),
                                  if (app.consoleVisible) ...[
                                    AxSplitter(
                                      label: 'Resize the console',
                                      vertical: false,
                                      invert: true,
                                      value: app.consoleHeight,
                                      min: 90,
                                      max: 640,
                                      defaultValue: AppState.consoleDefault,
                                      onChanged: (v) => app.consoleHeight = v,
                                    ),
                                    SizedBox(
                                      height: app.consoleHeight,
                                      child: const ConsoleRegion(),
                                    ),
                                  ],
                                ],
                              ),
                            ),
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
              if (_paletteOpen && !_filesMode)
                Palette(
                  commands: commands(),
                  seed: _paletteSeed,
                  onClose: () => setState(() => _paletteOpen = false),
                ),
              if (_paletteOpen && _filesMode)
                Palette(
                  key: const ValueKey('go-to-file'),
                  commands: const [],
                  load: _files,
                  label: 'Go to File',
                  inputLabel: 'File name',
                  placeholder: 'Search files by name',
                  empty: 'No matching file',
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
          Expanded(
            child: FilesPage(
              onOpenFile: (p, {bool focus = true}) =>
                  unawaited(app.documents.open(p, focus: focus)),
            ),
          ),
        ],
      ),
    );
  }
}

// ---- main area -------------------------------------------------------------------------------

class _Main extends StatelessWidget {
  const _Main({required this.editor});
  final GlobalKey<EditorAreaState> editor;

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final t = context.axiom;
    return ListenableBuilder(
      listenable: Listenable.merge([app.workspace, app.client.state, app.documents]),
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
        if (ws.isOpen && app.documents.docs.isNotEmpty) return EditorArea(key: editor);
        if (ws.isOpen) {
          return _Centered(
            children: [
              AnswerLine(ws.name, detail: ws.root),
              const SizedBox(height: Metrics.s5),
              Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text('Open a file from the explorer, or', style: t.type.small),
                  const SizedBox(width: 6),
                  AxButton(
                    label: 'Go to File…',
                    kind: ButtonKind.quiet,
                    hint: 'Go to File… — Mod+P',
                    onPressed: () => app.openGoToFile?.call(),
                  ),
                  const Keycaps('Mod+P'),
                ],
              ),
              if (app.documents.banner.isNotEmpty) ...[
                const SizedBox(height: Metrics.s4),
                Text(
                  app.documents.banner,
                  style: t.type.small.copyWith(color: t.colors.dangerText),
                ),
              ],
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
                listenable: Listenable.merge([app.workspace, app.documents]),
                builder: (context, _) {
                  final doc = app.documents.active;
                  if (doc == null) {
                    return app.workspace.isOpen
                        ? Text(
                            app.workspace.root,
                            style: t.type.tiny,
                            overflow: TextOverflow.ellipsis,
                          )
                        : const SizedBox.shrink();
                  }
                  return Row(
                    children: [
                      const LspStatus(),
                      const SizedBox(width: Metrics.s5),
                      if (app.documents.cursorText.isNotEmpty)
                        Semantics(
                          label: app.documents.cursorText,
                          child: Text(
                            app.documents.cursorText,
                            style: t.type.numeric.copyWith(fontSize: 11),
                          ),
                        ),
                      const SizedBox(width: Metrics.s5),
                      Text(languageName(doc.language), style: t.type.tiny),
                    ],
                  );
                },
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// A question with its actions (unsaved changes, conflicts); pops the chosen action's id.
class _Question extends StatelessWidget {
  const _Question({required this.title, required this.detail, required this.actions});
  final String title;
  final String detail;
  final List<(String, String)> actions;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    return SizedBox(
      width: 460,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(20, 18, 20, 16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Semantics(header: true, child: Text(title, style: t.type.title)),
            const SizedBox(height: Metrics.s4),
            Text(detail, style: t.type.small),
            const SizedBox(height: Metrics.s6),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                for (final (i, (id, label)) in actions.indexed) ...[
                  if (i > 0) const SizedBox(width: Metrics.s3),
                  AxButton(
                    label: label,
                    autofocus: i == 0,
                    kind: id == 'cancel'
                        ? ButtonKind.quiet
                        : (id == 'discard' ? ButtonKind.danger : ButtonKind.standard),
                    onPressed: () => Navigator.of(context).pop(id),
                  ),
                ],
              ],
            ),
          ],
        ),
      ),
    );
  }
}

/// Go to line: a number, Enter goes there.
class _GoToLine extends StatefulWidget {
  const _GoToLine({required this.lines});
  final int lines;

  @override
  State<_GoToLine> createState() => _GoToLineState();
}

class _GoToLineState extends State<_GoToLine> {
  final _text = TextEditingController();

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  void _go() {
    final n = int.tryParse(_text.text.trim());
    if (n == null) return;
    Navigator.of(context).pop(n.clamp(1, widget.lines));
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    return SizedBox(
      width: 360,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Go to line (1–${widget.lines})', style: t.type.label),
            const SizedBox(height: Metrics.s4),
            CallbackShortcuts(
              bindings: {const SingleActivator(LogicalKeyboardKey.enter): _go},
              child: AxTextInput(
                label: 'Line number',
                controller: _text,
                autofocus: true,
                placeholder: 'Line',
                onSubmitted: (_) => _go(),
                inputFormatters: [FilteringTextInputFormatter.digitsOnly],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
