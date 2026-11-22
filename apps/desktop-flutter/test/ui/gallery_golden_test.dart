// Golden images of the gallery in both themes (FL-DS-001), with the real fonts. Linux only:
// text rasterisation differs between platforms. Update with
// `flutter test --update-goldens test/ui/gallery_golden_test.dart`.

import 'dart:io';

import 'package:axiom_desktop/ui/gallery.dart';
import 'package:axiom_desktop/ui/overlays.dart';
import 'package:axiom_desktop/ui/pressable.dart';
import 'package:axiom_desktop/ui/theme.dart';
import 'package:axiom_desktop/ui/tokens.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

Future<void> loadFonts() async {
  for (final (family, files) in [
    ('IBM Plex Sans', ['Regular', 'Medium', 'SemiBold', 'Italic']),
    ('IBM Plex Mono', ['Regular', 'Medium', 'Italic']),
  ]) {
    final loader = FontLoader(family);
    for (final f in files) {
      final path = 'assets/fonts/${family.replaceAll(' ', '')}-$f.ttf';
      loader.addFont(Future.value(ByteData.sublistView(File(path).readAsBytesSync())));
    }
    await loader.load();
  }
}

void main() {
  setUpAll(loadFonts);
  for (final (name, colors) in [('dark', AxiomColors.dark), ('light', AxiomColors.light)]) {
    testWidgets('gallery, $name', (tester) async {
      tester.view.physicalSize = const Size(1000, 1180);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final theme = AxiomThemeData(colors: colors, motionMode: MotionMode.off);
      await tester.pumpWidget(
        WidgetsApp(
          color: colors.ground,
          textStyle: theme.type.body,
          debugShowCheckedModeBanner: false,
          pageRouteBuilder: <T>(s, b) =>
              PageRouteBuilder<T>(settings: s, pageBuilder: (c, _, _) => b(c)),
          home: const RepaintBoundary(child: Gallery()),
          builder: (context, child) => AxiomTheme(
            data: theme,
            child: HintScope(
              hint: ValueNotifier(null),
              child: ToastHost(child: child!),
            ),
          ),
        ),
      );
      await tester.pump();
      await expectLater(find.byType(Gallery), matchesGoldenFile('goldens/gallery-$name.png'));
    }, skip: !Platform.isLinux);
  }
}
