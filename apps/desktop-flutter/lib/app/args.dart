import 'dart:io';

/// The shell's command line, as the Tauri shell's: `--daemon <path>`, `--data-dir <dir>`
/// (or `AXIOM_DATA_DIR`), and `--gallery` (debug builds: the design system's gallery).
final class AppArgs {
  const AppArgs({this.daemon, this.dataDir, this.gallery = false});
  final String? daemon;
  final String? dataDir;
  final bool gallery;

  static AppArgs parse(List<String> args, {Map<String, String>? environment}) {
    String? daemon, dataDir;
    var gallery = false;
    for (var i = 0; i < args.length; i++) {
      final a = args[i];
      String? next() => i + 1 < args.length ? args[++i] : null;
      if (a == '--daemon') {
        daemon = next();
      } else if (a.startsWith('--daemon=')) {
        daemon = a.substring(9);
      } else if (a == '--data-dir') {
        dataDir = next();
      } else if (a.startsWith('--data-dir=')) {
        dataDir = a.substring(11);
      } else if (a == '--gallery') {
        gallery = true;
      }
    }
    final env = environment ?? Platform.environment;
    dataDir ??= env['AXIOM_DATA_DIR'];
    return AppArgs(daemon: daemon, dataDir: dataDir, gallery: gallery);
  }

  /// Where the shell keeps its own small files (window layout): the data directory when one is
  /// given, else the platform's configuration directory.
  String shellDir({Map<String, String>? environment}) {
    if (dataDir != null && dataDir!.isNotEmpty) return dataDir!;
    final env = environment ?? Platform.environment;
    final home = env['HOME'] ?? env['USERPROFILE'] ?? '.';
    if (Platform.isMacOS) return '$home/Library/Application Support/dev.axiom.Axiom';
    if (Platform.isWindows) return '${env['APPDATA'] ?? home}\\Axiom';
    final xdg = env['XDG_CONFIG_HOME'];
    return '${xdg != null && xdg.isNotEmpty ? xdg : '$home/.config'}/axiom';
  }
}
