import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';

import 'ui/gallery.dart';
import 'ui/overlays.dart';
import 'ui/pressable.dart';
import 'ui/theme.dart';
import 'ui/tokens.dart';

/// Axiom's desktop shell (ADR-0020). Until FL-SHELL-001 it shows the design system's gallery.
void main(List<String> args) {
  runApp(const AxiomApp());
}

class AxiomApp extends StatefulWidget {
  const AxiomApp({super.key, this.home});

  /// What to show; the gallery until the shell exists.
  final Widget? home;

  @override
  State<AxiomApp> createState() => _AxiomAppState();
}

class _AxiomAppState extends State<AxiomApp> with WidgetsBindingObserver {
  final _hint = ValueNotifier<String?>(null);
  Brightness _brightness = PlatformDispatcher.instance.platformBrightness;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void didChangePlatformBrightness() =>
      setState(() => _brightness = PlatformDispatcher.instance.platformBrightness);

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _hint.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final colors = _brightness == Brightness.dark ? AxiomColors.dark : AxiomColors.light;
    final theme = AxiomThemeData(colors: colors);
    return WidgetsApp(
      title: 'Axiom',
      color: colors.ground,
      textStyle: theme.type.body,
      debugShowCheckedModeBanner: false,
      pageRouteBuilder: <T>(RouteSettings settings, WidgetBuilder builder) =>
          PageRouteBuilder<T>(settings: settings, pageBuilder: (context, _, _) => builder(context)),
      home: widget.home ?? const Gallery(),
      builder: (context, child) => AxiomTheme(
        data: theme,
        child: HintScope(
          hint: _hint,
          child: ToastHost(child: child!),
        ),
      ),
    );
  }
}
