import 'package:flutter/widgets.dart';

import 'ui/tokens.dart';

/// The root: theme from the system, our own text and focus, no Material.
class AxiomApp extends StatelessWidget {
  const AxiomApp({required this.home, super.key, this.density = AxDensity.regular});
  final Widget home;
  final AxDensity density;
  @override
  Widget build(BuildContext context) => WidgetsApp(
    title: 'Axiom',
    color: AxColors.dark.live,
    debugShowCheckedModeBanner: false,
    pageRouteBuilder: <T>(RouteSettings s, WidgetBuilder b) =>
        PageRouteBuilder<T>(settings: s, pageBuilder: (c, _, _) => b(c)),
    home: Builder(
      builder: (context) {
        final light = MediaQuery.platformBrightnessOf(context) == Brightness.light;
        return Ax(
          theme: AxTheme(colors: light ? AxColors.light : AxColors.dark, density: density),
          child: home,
        );
      },
    ),
  );
}
