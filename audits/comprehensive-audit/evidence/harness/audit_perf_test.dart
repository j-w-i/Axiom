// AUDIT ONLY (disposable copy): profile-mode measurements. Memory (RSS of the shell and the
// daemon) across repeated work, latency of large-file open, frame timings of scenarios the
// committed frames_test does not cover.

import 'dart:convert';
import 'dart:io';

import 'package:axiom_desktop/app/app_state.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:re_editor/re_editor.dart';

import 'editor_frames_test.dart' show bigRust;
import 'frame_report.dart';
import 'support.dart';

int rssKb(int pid) {
  try {
    final s = File('/proc/$pid/status').readAsStringSync();
    final m = RegExp(r'VmRSS:\s+(\d+)').firstMatch(s);
    return int.parse(m!.group(1)!);
  } on Object {
    return -1;
  }
}

int pssKb(int pid) {
  try {
    final s = File('/proc/$pid/smaps_rollup').readAsStringSync();
    final m = RegExp(r'Pss:\s+(\d+)').firstMatch(s);
    return int.parse(m!.group(1)!);
  } on Object {
    return -1;
  }
}

/// CPU ticks (utime+stime) of a process.
int ticks(int pid) {
  try {
    final s = File('/proc/$pid/stat').readAsStringSync();
    final f = s.substring(s.lastIndexOf(')') + 2).split(' ');
    return int.parse(f[11]) + int.parse(f[12]);
  } on Object {
    return -1;
  }
}

