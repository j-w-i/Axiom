import 'dart:async';

import 'package:flutter/widgets.dart';

import '../daemon/client.dart';
import '../editor/lsp/lsp_editor.dart';
import '../state/agent.dart';
import '../state/documents.dart';
import '../state/lsp.dart';
import '../state/problems.dart';
import '../state/tasks.dart';
import '../state/terminals.dart';
import '../state/settings.dart';
import '../state/workspace.dart';
import '../ui/tokens.dart';
import 'args.dart';
import 'local_prefs.dart';

/// The rail's destinations, in the owner's order (ADR-0020 §6). A destination appears on the
/// rail once its page exists.
enum Destination { files, chat, search, scm, environments }

/// One command: shown in the palette and menus, bound to its shortcut.
final class Command {
  const Command({
    required this.id,
    required this.title,
    required this.run,
    this.shortcut,
    this.enabled,
    this.category,
  });
  final String id;
  final String title;

  /// Written with `Mod` (e.g. `Mod+Shift+P`).
  final String? shortcut;
  final String? category;
  final VoidCallback run;
  final bool Function()? enabled;

  bool get isEnabled => enabled?.call() ?? true;
}

/// Everything the shell presents, wired to one daemon.
class AppState {
  AppState({required this.args, required this.client, required this.prefs})
    : settings = SettingsStore(client),
      workspace = WorkspaceStore(client) {
    documents = DocumentsStore(client, workspace);
    lsp = LspStore(client, workspace);
    lspSync = LspSync(docs: documents, lsp: lsp);
    documents.onSaved = lspSync.saved;
    problems = ProblemsStore(lsp, client: client, workspace: workspace);
    tasks = TasksStore(client, workspace)..onProviderStart = problems.begin;
    terminals = TerminalsStore(client, workspace);
    agent = AgentStore(client, workspace);
  }

  final AppArgs args;
  final DaemonClient client;
  final LocalPrefs prefs;
  final SettingsStore settings;
  final WorkspaceStore workspace;
  late final DocumentsStore documents;
  late final LspStore lsp;
  late final LspSync lspSync;
  late final ProblemsStore problems;
  late final TasksStore tasks;
  late final TerminalsStore terminals;
  late final AgentStore agent;

  /// The hint line's text (the hovered or focused control's hint).
  final hint = ValueNotifier<String?>(null);

  /// Opens the palette (set by the shell).
  VoidCallback? openPalette;

  /// Opens Settings, at [section] when given (`models`, …).
  void Function([String? section])? openSettings;
  Future<void> Function()? pickFolder;
  VoidCallback? openGoToFile;

  /// Focuses the Chat composer (set by the Chat view once it exists).
  VoidCallback? focusComposer;

  /// Focuses the AI dock's composer (set by the dock while it is shown).
  VoidCallback? focusDock;

  /// Focuses the run prompt (set by the Output view).
  VoidCallback? focusRunPrompt;

  /// Focuses the shown terminal, creating one when there is none (set by the Terminal view).
  Future<void> Function()? focusTerminal;

  /// Runs the window's command bound to [e], if one is (set by the shell; the terminal hands it
  /// the window's chords).
  bool Function(KeyEvent e)? shellShortcut;

  /// [daemonEnvironment]: extra variables for the daemon (the end-to-end suites' fake servers).
  static Future<AppState> create(AppArgs args, {Map<String, String>? daemonEnvironment}) async {
    final program = locateDaemon(explicit: args.daemon);
    final client = DaemonClient(
      DaemonConfig(
        program: program ?? 'axiomd',
        args: [
          if (args.dataDir != null) ...['--data-dir', args.dataDir!],
        ],
        environment: daemonEnvironment,
      ),
    );
    final prefs = await LocalPrefs.load(args.shellDir());
    return AppState(args: args, client: client, prefs: prefs);
  }

  /// Starts the daemon, then loads settings and reopens the last workspace.
  Future<void> start() async {
    final s = await client.start();
    if (s is! DaemonReady) return;
    await settings.load();
    unawaited(workspace.refreshRecent());
    await workspace.reopenLast();
  }

  Future<void> restartBackend() async {
    final s = await client.restart();
    if (s is DaemonReady) {
      await settings.load();
      await workspace.reopenLast();
    }
  }

  /// The quit path: save the shell's state, stop the daemon (bounded).
  Future<void> shutdown() async {
    await documents.saveTabState();
    await workspace.flushState();
    await prefs.flush();
    await client.shutdown();
  }

  // ---- layout (shell-local) ---------------------------------------------------------------

  static const sideDefault = 260.0;

  Destination get destination =>
      Destination.values.asNameMap()[prefs.string('destination', 'files')] ?? Destination.files;
  set destination(Destination d) => prefs.set('destination', d.name);

  bool get sideVisible => prefs.flag('side.visible', true);
  set sideVisible(bool v) => prefs.set('side.visible', v);

  double get sideWidth => prefs.number('side.width', sideDefault);
  set sideWidth(double v) => prefs.set('side.width', v);

  static const consoleDefault = 220.0;

  bool get consoleVisible => prefs.flag('console.visible', false);
  set consoleVisible(bool v) => prefs.set('console.visible', v);

  double get consoleHeight => prefs.number('console.height', consoleDefault);
  set consoleHeight(double v) => prefs.set('console.height', v);

  static const dockDefault = 380.0;

  /// The AI dock beside the editor (Mod+L; workspace-spec.md §5.2): Chat in the folder's scope.
  bool get dockVisible => prefs.flag('dock.visible', false);
  set dockVisible(bool v) => prefs.set('dock.visible', v);

  double get dockWidth => prefs.number('dock.width', dockDefault);
  set dockWidth(double v) => prefs.set('dock.width', v);

  MotionMode get motion => settings.motion;
}

/// Puts the [AppState] in scope.
class AppScope extends InheritedWidget {
  const AppScope({super.key, required this.app, required super.child});
  final AppState app;

  static AppState of(BuildContext context) =>
      context.getInheritedWidgetOfExactType<AppScope>()!.app;

  @override
  bool updateShouldNotify(AppScope old) => old.app != app;
}
