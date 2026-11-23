import 'dart:io';
import 'dart:async';
import 'dart:ui' show AppExitResponse, AppExitType;

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'app/app_state.dart';
import 'app/args.dart';
import 'shell/shell.dart';
import 'state/settings.dart';
import 'ui/gallery.dart';
import 'ui/overlays.dart';
import 'ui/pressable.dart';
import 'daemon/client.dart';
import 'ui/theme.dart';
import 'ui/tokens.dart';

/// Axiom's desktop shell (ADR-0020).
Future<void> main(List<String> argv) async {
  final entered = DateTime.now().microsecondsSinceEpoch;
  WidgetsFlutterBinding.ensureInitialized();
  final args = AppArgs.parse(argv);
  if (args.gallery) {
    runApp(const AxiomApp(home: Gallery()));
    return;
  }
  final app = await AppState.create(args);
  if (args.selfTest) {
    // Package verification: find the daemon as an installed copy does, complete the handshake,
    // report it, and quit (scripts/verify-package.sh).
    final code = await selfTest(app);
    exit(code);
  }
  runApp(AxiomApp(app: app));
  if (Platform.environment['AXIOM_STARTUP_TRACE'] == '1') {
    unawaited(startupTrace(app, entered));
  } else {
    unawaited(app.start());
  }
}

/// `AXIOM_STARTUP_TRACE=1` (scripts/perf.sh, audit AUD-061): prints, on stderr, when `main` was
/// entered, when the first frame was rasterised, when the daemon was ready and (with
/// `AXIOM_STARTUP_OPEN=<folder>`) when that folder was listed, as microseconds since the epoch;
/// then quits after `AXIOM_STARTUP_HOLD_MS` (default 1500), so the harness can sample the idle
/// process meanwhile.
Future<void> startupTrace(AppState app, int entered) async {
  void mark(String what) =>
      stderr.writeln('AXIOM_STARTUP $what=${DateTime.now().microsecondsSinceEpoch}');
  stderr.writeln('AXIOM_STARTUP main=$entered');
  unawaited(WidgetsBinding.instance.waitUntilFirstFrameRasterized.then((_) => mark('first_frame')));
  await app.start();
  mark('ready');
  final folder = Platform.environment['AXIOM_STARTUP_OPEN'];
  if (folder != null && folder.isNotEmpty) {
    await app.workspace.openFolder(folder);
    while (app.workspace.isOpen && !app.workspace.rootLoaded) {
      await Future<void>.delayed(const Duration(milliseconds: 2));
    }
    mark('folder');
    final file = Platform.environment['AXIOM_STARTUP_FILE'];
    if (file != null && file.isNotEmpty) await app.documents.open(file);
  }
  stderr.writeln('AXIOM_STARTUP pids shell=$pid daemon=${app.client.pid}');
  final hold = int.tryParse(Platform.environment['AXIOM_STARTUP_HOLD_MS'] ?? '') ?? 1500;
  await Future<void>.delayed(Duration(milliseconds: hold));
  await app.shutdown();
  exit(0);
}

/// Starts the daemon, waits for `initialize`, prints `connected <daemon> <version> <protocol>`
/// (or why not) and shuts it down. 0 when connected.
Future<int> selfTest(AppState app) async {
  final program = app.client.config.program;
  try {
    await app.client.start().timeout(const Duration(seconds: 20));
    final s = app.client.state.value;
    if (s is DaemonReady) {
      stdout.writeln('connected $program ${s.serverVersion} ${s.protocolVersion}');
      await app.client.shutdown();
      return 0;
    }
    stdout.writeln('not connected: $s');
    return 1;
  } on Object catch (e) {
    stdout.writeln('not connected ($program): $e');
    return 1;
  }
}

class AxiomApp extends StatefulWidget {
  const AxiomApp({super.key, this.app, this.home});

  /// The application; null shows [home] alone (the gallery, tests).
  final AppState? app;
  final Widget? home;

  @override
  State<AxiomApp> createState() => _AxiomAppState();
}

class _AxiomAppState extends State<AxiomApp> with WidgetsBindingObserver {
  late final ValueNotifier<String?> _hint = widget.app?.hint ?? ValueNotifier(null);
  Brightness _platform = PlatformDispatcher.instance.platformBrightness;
  AppLifecycleListener? _lifecycle;
  bool _quitting = false;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    final app = widget.app;
    if (app != null) {
      _lifecycle = AppLifecycleListener(
        onExitRequested: () async {
          if (_quitting) return AppExitResponse.exit;
          // Unsaved work: ask first (Save, Don't Save, Cancel); quit once it is answered.
          if (app.documents.dirtyPaths.isNotEmpty) {
            app.documents.onQuitConfirmed = () async {
              _quitting = true;
              await app.shutdown();
              await ServicesBinding.instance.exitApplication(AppExitType.required);
            };
            app.documents.requestQuit();
            return AppExitResponse.cancel;
          }
          _quitting = true;
          await app.shutdown();
          return AppExitResponse.exit;
        },
      );
    }
  }

  @override
  void didChangePlatformBrightness() =>
      setState(() => _platform = PlatformDispatcher.instance.platformBrightness);

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _lifecycle?.dispose();
    if (widget.app == null) _hint.dispose();
    super.dispose();
  }

  AxiomThemeData _theme() {
    final s = widget.app?.settings;
    final brightness = switch (s?.theme ?? ThemeSetting.system) {
      ThemeSetting.light => Brightness.light,
      ThemeSetting.dark => Brightness.dark,
      ThemeSetting.system => _platform,
    };
    return AxiomThemeData(
      colors: brightness == Brightness.dark ? AxiomColors.dark : AxiomColors.light,
      motionMode: s?.motion ?? MotionMode.standard,
    );
  }

  @override
  Widget build(BuildContext context) {
    final app = widget.app;
    Widget build(BuildContext context) {
      final theme = _theme();
      Widget child = WidgetsApp(
        title: 'Axiom',
        color: theme.colors.ground,
        textStyle: theme.type.body,
        debugShowCheckedModeBanner: false,
        pageRouteBuilder: <T>(RouteSettings settings, WidgetBuilder builder) => PageRouteBuilder<T>(
          settings: settings,
          pageBuilder: (context, _, _) => builder(context),
        ),
        home: widget.home ?? const Shell(),
        builder: (context, child) => AxiomTheme(
          data: theme,
          child: HintScope(
            hint: _hint,
            child: ToastHost(child: child!),
          ),
        ),
      );
      if (app != null) child = AppScope(app: app, child: child);
      return child;
    }

    if (app == null) return build(context);
    return ListenableBuilder(listenable: app.settings, builder: (context, _) => build(context));
  }
}
