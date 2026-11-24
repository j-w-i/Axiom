@TestOn('mac-os')
library;

import 'dart:io';
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:axiom/gallery/sheets.dart';
import 'package:axiom/gallery/workbench.dart';
import 'package:axiom/patterns/execution.dart';
import 'package:axiom/ui/surface.dart';
import 'package:axiom/ui/tokens.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'shot.dart';

/// Machines rasterise glyph edges with rounding differences of a few levels in 255. Those are not
/// counted; everything larger is, and at most 0.1 % of the pixels may differ by that much.
class Tolerant extends LocalFileComparator {
  Tolerant(super.testFile);
  static const level = 8, share = .001;

  static Future<(int, int, ByteData)> _pixels(List<int> png) async {
    final codec = await ui.instantiateImageCodec(Uint8List.fromList(png));
    final img = (await codec.getNextFrame()).image;
    final d = (await img.toByteData(format: ui.ImageByteFormat.rawRgba))!;
    final r = (img.width, img.height, d);
    img.dispose();
    return r;
  }

  static Future<double> difference(List<int> a, List<int> b) async {
    final (w, h, x) = await _pixels(a);
    final (w2, h2, y) = await _pixels(b);
    if (w != w2 || h != h2) return 1;
    var n = 0;
    for (var i = 0; i < x.lengthInBytes; i += 4) {
      if ((x.getUint8(i) - y.getUint8(i)).abs() > level ||
          (x.getUint8(i + 1) - y.getUint8(i + 1)).abs() > level ||
          (x.getUint8(i + 2) - y.getUint8(i + 2)).abs() > level) {
        n++;
      }
    }
    return n / (w * h);
  }

  @override
  Future<bool> compare(Uint8List image, Uri golden) async {
    final master = await getGoldenBytes(golden);
    if (await difference(image, master) <= share) return true;
    final r = await GoldenFileComparator.compareLists(image, master);
    throw FlutterError(await generateFailureOutput(r, golden, basedir));
  }
}

void main() {
  setUpAll(() {
    final base = goldenFileComparator as LocalFileComparator;
    goldenFileComparator = Tolerant(base.basedir.resolve('goldens_test.dart'));
  });

  for (final (theme, c) in [('dark', AxColors.dark), ('light', AxColors.light)]) {
    for (final (name, build) in sheets) {
      testWidgets('$name, $theme', (t) async {
        await mount(
          t,
          Surface(
            c.chassis,
            radius: 0,
            child: Builder(
              builder: (x) => SingleChildScrollView(
                physics: const NeverScrollableScrollPhysics(),
                child: build(x),
              ),
            ),
          ),
          size: const Size(960, 980),
          theme: AxTheme(colors: c),
          dpr: 1,
        );
        expect(t.takeException(), isNull);
        await snap(t, 'sheet-${name.toLowerCase()}-$theme');
        await expectLater(
          find.byKey(stage),
          matchesGoldenFile('goldens/${name.toLowerCase()}-$theme.png'),
        );
      });
    }
    for (final size in const [Size(1440, 900), Size(1100, 700), Size(820, 640)]) {
      testWidgets('workbench ${size.width.round()}, $theme', (t) async {
        await mount(
          t,
          const Workbench(),
          size: size,
          theme: AxTheme(colors: c),
          dpr: 1,
        );
        expect(t.takeException(), isNull, reason: 'no overflow');
        await expectLater(
          find.byKey(stage),
          matchesGoldenFile('goldens/workbench-${size.width.round()}-$theme.png'),
        );
      });
    }
  }

  testWidgets('what an agent runs on, with a question open', (t) async {
    await mount(t, const Workbench(), size: const Size(1100, 700), dpr: 1);
    for (final f in [
      find.byType(ExecutionKey),
      find.bySemanticsLabel('Model: GPT-5.6'),
      find.bySemanticsLabel('GPT-5.6 mini'),
    ]) {
      await t.tap(f);
      await t.pump(const Duration(milliseconds: 50));
      await t.pump(const Duration(milliseconds: 300));
    }
    await expectLater(find.byKey(stage), matchesGoldenFile('goldens/execution-dark.png'));
  });

  testWidgets('the comparison sees one pixel of movement', (t) async {
    Future<Uint8List> shot(double dx) async {
      await mount(
        t,
        Transform.translate(offset: Offset(dx, 0), child: const Workbench()),
        size: const Size(1100, 700),
        dpr: 1,
      );
      late Uint8List out;
      await t.runAsync(() async {
        final r = stage.currentContext!.findRenderObject()! as RenderRepaintBoundary;
        final img = await r.toImage();
        out = (await img.toByteData(format: ui.ImageByteFormat.png))!.buffer.asUint8List();
      });
      return out;
    }

    final (a, b, c) = (await shot(0), await shot(0), await shot(1));
    await t.runAsync(() async {
      expect(await Tolerant.difference(a, b), 0);
      expect(await Tolerant.difference(a, c), greaterThan(Tolerant.share * 10));
    });
  });

  test('every golden is used', () {
    final files = Directory('test/goldens').listSync().length;
    expect(files, 2 * (sheets.length + 3) + 1);
  });
}
