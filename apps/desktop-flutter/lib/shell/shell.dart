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
import '../chat/chat_page.dart';
import '../daemon/client.dart';
import '../daemon/protocol.g.dart' show Methods, SearchMode, SearchParams;
import '../console/console.dart';
import '../editor/editor_area.dart';
import '../editor/languages.dart';
import '../files/explorer.dart';
import '../files/file_actions.dart';
import '../state/documents.dart';
import '../state/settings.dart';
import '../state/tasks.dart';
import '../console/output.dart';
import '../env/env_page.dart';
import '../inspector/inspector_page.dart';
import '../scm/scm_page.dart';
import '../search/search_page.dart';
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

  /// A list from the editor (references, symbols, code actions) shown in the palette.
  ({String label, String inputLabel, String placeholder, String empty, List<Command> items})?
  _picker;
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
      app.shellShortcut = _dispatch;
      app.openPicker =
          ({
            required String label,
            required String inputLabel,
            required String placeholder,
            required String empty,
            required List<Command> items,
          }) => setState(() {
            _picker = (
              label: label,
              inputLabel: inputLabel,
              placeholder: placeholder,
              empty: empty,
              items: items,
            );
          });
      app.showDock = _showDock;
      app.documents.addListener(_onDocuments);
      app.environments.addListener(_onEnvironments);
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
          builder: (context) => AxQuestion(
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
          label: d.conflictIsDeleted ? 'Deleted on disk' : 'Changed on disk',
          // A file deleted on disk has no version to reload: save it again, or close it (AUD-019).
          builder: (context) => d.conflictIsDeleted
              ? AxQuestion(
                  title: '$conflict was deleted on disk',
                  detail: 'Another program deleted the file since you opened it. Save your version to create it again, or close it (your changes are lost).',
                  actions: const [
                    ('overwrite', 'Save'),
                    ('close', 'Close Without Saving'),
                    ('cancel', 'Cancel'),
                  ],
                  defaultAction: 'cancel',
                )
              : AxQuestion(
                  title: '$conflict changed on disk',
                  detail: 'Another program changed the file since you opened it. Overwrite it with your version, or reload the version on disk (your changes are lost).',
                  actions: const [
                    ('overwrite', 'Overwrite'),
                    ('reload', 'Reload'),
                    ('cancel', 'Cancel'),
                  ],
                  // Enter must not overwrite another program's changes (AUD-018).
                  defaultAction: 'cancel',
                ),
        ).then((a) {
          _conflictShown = false;
          unawaited(d.resolveConflict(a ?? 'cancel'));
        }),
      );
    }
  }

  bool _planShown = false;

  /// An install plan is shown for approval wherever it was asked for (the Environments page, the
  /// editor's language server line); one Axiom approves itself (auto-install) is not.
  void _onEnvironments() {
    final env = app.environments;
    final p = env.plan;
    if (p == null || _planShown || !mounted || env.planIsAutomatic) return;
    _planShown = true;
    unawaited(
      showAxDialog<bool>(
        context: context,
        label: 'Install ${p.providerName} ${p.plan.version}',
        builder: (_) => PlanDialog(plan: p.plan, providerName: p.providerName),
      ).then((install) {
        _planShown = false;
        if (install == true) {
          unawaited(env.approvePlan());
        } else {
          env.dismissPlan();
        }
      }),
    );
  }

  @override
  void dispose() {
    app.environments.removeListener(_onEnvironments);
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
    if (_editor.currentState?.focusedGroup() == 2 && d.split != null) {
      return d.doc(d.split!.active);
    }
    return d.active;
  }

  void _showSettings([String? section]) {
    unawaited(
      showAxDialog<void>(
        context: context,
        label: 'Settings',
        builder: (_) => SettingsPage(app: app, section: section ?? 'appearance'),
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
        shortcut: 'Mod+Shift+E',
        run: () => _go(Destination.files),
      ),
      Command(
        id: 'search',
        title: 'Search in Files',
        shortcut: 'Mod+Shift+F',
        category: 'View',
        enabled: () => ws.isOpen,
        run: () {
          _go(Destination.search);
          WidgetsBinding.instance.addPostFrameCallback((_) => app.focusSearch?.call());
        },
      ),
      Command(
        id: 'inspector',
        title: 'Show Inspector',
        category: 'View',
        run: () => _go(Destination.inspector),
      ),
      Command(
        id: 'environments',
        title: 'Show Environments',
        category: 'View',
        run: () => _go(Destination.environments),
      ),
      Command(
        id: 'scm',
        title: 'Show Source Control',
        shortcut: 'Mod+Shift+G',
        category: 'View',
        run: () => _go(Destination.scm),
      ),
      Command(id: 'chat', title: 'Show Chat', category: 'View', run: () => _go(Destination.chat)),
      Command(
        id: 'run-command',
        title: 'Run Command…',
        shortcut: 'Mod+Shift+R',
        category: 'Run',
        enabled: () => ws.isOpen,
        run: () {
          app.prefs.set('console.tab', 'output');
          app.consoleVisible = true;
          WidgetsBinding.instance.addPostFrameCallback((_) => app.focusRunPrompt?.call());
        },
      ),
      Command(
        id: 'rerun',
        title: app.tasks.lastLabel.isEmpty ? 'Rerun Last' : 'Rerun: ${app.tasks.lastLabel}',
        shortcut: 'F5',
        category: 'Run',
        enabled: () => app.tasks.canRun && app.tasks.lastLabel.isNotEmpty,
        run: () {
          app.prefs.set('console.tab', 'output');
          app.consoleVisible = true;
          app.tasks.rerunLast();
        },
      ),
      Command(
        id: 'stop',
        title: 'Stop',
        shortcut: 'Shift+F5',
        category: 'Run',
        enabled: () => app.tasks.running,
        run: app.tasks.stop,
      ),
      for (final t in app.tasks.providerTasks)
        Command(
          id: 'task:${t.providerId}:${t.id}',
          title: providerTaskTitle(t),
          category: 'Run',
          enabled: () => app.tasks.canRun,
          run: () {
            app.prefs.set('console.tab', 'output');
            app.consoleVisible = true;
            app.tasks.runProvider(t);
          },
        ),
      Command(
        id: 'new-terminal',
        title: 'New Terminal',
        shortcut: 'Mod+Shift+`',
        category: 'View',
        enabled: () => ws.isOpen,
        run: () {
          app.prefs.set('console.tab', 'terminal');
          app.consoleVisible = true;
          unawaited(_newTerminal());
        },
      ),
      Command(
        id: 'show-terminal',
        title: 'Show Terminal',
        category: 'View',
        enabled: () => ws.isOpen,
        run: () {
          app.prefs.set('console.tab', 'terminal');
          app.consoleVisible = true;
          WidgetsBinding.instance.addPostFrameCallback((_) => app.focusTerminal?.call());
        },
      ),
      Command(
        id: 'focus-console',
        title: 'Focus the Console',
        shortcut: 'Mod+`',
        category: 'View',
        run: () {
          app.consoleVisible = true;
          WidgetsBinding.instance.addPostFrameCallback((_) {
            switch (app.prefs.string('console.tab', 'problems')) {
              case 'terminal':
                app.focusTerminal?.call();
              case 'output':
                app.focusRunPrompt?.call();
              default:
                break;
            }
          });
        },
      ),
      Command(
        id: 'toggle-dock',
        title: 'Toggle AI Dock',
        shortcut: 'Mod+L',
        category: 'View',
        enabled: () => ws.isOpen,
        run: _toggleDock,
      ),
      Command(
        id: 'new-chat',
        title: 'New Chat',
        shortcut: 'Mod+N',
        category: 'View',
        run: () {
          app.agent.newChat();
          _go(Destination.chat);
        },
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
      // The Edit menu: the focused editor's edits, also on their keys (AUD-111).
      for (final (id, title, keys, act) in <(String, String, String, void Function(Doc))>[
        ('undo', 'Undo', 'Mod+Z', (d) => d.controller.undo()),
        ('redo', 'Redo', 'Mod+Shift+Z', (d) => d.controller.redo()),
        ('cut', 'Cut', 'Mod+X', (d) => d.controller.cut()),
        ('copy', 'Copy', 'Mod+C', (d) => d.controller.copy()),
        ('paste', 'Paste', 'Mod+V', (d) => d.controller.paste()),
        ('select-all', 'Select All', 'Mod+A', (d) => d.controller.selectAll()),
      ])
        Command(
          id: 'edit-$id',
          title: title,
          shortcut: keys,
          category: 'Edit',
          enabled: () =>
              _focusedDoc != null && (!_focusedDoc!.readOnly || id == 'copy' || id == 'select-all'),
          run: () {
            final d = _focusedDoc;
            if (d == null) return;
            act(d);
            _editor.currentState?.focusGroup(_editor.currentState?.focusedGroup() ?? 1);
          },
        ),
      // Files and folders (AUD-022, protocol 0.6).
      Command(
        id: 'new-file',
        title: 'New File…',
        category: 'File',
        enabled: () => ws.isOpen,
        run: () => unawaited(newFile(context, app)),
      ),
      Command(
        id: 'new-folder',
        title: 'New Folder…',
        category: 'File',
        enabled: () => ws.isOpen,
        run: () => unawaited(newFolder(context, app)),
      ),
      Command(
        id: 'save-as',
        title: 'Save As…',
        shortcut: 'Mod+Shift+S',
        category: 'File',
        enabled: () => _focusedDoc != null && !_focusedDoc!.readOnly,
        run: () => unawaited(saveAs(context, app, _focusedDoc!)),
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

  /// A new terminal in the folder, focused once the Terminal view shows it.
  Future<void> _newTerminal() async {
    final id = await app.terminals.create();
    if (id != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) => app.focusTerminal?.call());
    }
  }

  /// Opens the dock beside the editor and focuses it (the Inspector's "Ask AI").
  void _showDock() {
    if (!app.workspace.isOpen) return;
    setState(() {
      if (app.destination == Destination.chat) app.destination = Destination.files;
      app.dockVisible = true;
    });
    WidgetsBinding.instance.addPostFrameCallback((_) => app.focusDock?.call());
  }

  /// The dock opens beside the editor (leaving Chat for the files when Chat is showing).
  void _toggleDock() {
    final open = app.dockVisible && app.destination != Destination.chat;
    setState(() {
      if (app.destination == Destination.chat) app.destination = Destination.files;
      app.dockVisible = !open;
    });
    if (open) {
      _editor.currentState?.focusGroup(1);
    } else {
      app.focusDock?.call();
    }
  }

  void _go(Destination d) {
    setState(() {
      app.destination = d;
      app.sideVisible = true;
    });
    if (d == Destination.chat) app.focusComposer?.call();
  }

  /// The window's keys.
  late final Map<ShortcutActivator, Intent> _shortcuts = {
    _mod(LogicalKeyboardKey.keyK): const _CommandIntent('palette'),
    _mod(LogicalKeyboardKey.keyP, shift: true): const _CommandIntent('palette'),
    _mod(LogicalKeyboardKey.keyB): const _CommandIntent('toggle-side'),
    _mod(LogicalKeyboardKey.keyE, shift: true): const _CommandIntent('files'),
    _mod(LogicalKeyboardKey.keyL): const _CommandIntent('toggle-dock'),
    _mod(LogicalKeyboardKey.keyN): const _CommandIntent('new-chat'),
    _mod(LogicalKeyboardKey.keyJ): const _CommandIntent('toggle-console'),
    _mod(LogicalKeyboardKey.comma): const _CommandIntent('settings'),
    _mod(LogicalKeyboardKey.keyO): const _CommandIntent('open-folder'),
    _mod(LogicalKeyboardKey.keyP): const _CommandIntent('go-to-file'),
    _mod(LogicalKeyboardKey.keyS): const _CommandIntent('save'),
    _mod(LogicalKeyboardKey.keyS, shift: true): const _CommandIntent('save-as'),
    SingleActivator(LogicalKeyboardKey.keyS, control: !_isMac, meta: _isMac, alt: true):
        const _CommandIntent('save-all'),
    _mod(LogicalKeyboardKey.keyW): const _CommandIntent('close-editor'),
    const SingleActivator(LogicalKeyboardKey.keyG, control: true): const _CommandIntent(
      'go-to-line',
    ),
    const SingleActivator(LogicalKeyboardKey.tab, control: true): const _CommandIntent('next-tab'),
    const SingleActivator(LogicalKeyboardKey.tab, control: true, shift: true): const _CommandIntent(
      'prev-tab',
    ),
    const SingleActivator(LogicalKeyboardKey.pageUp, control: true, shift: true):
        const _CommandIntent('move-tab-left'),
    const SingleActivator(LogicalKeyboardKey.pageDown, control: true, shift: true):
        const _CommandIntent('move-tab-right'),
    _mod(LogicalKeyboardKey.backslash): const _CommandIntent('split-right'),
    _mod(LogicalKeyboardKey.digit1): const _CommandIntent('focus-left'),
    _mod(LogicalKeyboardKey.digit2): const _CommandIntent('focus-right'),
    _mod(LogicalKeyboardKey.keyR, shift: true): const _CommandIntent('run-command'),
    _mod(LogicalKeyboardKey.keyF, shift: true): const _CommandIntent('search'),
    _mod(LogicalKeyboardKey.keyG, shift: true): const _CommandIntent('scm'),
    const SingleActivator(LogicalKeyboardKey.f5): const _CommandIntent('rerun'),
    const SingleActivator(LogicalKeyboardKey.f5, shift: true): const _CommandIntent('stop'),
    _mod(LogicalKeyboardKey.backquote, shift: true): const _CommandIntent('new-terminal'),
    _mod(LogicalKeyboardKey.backquote): const _CommandIntent('focus-console'),
  };

  void _invoke(String id) {
    if (id == 'palette') {
      _showPalette();
      return;
    }
    final cmd = commands().where((x) => x.id == id).firstOrNull;
    if (cmd != null && cmd.isEnabled) cmd.run();
  }

  /// The command bound to [e], run (the terminal hands the window its chords).
  bool _dispatch(KeyEvent e) {
    for (final MapEntry(key: a, value: i) in _shortcuts.entries) {
      if (i is _CommandIntent && a.accepts(e, HardwareKeyboard.instance)) {
        _invoke(i.id);
        return true;
      }
    }
    return false;
  }

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Shortcuts(
      shortcuts: _shortcuts,
      child: Actions(
        actions: {
          _CommandIntent: CallbackAction<_CommandIntent>(
            onInvoke: (i) {
              _invoke(i.id);
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
                        listenable: Listenable.merge([app.prefs, app.workspace]),
                        builder: (context, _) {
                          // The stored panel sizes are what the person chose; what is shown
                          // fits the window, the editor keeping its minimum (AUD-014).
                          final win = MediaQuery.sizeOf(context);
                          final dockOn =
                              app.dockVisible &&
                              app.workspace.isOpen &&
                              app.destination != Destination.chat;
                          final fit = fitPanels(
                            width: win.width - Metrics.railWidth - 1,
                            height:
                                win.height - Metrics.titleRowHeight - Metrics.statusBarHeight - 2,
                            side: app.sideVisible ? app.sideWidth : 0,
                            dock: dockOn ? app.dockWidth : 0,
                            console: app.consoleVisible ? app.consoleHeight : 0,
                          );
                          return Row(
                            crossAxisAlignment: CrossAxisAlignment.stretch,
                            children: [
                              _Rail(
                                // Chat owns the main area, so it stays marked with the side bar hidden.
                                selected: app.sideVisible || app.destination == Destination.chat
                                    ? app.destination
                                    : null,
                                onSelect: (d) {
                                  setState(() {
                                    if (app.destination == d && app.sideVisible) {
                                      app.sideVisible = false;
                                    } else {
                                      app.destination = d;
                                      app.sideVisible = true;
                                    }
                                  });
                                  if (d == Destination.chat && app.destination == d) {
                                    app.focusComposer?.call();
                                  }
                                },
                              ),
                              const Groove(vertical: true),
                              // Kept mounted while hidden: showing it again costs no rebuild of
                              // the explorer (measured: 20 ms otherwise, with 3000 rows listed).
                              Offstage(
                                offstage: !app.sideVisible,
                                child: SizedBox(
                                  width: app.sideVisible ? fit.side : 0,
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
                                  value: fit.side,
                                  min: fit.sideMin,
                                  max: fit.sideMax,
                                  defaultValue: AppState.sideDefault,
                                  focusNode: _sideSplitter,
                                  onChanged: (v) => app.sideWidth = v,
                                ),
                              ],
                              Expanded(
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.stretch,
                                  children: [
                                    Expanded(
                                      child: _Main(
                                        editor: _editor,
                                        chat: app.destination == Destination.chat,
                                        environments: app.destination == Destination.environments,
                                      ),
                                    ),
                                    if (app.consoleVisible) ...[
                                      AxSplitter(
                                        label: 'Resize the console',
                                        vertical: false,
                                        invert: true,
                                        value: fit.console,
                                        min: fit.consoleMin,
                                        max: fit.consoleMax,
                                        defaultValue: AppState.consoleDefault,
                                        onChanged: (v) => app.consoleHeight = v,
                                      ),
                                      SizedBox(height: fit.console, child: const ConsoleRegion()),
                                    ],
                                  ],
                                ),
                              ),
                              if (app.dockVisible &&
                                  app.workspace.isOpen &&
                                  app.destination != Destination.chat) ...[
                                AxSplitter(
                                  label: 'Resize the AI dock',
                                  value: fit.dock,
                                  min: fit.dockMin,
                                  max: fit.dockMax,
                                  invert: true,
                                  defaultValue: AppState.dockDefault,
                                  onChanged: (v) => app.dockWidth = v,
                                ),
                                SizedBox(width: fit.dock, child: const ChatView(dock: true)),
                              ],
                            ],
                          );
                        },
                      ),
                    ),
                    const Groove(),
                    const _StatusBar(),
                  ],
                ),
              ),
              const Positioned(right: 0, bottom: 0, child: SheetWarmup()),
              Positioned(
                right: 16,
                bottom: 36,
                width: 440,
                child: ListenableBuilder(
                  listenable: Listenable.merge([app.prefs, app.workspace]),
                  builder: (context, _) => ApprovalsStack(
                    chatShowing:
                        app.destination == Destination.chat ||
                        (app.dockVisible && app.workspace.isOpen),
                    onShow: (id) {
                      final a = app.agent;
                      if (a.activeId != id) unawaited(a.open(id));
                      _go(Destination.chat);
                    },
                  ),
                ),
              ),
              if (_paletteOpen && !_filesMode)
                Palette(
                  commands: commands(),
                  seed: _paletteSeed,
                  onClose: () => setState(() => _paletteOpen = false),
                ),
              if (_picker case final p?)
                Palette(
                  key: ValueKey('picker ${p.label}'),
                  commands: p.items,
                  label: p.label,
                  inputLabel: p.inputLabel,
                  placeholder: p.placeholder,
                  empty: p.empty,
                  onClose: () => setState(() => _picker = null),
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

/// The panel sizes shown in a [width] × [height] work area (beside the rail, between the title
/// row and the status bar), from the stored ones: the editor keeps [editorMinWidth] ×
/// [editorMinHeight] while the panels can shrink to their minimums (the dock first, then the side
/// bar; then both proportionally), and every splitter's range stays inside what fits (AUD-014).
typedef PanelFit = ({
  double side,
  double sideMin,
  double sideMax,
  double dock,
  double dockMin,
  double dockMax,
  double console,
  double consoleMin,
  double consoleMax,
});

const editorMinWidth = 320.0;
// Tabs, breadcrumbs and up to two banner lines, and still some code (AUD-014).
const editorMinHeight = 240.0;

PanelFit fitPanels({
  required double width,
  required double height,
  required double side,
  required double dock,
  required double console,
}) {
  const splitter = 5.0;
  double nonNeg(double v) => v.isFinite && v > 0 ? v : 0;
  final room = nonNeg(
    width - editorMinWidth - (side > 0 ? splitter : 0) - (dock > 0 ? splitter : 0),
  );
  var s = side, d = dock;
  if (s + d > room) {
    // The dock gives way first, down to its minimum; then the side bar, down to its; then both.
    d = d > 0 ? (room - s).clamp(300.0, d) : 0;
    if (s + d > room) s = s > 0 ? (room - d).clamp(180.0, s) : 0;
    if (s + d > room && s + d > 0) {
      final k = room / (s + d);
      s *= k;
      d *= k;
    }
  }
  final sideMax = nonNeg(room - d).clamp(0.0, 520.0);
  final sideMin = sideMax < 180 ? sideMax : 180.0;
  final dockMax = nonNeg(room - s).clamp(0.0, 720.0);
  final dockMin = dockMax < 300 ? dockMax : 300.0;
  final consoleRoom = nonNeg(height - editorMinHeight - splitter);
  final consoleMax = consoleRoom.clamp(0.0, 640.0);
  final consoleMin = consoleMax < 90 ? consoleMax : 90.0;
  return (
    side: s.clamp(0.0, side > 0 ? sideMax : 0.0),
    sideMin: sideMin,
    sideMax: sideMax,
    dock: d.clamp(0.0, dock > 0 ? dockMax : 0.0),
    dockMin: dockMin,
    dockMax: dockMax,
    console: console > 0 ? console.clamp(consoleMin, consoleMax) : 0,
    consoleMin: consoleMin,
    consoleMax: consoleMax,
  );
}

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
            color: open || s.hovered ? c.hover : null,
            borderRadius: BorderRadius.circular(Metrics.radiusControl),
          ),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 5),
            child: Text(
              name,
              style: t.type.small.copyWith(color: open || s.hovered ? c.ink : c.ink2),
            ),
          ),
        ),
      ),
    );
    return SizedBox(
      height: context.scaled(Metrics.titleRowHeight, fontSize: 13, lineHeight: 18),
      child: ColoredBox(
        color: c.bar,
        child: Padding(
          padding: const EdgeInsets.fromLTRB(Metrics.s5, 0, Metrics.s4, 0),
          // Tauri's narrow-window rules: under 1040 px the command field becomes an icon button
          // and the folder's path goes; everything else gives way before the row overflows.
          child: LayoutBuilder(
            builder: (context, box) {
              final narrow = box.maxWidth < 1040;
              return Row(
                children: [
                  // The brand: the brass mark and the word (Tauri's title row).
                  ExcludeSemantics(
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        AxiomIcon(AxiomIcons.logo, size: 17, color: c.accent),
                        const SizedBox(width: 7),
                        Text('Axiom', style: t.type.body.copyWith(fontWeight: FontWeight.w600)),
                      ],
                    ),
                  ),
                  const SizedBox(width: Metrics.s4),
                  menu('File', 'File'),
                  menu('Edit', 'Edit'),
                  menu('View', 'View'),
                  menu('Preferences', 'Preferences'),
                  menu('Help', 'Help'),
                  // The open folder: its name and, quieter, its path.
                  ListenableBuilder(
                    listenable: app.workspace,
                    builder: (context, _) {
                      final ws = app.workspace;
                      if (!ws.isOpen) return const SizedBox.shrink();
                      return Flexible(
                        child: LayoutBuilder(
                          builder: (context, seg) => seg.maxWidth < 64
                              ? const SizedBox.shrink()
                              : Row(
                                  mainAxisSize: MainAxisSize.min,
                                  children: [
                                    const SizedBox(width: Metrics.s2),
                                    SizedBox(
                                      height: 18,
                                      width: 1,
                                      child: ColoredBox(color: c.groove),
                                    ),
                                    const SizedBox(width: Metrics.s5),
                                    // Both give way when the row is crowded: the path first, then the
                                    // name, which ellipsises rather than overflowing.
                                    Flexible(
                                      flex: 2,
                                      child: Text(
                                        ws.name,
                                        style: t.type.body.copyWith(fontWeight: FontWeight.w600),
                                        overflow: TextOverflow.ellipsis,
                                        maxLines: 1,
                                        softWrap: false,
                                      ),
                                    ),
                                    if (!narrow) const SizedBox(width: 7),
                                    if (!narrow)
                                      Flexible(
                                        child: ConstrainedBox(
                                          constraints: const BoxConstraints(maxWidth: 240),
                                          child: Text(
                                            ws.root,
                                            style: t.type.codeSmall.copyWith(
                                              fontSize: 11,
                                              color: c.ink3,
                                            ),
                                            overflow: TextOverflow.ellipsis,
                                            maxLines: 1,
                                            softWrap: false,
                                          ),
                                        ),
                                      ),
                                  ],
                                ),
                        ),
                      );
                    },
                  ),
                  const Spacer(),
                  if (narrow)
                    AxIconButton(
                      icon: AxiomIcons.search,
                      label: 'Search files, run commands',
                      shortcut: 'Mod+K',
                      onPressed: onSearch,
                    )
                  else
                    Pressable(
                      label: 'Search files, run commands',
                      hint: 'Search files, run commands — Mod+K',
                      onPressed: onSearch,
                      builder: (context, s) => FocusRing(
                        visible: s.focused,
                        child: SizedBox(
                          width: (box.maxWidth * 0.32).clamp(180.0, 380.0),
                          height: 26,
                          child: Well(
                            radius: Metrics.radiusRaised,
                            border: s.hovered ? c.lineInput : null,
                            child: Padding(
                              padding: const EdgeInsets.fromLTRB(8, 0, 4, 0),
                              child: Row(
                                children: [
                                  AxiomIcon(
                                    AxiomIcons.search,
                                    size: 14,
                                    color: s.hovered ? c.ink2 : c.ink3,
                                  ),
                                  const SizedBox(width: 8),
                                  Expanded(
                                    child: Text(
                                      'Search files, run commands',
                                      style: t.type.small.copyWith(
                                        color: s.hovered ? c.ink2 : c.ink3,
                                      ),
                                      overflow: TextOverflow.ellipsis,
                                    ),
                                  ),
                                  const Keycaps('Mod+K'),
                                ],
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                  const SizedBox(width: Metrics.s4),
                  const Transport(),
                  const Spacer(),
                  ListenableBuilder(
                    listenable: app.prefs,
                    builder: (context, _) => Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        AxIconButton(
                          icon: AxiomIcons.sidebar,
                          label: 'Toggle side bar',
                          shortcut: 'Mod+B',
                          selected: app.sideVisible,
                          onPressed: () => app.sideVisible = !app.sideVisible,
                        ),
                        const SizedBox(width: 2),
                        AxIconButton(
                          icon: AxiomIcons.panel,
                          label: 'Toggle console',
                          shortcut: 'Mod+J',
                          selected: app.consoleVisible,
                          onPressed: () => app.consoleVisible = !app.consoleVisible,
                        ),
                        const SizedBox(width: 2),
                      ],
                    ),
                  ),
                  AxIconButton(
                    icon: AxiomIcons.gear,
                    label: 'Settings',
                    shortcut: 'Mod+,',
                    onPressed: () => app.openSettings?.call(),
                  ),
                ],
              );
            },
          ),
        ),
      ),
    );
  }
}