/// All descendants' pids of [pid].
List<int> children(int pid) {
  final out = <int>[];
  for (final e in Directory('/proc').listSync()) {
    final n = int.tryParse(e.path.split('/').last);
    if (n == null) continue;
    try {
      final s = File('/proc/$n/stat').readAsStringSync();
      final f = s.substring(s.lastIndexOf(')') + 2).split(' ');
      if (int.parse(f[1]) == pid) out.add(n);
    } on Object {
      continue;
    }
  }
  return out;
}

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  binding.framePolicy = LiveTestWidgetsFlutterBindingFramePolicy.fullyLive;

  testWidgets('audit perf', timeout: const Timeout(Duration(minutes: 20)), (tester) async {
    final res = <String, Object?>{};
    final data = tempDir('axiom-audit-perf-data-');
    final ws = tempDir('axiom-audit-perf-ws-');
    for (var i = 0; i < 20; i++) {
      writeFile(ws, 'src/f$i.rs', bigRust(2000));
    }
    writeFile(ws, 'big/huge.rs', bigRust(100000));
    for (var i = 0; i < 3000; i++) {
      writeFile(ws, 'many/file_$i.txt', 'needle one\nneedle two\n');
    }
    final self = pid;
    Future<void> rest([int ms = 1500]) => Future<void>.delayed(Duration(milliseconds: ms));
    Map<String, int> mem(AppState app) {
      final d = app.client.pid ?? -1;
      return {
        'shell_rss_kb': rssKb(self),
        'shell_pss_kb': pssKb(self),
        'daemon_rss_kb': rssKb(d),
        'daemon_pss_kb': pssKb(d),
        'daemon_children': children(d).length,
      };
    }

    final sw = Stopwatch()..start();
    final app = await boot(tester, data.path);
    res['boot_to_ready_ms'] = sw.elapsedMilliseconds;
    await rest();
    res['mem_0_no_folder'] = mem(app);

    sw.reset();
    await app.workspace.openFolder(ws.path);
    await until(tester, () => app.workspace.rootLoaded, what: 'root');
    res['open_folder_ms'] = sw.elapsedMilliseconds;
    await rest(3000);
    res['mem_1_folder'] = mem(app);

    // Idle CPU with a folder and a document open (caret settled), 10 s.
    await app.documents.open('src/f0.rs');
    await rest(12000);
    final d0 = app.client.pid!;
    final t0 = ticks(self), dt0 = ticks(d0);
    final w = Stopwatch()..start();
    await rest(10000);
    final tick = 100; // CLK_TCK on Linux
    res['idle_cpu_pct_shell'] = (ticks(self) - t0) / tick / (w.elapsedMilliseconds / 1000) * 100;
    res['idle_cpu_pct_daemon'] = (ticks(d0) - dt0) / tick / (w.elapsedMilliseconds / 1000) * 100;

    // Memory across 10 rounds of opening and closing 20 files of 2000 lines.
    final rounds = <Map<String, int>>[];
    for (var r = 0; r < 20; r++) {
      for (var i = 0; i < 20; i++) {
        await app.documents.open('src/f$i.rs');
        await tester.pump(const Duration(milliseconds: 30));
      }
      await rest(500);
      for (var i = 0; i < 20; i++) {
        app.documents.close('src/f$i.rs');
        await tester.pump(const Duration(milliseconds: 10));
      }
      await rest(800);
      rounds.add(mem(app));
    }
    res['mem_open_close_rounds'] = rounds;

    // A 100 000-line file: time to open, frames while scrolling, typing at the end.
    sw.reset();
    await app.documents.open('big/huge.rs');
    await until(tester, () => app.documents.activePath == 'big/huge.rs', what: 'huge');
    res['open_100k_lines_ms'] = sw.elapsedMilliseconds;
    final firstFrames = await measureFrames('huge first second', () => rest(1000));
    res['huge_first_second'] = firstFrames.toJson();
    res['mem_2_huge_open'] = mem(app);
    final ed = find.byType(CodeEditor).last;
    res['huge_fling'] = (await measureFrames('huge fling', () async {
      for (var i = 0; i < 10; i++) {
        await tester.fling(ed, const Offset(0, -1500), 6000);
        await rest(150);
      }
    })).toJson();
    final doc = app.documents.doc('big/huge.rs')!;
    doc.controller.selection = CodeLineSelection.collapsed(index: doc.controller.codeLines.length - 1, offset: 0);
    await rest(300);
    res['huge_typing_at_end'] = (await measureFrames('huge typing', () async {
      for (final ch in 'let audit_value = 42;'.split('')) {
        doc.controller.replaceSelection(ch);
        await tester.pump(const Duration(milliseconds: 40));
      }
    })).toJson();
    res['huge_typing_at_start'] = (await measureFrames('huge typing start', () async {
      doc.controller.selection = const CodeLineSelection.collapsed(index: 0, offset: 0);
      for (final ch in '// start edit'.split('')) {
        doc.controller.replaceSelection(ch);
        await tester.pump(const Duration(milliseconds: 40));
      }
    })).toJson();
    await rest(2000);
    res['mem_3_huge_edited'] = mem(app);

    // Search with 6000 hits, then fling the results.
    await key(tester, LogicalKeyboardKey.keyF, mod: true, shift: true);
    await rest(300);
    sw.reset();
    res['search_render'] = (await measureFrames('search 6000 hits', () async {
      app.search.update(query: 'needle');
      app.search.now();
      await until(tester, () => app.search.total > 0 && !app.search.running, what: 'hits', timeout: const Duration(seconds: 90),
          detail: () => 'query=${app.search.query} running=${app.search.running} total=${app.search.total} error=${app.search.error}');
      res['search_total'] = app.search.total;
      res['search_first_hit_ms'] = sw.elapsedMilliseconds;
      await rest(2500);
    })).toJson();
    final list = find.byType(Scrollable).evaluate().isNotEmpty ? find.byType(Scrollable).first : null;
    if (list != null) {
      res['search_fling'] = (await measureFrames('search fling', () async {
        for (var i = 0; i < 6; i++) {
          await tester.fling(list, const Offset(0, -1200), 5000);
          await rest(200);
        }
      })).toJson();
    }
    res['mem_4_search'] = mem(app);

    // Console flood: 200 000 lines.
    await tapLabelSafe(tester, 'Files');
    await key(tester, LogicalKeyboardKey.keyJ, mod: true);
    await rest(500);
    sw.reset();
    res['console_flood'] = (await measureFrames('console flood', () async {
      app.tasks.run('seq 1 200000');
      await rest(6000);
    })).toJson();
    res['mem_5_console'] = mem(app);

    // Typing in a normal file while a stream floods the console.
    await app.documents.open('src/f1.rs');
    await rest(500);
    final d1 = app.documents.doc('src/f1.rs')!;
    d1.controller.selection = const CodeLineSelection.collapsed(index: 3, offset: 0);
    app.tasks.run('for i in \$(seq 1 4000); do echo line-\$i-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx; sleep 0.001; done');
    res['typing_under_stream'] = (await measureFrames('typing under stream', () async {
      for (final ch in 'let under_stream = 1;'.split('')) {
        d1.controller.replaceSelection(ch);
        await tester.pump(const Duration(milliseconds: 60));
      }
    })).toJson();
    await rest(3000);
    res['mem_6_end'] = mem(app);

    sw.reset();
    await app.shutdown();
    res['shutdown_ms'] = sw.elapsedMilliseconds;

    final out = Platform.environment['AUDIT_OUT'] ?? '/tmp';
    File('$out/perf/audit-perf-${DateTime.now().millisecondsSinceEpoch}.json')
      ..parent.createSync(recursive: true)
      ..writeAsStringSync(const JsonEncoder.withIndent('  ').convert(res));
    // ignore: avoid_print
    print('AUDITPERF ${jsonEncode(res)}');
    binding.reportData = res;
    try {
      data.deleteSync(recursive: true);
      ws.deleteSync(recursive: true);
    } on Object catch (_) {}
  });
}

Future<void> tapLabelSafe(WidgetTester t, String label) async {
  final f = named(label);
  if (f.evaluate().isNotEmpty) {
    await t.tap(f.first);
    await t.pump(const Duration(milliseconds: 200));
  }
}
