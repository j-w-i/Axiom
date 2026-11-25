import 'package:flutter/widgets.dart';

import 'ui/fonts.dart';
import 'ui/prefs.dart';
import 'ui/tokens.dart';
import 'ui/zoom.dart';

/// The root: what its person chose and the rest from the system, our own text and focus, no Material.
class AxiomApp extends StatefulWidget {
  const AxiomApp({required this.home, super.key, this.prefs});
  final Widget home;

  /// Where what was chosen is kept; without it nothing outlives the window.
  final ValueNotifier<Prefs>? prefs;
  @override
  State<AxiomApp> createState() => _AxiomAppState();
}

class _AxiomAppState extends State<AxiomApp> {
  late final _prefs = widget.prefs ?? ValueNotifier(const Prefs());

  @override
  void dispose() {
    if (widget.prefs == null) _prefs.dispose();
    super.dispose();
  }

  /// The named family, if it is installed. Looked for when it is named, and not with every frame.
  (String, bool)? _looked;
  String _family(String name) {
    if (_looked?.$1 != name) _looked = (name, hasFamily(name));
    return _looked!.$2 ? name : '';
  }

  @override
  Widget build(BuildContext context) => PrefsScope(
    _prefs,
    child: WidgetsApp(
      title: 'Axiom',
      color: AxColors.dark.live,
      debugShowCheckedModeBanner: false,
      pageRouteBuilder: <T>(RouteSettings s, WidgetBuilder b) =>
          PageRouteBuilder<T>(settings: s, pageBuilder: (c, _, _) => b(c)),
      home: widget.home,
      // Round the navigator: what floats is in it too.
      builder: (context, child) {
        final p = PrefsScope.of(context), m = MediaQuery.of(context);
        final light = switch (p.look) {
          Look.system => m.platformBrightness == Brightness.light,
          Look.light => true,
          Look.dark => false,
        };
        final scale = m.textScaler.scale(16) / 16 * p.text;
        return MediaQuery(
          data: p.text == 1 ? m : m.copyWith(textScaler: TextScaler.linear(scale)),
          child: Ax(
            theme: AxTheme(
              colors: light ? AxColors.light : AxColors.dark,
              density: p.density,
              code: _family(p.code),
            ),
            child: Zoom(p.zoom, child: child!),
          ),
        );
      },
    ),
  );
}
