import 'dart:async';
import 'dart:ui' show AppExitResponse;

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';

import 'app/app_state.dart';
import 'app/args.dart';
import 'shell/shell.dart';
import 'state/settings.dart';
import 'ui/gallery.dart';
import 'ui/overlays.dart';
import 'ui/pressable.dart';
import 'ui/theme.dart';
import 'ui/tokens.dart';

/// Axiom's desktop shell (ADR-0020).
Future<void> main(List<String> argv) async {
  WidgetsFlutterBinding.ensureInitialized();
  final args = AppArgs.parse(argv);
  if (args.gallery) {
    runApp(const AxiomApp(home: Gallery()));
    return;
  }
  final app = await AppState.create(args);
  runApp(AxiomApp(app: app));
  unawaited(app.start());
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
