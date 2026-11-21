import 'dart:async';

import 'package:flutter/widgets.dart';

import '../daemon/client.dart';
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
      workspace = WorkspaceStore(client);

  final AppArgs args;
  final DaemonClient client;
  final LocalPrefs prefs;
  final SettingsStore settings;
  final WorkspaceStore workspace;

  /// The hint line's text (the hovered or focused control's hint).
  final hint = ValueNotifier<String?>(null);

  /// Opens the palette (set by the shell).
  VoidCallback? openPalette;
  VoidCallback? openSettings;
  Future<void> Function()? pickFolder;

  static Future<AppState> create(AppArgs args) async {
    final program = locateDaemon(explicit: args.daemon);
    final client = DaemonClient(
      DaemonConfig(
        program: program ?? 'axiomd',
        args: [
          if (args.dataDir != null) ...['--data-dir', args.dataDir!],
        ],
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