// ---- rail ------------------------------------------------------------------------------------

/// The destinations that exist so far (a destination appears with its page).
const _railItems = <(Destination, String, IconData16, String?)>[
  (Destination.files, 'Files', AxiomIcons.files, 'Mod+Shift+E'),
  (Destination.chat, 'Chat', AxiomIcons.chat, null),
  (Destination.search, 'Search', AxiomIcons.search, 'Mod+Shift+F'),
  (Destination.scm, 'Source Control', AxiomIcons.branch, 'Mod+Shift+G'),
  (Destination.environments, 'Environments', AxiomIcons.layers, null),
  (Destination.inspector, 'Inspector', AxiomIcons.inspect, null),
];

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
  // Tauri's rail: 34 px destinations 4 px apart below 8 px of padding; the selected one has a
  // 2 px brass marker on the window's edge, 7 px in from its top and bottom.
  static const _itemHeight = 38.0;
  static const _top = 8.0;

  double _target() =>
      _top + _railItems.indexWhere((x) => x.$1 == widget.selected) * _itemHeight + 2 + 7;

  @override
  void didUpdateWidget(_Rail old) {
    super.didUpdateWidget(old);
    if (widget.selected == null || old.selected == widget.selected) return;
    final m = context.axiom.motion;
    if (!_placed || !m.glides) {
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
                  const SizedBox(height: _top),
                  for (final (d, label, icon, shortcut) in _railItems)
                    SizedBox(
                      height: _itemHeight,
                      child: d == Destination.chat || d == Destination.environments
                          // Chat carries the approvals waiting for the person; Environments
                          // the toolchains that need them.
                          ? ListenableBuilder(
                              listenable: d == Destination.chat
                                  ? AppScope.of(context).agent
                                  : AppScope.of(context).environments,
                              builder: (context, _) {
                                final app = AppScope.of(context);
                                final chat = d == Destination.chat;
                                final n = chat
                                    ? app.agent.approvals.length
                                    : app.environments.needAttention;
                                return Stack(
                                  clipBehavior: Clip.none,
                                  children: [
                                    Center(
                                      child: AxIconButton(
                                        // Beside the rail, not under it (AUD-051).
                                        tooltipSide: OverlaySide.right,
                                        icon: icon,
                                        label: chat
                                            ? (n == 0 ? label : '$label, $n waiting for you')
                                            : environmentsRailLabel(n),
                                        shortcut: shortcut,
                                        selected: widget.selected == d,
                                        engaged: false,
                                        color: c.ink3,
                                        size: 34,
                                        onPressed: () => widget.onSelect(d),
                                      ),
                                    ),
                                    if (n > 0)
                                      Positioned(
                                        right: 6,
                                        top: 4,
                                        child: IgnorePointer(child: AxBadge(n)),
                                      ),
                                  ],
                                );
                              },
                            )
                          : Center(
                              child: AxIconButton(
                                tooltipSide: OverlaySide.right,
                                icon: icon,
                                label: label,
                                shortcut: shortcut,
                                selected: widget.selected == d,
                                engaged: false,
                                color: c.ink3,
                                size: 34,
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
                    left: 0,
                    width: 2,
                    height: 34 - 14,
                    child: IgnorePointer(
                      child: DecoratedBox(
                        decoration: BoxDecoration(
                          color: c.accent,
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

class _SideBar extends StatefulWidget {
  const _SideBar({required this.destination});
  final Destination destination;

  @override
  State<_SideBar> createState() => _SideBarState();
}

/// Each destination's page stays mounted once shown: switching back costs no rebuild.
class _SideBarState extends State<_SideBar> {
  final _shown = <Destination>{};
  final _fileMenu = GlobalKey<AxMenuState>();

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      AppScope.of(context).openFileMenu = () => _fileMenu.currentState?.open();
    });
  }

  Widget _page(AppState app, Destination d) => switch (d) {
    Destination.chat => const ChatSide(),
    Destination.search => const SearchSide(),
    Destination.scm => const ScmSide(),
    Destination.environments => const EnvironmentsSide(),
    Destination.inspector => const InspectorSide(),
    _ => FilesPage(
      onOpenFile: (p, {bool focus = true}) => unawaited(app.documents.open(p, focus: focus)),
    ),
  };

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final t = context.axiom;
    final c = t.colors;
    final d = widget.destination;
    _shown.add(d);
    return ColoredBox(
      color: c.panel,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          // Tauri's panel head: 30 px, a small strong title and quieter meta, a seam below.
          SizedBox(
            height: Metrics.headHeight,
            child: Padding(
              padding: const EdgeInsets.fromLTRB(Metrics.s5, 0, Metrics.s2, 0),
              child: ListenableBuilder(
                listenable: app.workspace,
                builder: (context, _) {
                  final title = switch (d) {
                    Destination.chat => 'Chat',
                    Destination.search => 'Search',
                    Destination.scm => 'Source Control',
                    Destination.environments => 'Environments',
                    Destination.inspector => 'Inspector',
                    _ => 'Files',
                  };
                  final meta = app.workspace.isOpen ? app.workspace.name : null;
                  final files = d == Destination.files && app.workspace.isOpen;
                  return Row(
                    children: [
                      Semantics(
                        header: true,
                        child: Text(
                          title,
                          style: t.type.small.copyWith(color: c.ink, fontWeight: FontWeight.w600),
                        ),
                      ),
                      const SizedBox(width: Metrics.s4),
                      Expanded(
                        child: meta == null
                            ? const SizedBox.shrink()
                            : Text(meta, style: t.type.small, overflow: TextOverflow.ellipsis),
                      ),
                      // The folder's file operations (AUD-022): new file, new folder, and the
                      // selected row's menu (also on a right-click, Shift+F10, F2, Delete).
                      if (files) ...[
                        AxIconButton(
                          icon: AxiomIcons.plus,
                          label: 'New File…',
                          size: 24,
                          onPressed: () => unawaited(newFile(context, app)),
                        ),
                        AxIconButton(
                          icon: AxiomIcons.folder,
                          label: 'New Folder…',
                          size: 24,
                          onPressed: () => unawaited(newFolder(context, app)),
                        ),
                        AxMenu(
                          key: _fileMenu,
                          label: 'File actions',
                          hint: 'The selected file\'s actions — Shift+F10',
                          entries: () => fileMenu(context, app, app.workspace.selection),
                          trigger: (context, st, open) => FocusRing(
                            visible: st.focused,
                            child: SizedBox(
                              width: 24,
                              height: 24,
                              child: Center(
                                child: AxiomIcon(
                                  AxiomIcons.more,
                                  size: 14,
                                  color: open || st.hovered ? c.ink : c.ink3,
                                ),
                              ),
                            ),
                          ),
                        ),
                      ],
                    ],
                  );
                },
              ),
            ),
          ),
          const Hairline(),
          Expanded(
            child: Stack(
              fit: StackFit.expand,
              children: [
                for (final x in const [
                  Destination.files,
                  Destination.chat,
                  Destination.search,
                  Destination.scm,
                  Destination.environments,
                  Destination.inspector,
                ])
                  if (_shown.contains(x))
                    Offstage(
                      key: ValueKey(x),
                      offstage: x != d,
                      child: TickerMode(
                        enabled: x == d,
                        child: ExcludeFocus(excluding: x != d, child: _page(app, x)),
                      ),
                    ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

// ---- main area -------------------------------------------------------------------------------

class _Main extends StatefulWidget {
  const _Main({required this.editor, required this.chat, this.environments = false});
  final GlobalKey<EditorAreaState> editor;

  /// The Environments page owns the main area (the editor stays mounted behind it).
  final bool environments;

  /// Chat owns the main area (the editor stays mounted behind it).
  final bool chat;

  @override
  State<_Main> createState() => _MainState();
}

class _MainState extends State<_Main> {
  bool _chatShown = false;

  @override
  Widget build(BuildContext context) {
    _chatShown |= widget.chat;
    final work = _Work(editor: widget.editor);
    final git = AppScope.of(context).git;
    return ListenableBuilder(listenable: git, builder: (context, _) => _layers(work, git.diff));
  }

  Widget _layers(Widget work, String? diff) {
    // A diff (Source Control) covers the editor, which stays mounted under it.
    final covered = widget.chat || widget.environments || diff != null;
    return Stack(
      fit: StackFit.expand,
      children: [
        Offstage(
          offstage: covered,
          child: TickerMode(
            enabled: !covered,
            child: ExcludeFocus(excluding: covered, child: work),
          ),
        ),
        if (widget.environments && !widget.chat)
          const EnvironmentsPage()
        else if (diff != null && !widget.chat)
          DiffView(key: ValueKey(diff), path: diff),
        if (_chatShown)
          Offstage(
            offstage: !widget.chat,
            child: TickerMode(
              enabled: widget.chat,
              child: ExcludeFocus(excluding: !widget.chat, child: const ChatView()),
            ),
          ),
      ],
    );
  }
}

/// The folder's work: the editor, or what to do when nothing is open.
class _Work extends StatefulWidget {
  const _Work({required this.editor});
  final GlobalKey<EditorAreaState> editor;

  @override
  State<_Work> createState() => _WorkState();
}

class _WorkState extends State<_Work> {
  /// One instance for the state's life: a rebuild above (the palette opening, a splitter step,
  /// a folder listing) then stops here instead of rebuilding the whole editor (AUD-072).
  late final Widget _editorArea = EditorArea(key: widget.editor);

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final t = context.axiom;
    final c = t.colors;
    return ListenableBuilder(
      listenable: Listenable.merge([app.workspace, app.client.state, app.documents]),
      builder: (context, _) {
        final ws = app.workspace;
        final daemon = app.client.state.value;
        // The backend stopped while documents were open: they stay on screen (read-only, with
        // their unsaved text) under a line that says so and restarts it (AUD-001).
        if (daemon is DaemonFailed && app.documents.docs.isNotEmpty) {
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              ColoredBox(
                color: c.raised,
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: Metrics.s5, vertical: 6),
                  child: Row(
                    children: [
                      Expanded(
                        child: Semantics(
                          liveRegion: true,
                          child: Text(
                            'The backend stopped. Your documents are kept, read-only, until it restarts.',
                            style: t.type.small.copyWith(color: c.danger),
                            overflow: TextOverflow.ellipsis,
                          ),
                        ),
                      ),
                      AxButton(
                        label: 'Restart the Backend',
                        onPressed: () => unawaited(app.restartBackend()),
                      ),
                    ],
                  ),
                ),
              ),
              Expanded(child: _editorArea),
            ],
          );
        }
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
              Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  if (ws.rootGone && ws.isOpen) ...[
                    AxButton(
                      label: 'Close Folder',
                      onPressed: () {
                        ws.dismissError();
                        app.documents.requestCloseFolder();
                      },
                    ),
                    const SizedBox(width: Metrics.s3),
                    AxButton(
                      label: 'Open Folder…',
                      kind: ButtonKind.quiet,
                      onPressed: () => unawaited(app.pickFolder?.call()),
                    ),
                    const SizedBox(width: Metrics.s3),
                  ],
                  AxButton(label: 'Dismiss', kind: ButtonKind.quiet, onPressed: ws.dismissError),
                ],
              ),
            ],
          );
        }
        if (ws.isOpen && app.documents.docs.isNotEmpty) return _editorArea;
        if (ws.isOpen) {
          // Tauri's empty editor: the ways in, each with its keys, quiet and centred.
          Widget way(String label, String keys) => SizedBox(
            height: 30,
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                SizedBox(
                  width: 136,
                  child: Text(label, style: t.type.body.copyWith(color: c.ink2)),
                ),
                Keycaps(keys),
              ],
            ),
          );
          return Column(
            children: [
              _EmptyTabStrip(),
              Expanded(
                child: Center(
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      way('Command palette', 'Mod+K'),
                      way('Go to file', 'Mod+P'),
                      way('Run command', 'Mod+Shift+R'),
                      way('Toggle console', 'Mod+J'),
                      if (app.documents.banner.isNotEmpty) ...[
                        const SizedBox(height: Metrics.s4),
                        Text(
                          app.documents.banner,
                          style: t.type.small.copyWith(color: c.dangerText),
                        ),
                      ],
                    ],
                  ),
                ),
              ),
            ],
          );
        }
        // Tauri's welcome: the brass mark, the title and its line, the primary action with its
        // keys inside, then the recent folders.
        return LayoutBuilder(
          builder: (context, box) => SingleChildScrollView(
            padding: EdgeInsets.fromLTRB(Metrics.s7, box.maxHeight * 0.22, Metrics.s7, Metrics.s7),
            child: Center(
              child: SizedBox(
                width: 460,
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    ExcludeSemantics(child: AxiomIcon(AxiomIcons.logo, size: 32, color: c.accent)),
                    const SizedBox(height: Metrics.s6),
                    AnswerLine(
                      ws.opening ? 'Opening the folder…' : 'Open a folder to start',
                      // What opening a folder gives the person, not how it works (AUD-110).
                      detail:
                          'Pick a project folder: Axiom finds its toolchains, language servers '
                          'and Git changes.',
                    ),
                    const SizedBox(height: Metrics.s7),
                    AxButton(
                      label: 'Open Folder…',
                      kind: ButtonKind.primary,
                      hint: 'Open Folder… — Mod+O',
                      trailing: const Keycaps('Mod+O'),
                      onPressed: daemon is DaemonReady && !ws.opening
                          ? () => unawaited(app.pickFolder?.call())
                          : null,
                    ),
                    const SizedBox(height: Metrics.s7),
                    const RecentList(),
                  ],
                ),
              ),
            ),
          ),
        );
      },
    );
  }
}

