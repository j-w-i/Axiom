// Frame timings of the shell's interactions (ADR-0020 §5), from the engine's FrameTiming (build
// and raster per frame). Run in a profile build, in a private compositor at the owner's scale:
//   apps/desktop-flutter/scripts/frames.sh
// Every frame of every interaction must build and raster within the display's frame (16.7 ms).

import 'dart:io';

import 'package:axiom_desktop/app/app_state.dart';
import 'package:axiom_desktop/ui/splitter.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'support.dart';

const budgetMs = 16.7;

final class Report {
  Report(this.name, this.frames);
  final String name;
  final List<FrameTiming> frames;

  double _ms(Duration d) => d.inMicroseconds / 1000;
  List<double> get build => [for (final f in frames) _ms(f.buildDuration)];
  List<double> get raster => [for (final f in frames) _ms(f.rasterDuration)];
  int get over => frames
      .where((f) => _ms(f.buildDuration) > budgetMs || _ms(f.rasterDuration) > budgetMs)
      .length;

  double _p(List<double> v, double q) {
    if (v.isEmpty) return 0;
    final s = [...v]..sort();
    return s[((s.length - 1) * q).round()];
  }

  Map<String, Object> toJson() => {
    'frames': frames.length,
    'build_p50_ms': _p(build, .5),
    'build_p99_ms': _p(build, .99),
    'build_max_ms': _p(build, 1),
    'raster_p50_ms': _p(raster, .5),
    'raster_p99_ms': _p(raster, .99),
    'raster_max_ms': _p(raster, 1),
    'over_budget': over,
  };

  @override
  String toString() {
    final j = toJson();
    String f(String k) => (j[k]! as double).toStringAsFixed(1);
    return '${name.padRight(22)} frames ${'${j['frames']}'.padLeft(4)}  build p50 ${f('build_p50_ms')} p99 ${f('build_p99_ms')} max ${f('build_max_ms')}  '
        'raster p50 ${f('raster_p50_ms')} p99 ${f('raster_p99_ms')} max ${f('raster_max_ms')}  over ${j['over_budget']}';
  }
}

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  binding.framePolicy = LiveTestWidgetsFlutterBindingFramePolicy.fullyLive;

  testWidgets('frame timings', (tester) async {
    final data = tempDir('axiom-fl-frames-data-');
    final ws = tempDir('axiom-fl-frames-ws-');
    // A folder long enough to scroll: 3000 files.
    for (var i = 0; i < 3000; i++) {
      writeFile(ws, 'many/file_${i.toString().padLeft(4, '0')}.txt', '$i\n');
    }
    addTearDown(() {
      data.deleteSync(recursive: true);
      ws.deleteSync(recursive: true);
    });
    final app = await boot(tester, data.path);
    await app.workspace.openFolder(ws.path);
    await until(tester, () => app.workspace.rootLoaded, what: 'the root');
    app.workspace.setExpanded('many', true);
    await until(tester, () => app.workspace.node('many')?.loaded ?? false, what: 'the long folder');
    await Future<void>.delayed(const Duration(seconds: 1));

    final reports = <Report>[];
    Future<void> measure(String name, Future<void> Function() body) async {
      final frames = <FrameTiming>[];
      void cb(List<FrameTiming> t) => frames.addAll(t);
      SchedulerBinding.instance.addTimingsCallback(cb);
      await body();
      await Future<void>.delayed(const Duration(milliseconds: 600)); // let the last frames report
      SchedulerBinding.instance.removeTimingsCallback(cb);
      reports.add(Report(name, frames));
    }

    try {
      await _interactions(tester, app, measure);
    } catch (e, st) {
      // ignore: avoid_print
      print('FRAMES ERROR $e\n$st');
      rethrow;
    }

    final size = tester.view.physicalSize;
    final line =
        'window ${size.width.round()}x${size.height.round()} physical, dpr ${tester.view.devicePixelRatio}';
    // ignore: avoid_print
    print(line);
    for (final r in reports) {
      // ignore: avoid_print
      print(r);
    }
    binding.reportData = {'window': line, for (final r in reports) r.name: r.toJson()};
    await app.shutdown();
    if (Platform.environment['AXIOM_FRAMES_ASSERT'] != '0') {
      expect(
        [
          for (final r in reports)
            if (r.over > 0) '${r.name}: ${r.over} of ${r.frames.length}',
        ],
        isEmpty,
        reason: 'frames over $budgetMs ms',
      );
    }
  });
}

Future<void> _interactions(
  WidgetTester tester,
  AppState app,
  Future<void> Function(String name, Future<void> Function() body) measure,
) async {
  final list = find.byType(Scrollable).first;
  await measure('explorer fling', () async {
    for (var i = 0; i < 4; i++) {
      await tester.fling(list, const Offset(0, -1600), 4000);
      await Future<void>.delayed(const Duration(milliseconds: 900));
    }
    for (var i = 0; i < 2; i++) {
      await tester.fling(list, const Offset(0, 1600), 4000);
      await Future<void>.delayed(const Duration(milliseconds: 900));
    }
  });
  await measure('explorer wheel', () async {
    final g = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await g.addPointer(location: tester.getCenter(list));
    for (var i = 0; i < 60; i++) {
      await tester.sendEventToBinding(
        PointerScrollEvent(position: tester.getCenter(list), scrollDelta: const Offset(0, 120)),
      );
      await Future<void>.delayed(const Duration(milliseconds: 16));
    }
    await g.removePointer();
  });
  await measure('side bar toggle', () async {
    for (var i = 0; i < 8; i++) {
      await key(tester, LogicalKeyboardKey.keyB, mod: true);
      await Future<void>.delayed(const Duration(milliseconds: 250));
    }
  });
  await measure('palette open/close', () async {
    for (var i = 0; i < 6; i++) {
      await key(tester, LogicalKeyboardKey.keyK, mod: true);
      await Future<void>.delayed(const Duration(milliseconds: 250));
      await key(tester, LogicalKeyboardKey.escape);
      await Future<void>.delayed(const Duration(milliseconds: 250));
    }
  });
  await measure('splitter drag', () async {
    final at = tester.getCenter(find.byType(AxSplitter));
    final g = await tester.startGesture(at, kind: PointerDeviceKind.mouse);
    for (var i = 0; i < 60; i++) {
      await g.moveBy(Offset(i < 30 ? 6 : -6, 0));
      await Future<void>.delayed(const Duration(milliseconds: 16));
    }
    await g.up();
  });
  await measure('theme switch', () async {
    for (final th in ['dark', 'light', 'dark']) {
      await app.settings.setValue('appearance.theme', th);
      await Future<void>.delayed(const Duration(milliseconds: 400));
    }
  });
}
