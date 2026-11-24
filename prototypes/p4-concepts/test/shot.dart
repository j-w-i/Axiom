import 'dart:io';
import 'dart:ui' as ui;
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

Future<void> loadFonts() async {
  for (final (f, p) in [('Mona', 'MonaSans'), ('Archivo', 'Archivo'), ('Instrument', 'InstrumentSans'), ('Sofia', 'SofiaSans'), ('SofiaSemi', 'SofiaSansSemi'), ('Hanken', 'Hanken'), ('JB', 'JetBrainsMono'), ('Martian', 'MartianMono'), ('Spline', 'SplineSansMono'), ('Sometype', 'SometypeMono')]) {
    final l = FontLoader(f)..addFont(Future.value(ByteData.sublistView(File('fonts/$p.ttf').readAsBytesSync())));
    await l.load();
  }
}

Future<void> shot(WidgetTester t, Widget w, String name, {Size size = const Size(1440, 900), double dpr = 2}) async {
  debugDisableShadows = false;
  t.view.devicePixelRatio = dpr; t.view.physicalSize = size * dpr;
  await t.runAsync(loadFonts);
  final k = GlobalKey();
  await t.pumpWidget(RepaintBoundary(key: k, child: Directionality(textDirection: TextDirection.ltr, child: MediaQuery(data: MediaQueryData(size: size, devicePixelRatio: dpr), child: w))));
  await t.pump(const Duration(milliseconds: 100));
  await t.runAsync(() async {
    final img = await (k.currentContext!.findRenderObject()! as RenderRepaintBoundary).toImage(pixelRatio: dpr);
    final b = await img.toByteData(format: ui.ImageByteFormat.png);
    File('out/$name.png')..createSync(recursive: true)..writeAsBytesSync(b!.buffer.asUint8List());
  });
  debugDisableShadows = true;
  addTearDown(t.view.reset);
}
