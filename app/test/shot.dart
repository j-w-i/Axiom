import 'dart:io';
import 'dart:ui' as ui;

import 'package:axiom/ui/tokens.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

/// Loads the real fonts, so a rendered image shows what a person would see. Offscreen only: no
/// test opens a window.
Future<void> loadFonts() async {
  for (final (family, file) in [('Inter', 'Inter'), ('JetBrains Mono', 'JetBrainsMono')]) {
    final l = FontLoader(family)
      ..addFont(Future.value(ByteData.sublistView(File('fonts/$file.ttf').readAsBytesSync())));
    await l.load();
  }
}

final stage = GlobalKey();

Future<void> mount(
  WidgetTester t,
  Widget w, {
  Size size = const Size(1440, 900),
  double dpr = 2,
  AxTheme theme = const AxTheme(),
  bool reduceMotion = true,
  double textScale = 1,
}) async {
  t.view.devicePixelRatio = dpr;
  t.view.physicalSize = size * dpr;
  addTearDown(t.view.reset);
  await t.runAsync(loadFonts);
  await t.pumpWidget(
    RepaintBoundary(
      key: stage,
      child: Directionality(
        textDirection: TextDirection.ltr,
        child: MediaQuery(
          data: MediaQueryData(
            size: size,
            devicePixelRatio: dpr,
            disableAnimations: reduceMotion,
            textScaler: TextScaler.linear(textScale),
          ),
          child: WidgetsApp(
            color: const Color(0xFF000000),
            debugShowCheckedModeBanner: false,
            pageRouteBuilder: <T>(RouteSettings s, WidgetBuilder b) =>
                PageRouteBuilder<T>(settings: s, pageBuilder: (c, _, _) => b(c)),
            home: Ax(theme: theme, child: w),
          ),
        ),
      ),
    ),
  );
  await t.pump(const Duration(milliseconds: 50));
  await t.pump(const Duration(milliseconds: 300));
}

/// Writes build/shots/[name].png for a person to look at.
Future<void> snap(WidgetTester t, String name) async {
  debugDisableShadows = false;
  final r = stage.currentContext!.findRenderObject()! as RenderRepaintBoundary;
  void dirty(RenderObject o) {
    o.markNeedsPaint();
    o.visitChildren(dirty);
  }

  dirty(r);
  await t.pump();
  await t.runAsync(() async {
    final img = await r.toImage(pixelRatio: t.view.devicePixelRatio);
    final b = await img.toByteData(format: ui.ImageByteFormat.png);
    File('build/shots/$name.png')
      ..createSync(recursive: true)
      ..writeAsBytesSync(b!.buffer.asUint8List());
  });
  debugDisableShadows = true;
  dirty(r);
  await t.pump();
}
