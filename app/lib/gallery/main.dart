import 'package:flutter/widgets.dart';

import '../app.dart';
import '../ui/controls.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';
import 'sheets.dart';
import 'workbench.dart';

/// The component gallery: `flutter run -t lib/gallery/main.dart` (opens a window).
Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await Shades.load();
  runApp(const AxiomApp(home: Gallery()));
}

class Gallery extends StatefulWidget {
  const Gallery({super.key});
  @override
  State<Gallery> createState() => _GalleryState();
}

class _GalleryState extends State<Gallery> {
  int _page = 0;
  bool _light = false, _still = false;
  AxDensity _density = AxDensity.regular;

  @override
  Widget build(BuildContext context) {
    final theme = AxTheme(colors: _light ? AxColors.light : AxColors.dark, density: _density);
    return Ax(
      theme: theme,
      child: MediaQuery(
        data: MediaQuery.of(context).copyWith(disableAnimations: _still ? true : null),
        child: Surface(
          theme.colors.bay,
          radius: 0,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(86, 8, 12, 8),
                child: Wrap(
                  spacing: 12,
                  runSpacing: 8,
                  crossAxisAlignment: WrapCrossAlignment.center,
                  children: [
                    AxSegments(
                      label: 'Page',
                      items: [
                        for (final (i, s) in sheets.indexed) (i, s.$1, null),
                        (sheets.length, 'Workbench', null),
                      ],
                      value: _page,
                      onChanged: (v) => setState(() => _page = v),
                    ),
                    AxSegments(
                      label: 'Density',
                      items: [for (final d in AxDensity.values) (d, d.name, null)],
                      value: _density,
                      onChanged: (v) => setState(() => _density = v),
                    ),
                    AxCheck(
                      label: 'Light',
                      value: _light,
                      onChanged: (v) => setState(() => _light = v),
                    ),
                    AxCheck(
                      label: 'Reduced motion',
                      value: _still,
                      onChanged: (v) => setState(() => _still = v),
                    ),
                  ],
                ),
              ),
              const Hairline(),
              Expanded(
                child: _page < sheets.length
                    ? SingleChildScrollView(child: sheets[_page].$2(context))
                    : const Workbench(titleInset: 16),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