/// The tab strip's chrome while no file is open (Tauri keeps the strip).
class _EmptyTabStrip extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return SizedBox(
      height: context.scaled(Metrics.tabHeight, fontSize: 12, lineHeight: 16),
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: c.bar,
          border: Border(bottom: BorderSide(color: c.groove)),
        ),
      ),
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
    // Tauri's status bar: segments separated by seams, small muted text.
    final seam = SizedBox(width: 1, child: ColoredBox(color: c.hairline));
    Widget seg(Widget child) => Padding(
      padding: const EdgeInsets.symmetric(horizontal: Metrics.s4),
      child: Center(widthFactor: 1, child: child),
    );
    return SizedBox(
      height: context.scaled(Metrics.statusBarHeight, fontSize: 11, lineHeight: 14),
      child: ColoredBox(
        color: c.bar,
        child: Padding(
          padding: EdgeInsets.zero,
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              seg(
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
                          Text(text, style: t.type.small),
                        ],
                      ),
                    );
                  },
                ),
              ),
              seam,
              ListenableBuilder(
                listenable: app.git,
                builder: (context, _) {
                  final g = app.git.status;
                  if (g == null || !g.repo) return const SizedBox.shrink();
                  final branch =
                      g.branch ?? 'detached at ${(g.head ?? '').padRight(7).substring(0, 7)}';
                  final sync = [
                    if ((g.ahead ?? 0) > 0) '↑${g.ahead}',
                    if ((g.behind ?? 0) > 0) '↓${g.behind}',
                  ].join(' ');
                  final n = app.git.count();
                  return Row(
                    children: [
                      seg(
                        Semantics(
                          label:
                              'Branch $branch${sync.isEmpty ? '' : ', $sync'}, '
                              '$n change${n == 1 ? '' : 's'}'
                              '${(g.polling ?? false) ? ', file watching limited: changes are '
                                        'checked every 10 seconds' : ''}',
                          excludeSemantics: true,
                          child: Row(
                            children: [
                              AxiomIcon(AxiomIcons.branch, size: 12, color: c.ink3),
                              const SizedBox(width: 4),
                              // Long branch names ellipsise (AUD-078).
                              ConstrainedBox(
                                constraints: const BoxConstraints(maxWidth: 200),
                                child: Text(
                                  branch,
                                  style: t.type.small,
                                  maxLines: 1,
                                  softWrap: false,
                                  overflow: TextOverflow.ellipsis,
                                ),
                              ),
                              if (sync.isNotEmpty) ...[
                                const SizedBox(width: 4),
                                Text(sync, style: t.type.numeric),
                              ],
                              // No file watch could be made (the system's limit): changes show
                              // up to 10 s late (AUD-012).
                              if (g.polling ?? false) ...[
                                const SizedBox(width: 6),
                                AxiomIcon(AxiomIcons.warning, size: 11, color: c.warn),
                                const SizedBox(width: 3),
                                Text('watch limited', style: t.type.small.copyWith(color: c.warn)),
                              ],
                            ],
                          ),
                        ),
                      ),
                      seam,
                    ],
                  );
                },
              ),
              const SizedBox(width: Metrics.s5),
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
                              style: t.type.small,
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
                listenable: Listenable.merge([app.workspace, app.documents, app.documents.cursor]),
                builder: (context, _) {
                  final doc = app.documents.active;
                  if (doc == null) {
                    return app.workspace.isOpen
                        ? Row(
                            crossAxisAlignment: CrossAxisAlignment.stretch,
                            children: [
                              seam,
                              seg(
                                ConstrainedBox(
                                  constraints: const BoxConstraints(maxWidth: 320),
                                  child: Text(
                                    app.workspace.root,
                                    style: t.type.small,
                                    maxLines: 1,
                                    softWrap: false,
                                    overflow: TextOverflow.ellipsis,
                                  ),
                                ),
                              ),
                            ],
                          )
                        : const SizedBox.shrink();
                  }
                  return Row(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      seam,
                      seg(const LspStatus()),
                      if (app.documents.cursorText.isNotEmpty) ...[
                        seam,
                        seg(
                          Semantics(
                            label: app.documents.cursorText,
                            child: Text(app.documents.cursorText, style: t.type.numeric),
                          ),
                        ),
                      ],
                      seam,
                      // The language: a button that lists every language (ADR-0021 §1).
                      Pressable(
                        label: 'Language: ${doc.language.name}',
                        hint: 'Change the language of ${titleOf(doc.path)}',
                        onPressed: () => app.openPicker?.call(
                          label: 'Language',
                          inputLabel: 'Language name',
                          placeholder: 'Choose a language for ${titleOf(doc.path)}',
                          empty: 'No such language',
                          items: [
                            for (final l in Languages.instance.all)
                              Command(
                                id: 'language ${l.id}',
                                title: l.id == doc.language.id ? '${l.name} (current)' : l.name,
                                run: () => app.documents.setLanguage(doc.path, l),
                              ),
                          ],
                        ),
                        builder: (context, s) => FocusRing(
                          visible: s.focused,
                          child: ColoredBox(
                            color: s.hovered ? c.hover : c.hover.withValues(alpha: 0),
                            child: seg(
                              Text(
                                doc.language.name,
                                style: t.type.small.copyWith(color: s.hovered ? c.ink : null),
                              ),
                            ),
                          ),
                        ),
                      ),
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
