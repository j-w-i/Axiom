@TestOn('mac-os')
library;

import 'dart:io';
import 'dart:typed_data';

import 'package:axiom/gallery/sheets.dart';
import 'package:axiom/gallery/workbench.dart';
import 'package:axiom/ui/surface.dart';
import 'package:axiom/ui/tokens.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'shot.dart';

/// Fonts and antialiasing differ slightly between machines; a change a person would notice does not
/// fit in 0.3 % of the pixels.
class Tolerant extends LocalFileComparator {
  Tolerant(super.testFile);
  @override
  Future<bool> compare(Uint8List image, Uri golden) async {
    final r = await GoldenFileComparator.compareLists(image, await getGoldenBytes(golden));
    final ok = r.passed || r.diffPercent <= .003;
    if (!ok) throw FlutterError(await generateFailureOutput(r, golden, basedir));
    r.dispose();
    return ok;
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

  test('every golden is used', () {
    final files = Directory('test/goldens').listSync().length;
    expect(files, 2 * (sheets.length + 3));
  });
}
