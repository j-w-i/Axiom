// The console (FL-CONSOLE-001), against the real daemon, as the Tauri suites' smoke 7a–7c, 14,
// 25a/b and terminal.mjs:
// 1. Output: Mod+Shift+R and a command whose stdout and stderr stay apart, with its exit status;
//    Stop (Shift+F5) cancels a running command; a missing program is a structured error; the
//    title row's transport runs, counts the time, and ends in success or failure with Rerun.
// 2. Provider tasks: "Build: cargo build" in the palette builds a broken crate; the transport
//    offers it as Rerun; its error is a Problems row that opens src/main.rs at 2:18.
// 3. Terminal: Mod+Shift+` opens a shell in the folder; a typed command runs; the console's width
//    reaches the shell; the echo of a typed key is on screen within 50 ms (p95); a second
//    terminal is a tab; Mod+` focuses the shown one; Mod+K from the terminal opens the palette;
//    `exit` marks the tab ended; quitting leaves no shell.
// Run: apps/desktop-flutter/scripts/e2e.sh console_test.dart

import 'dart:io';

import 'package:axiom_desktop/app/app_state.dart';
import 'package:axiom_desktop/console/output.dart';
import 'package:axiom_desktop/console/terminal_view.dart';
import 'package:axiom_desktop/daemon/protocol.g.dart';
import 'package:axiom_desktop/shell/palette.dart';
import 'package:axiom_desktop/state/tasks.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'support.dart';

String _rows(AppState app) => app.tasks.rows.map((r) => r.text).join('\n');

String _screen(AppState app, [String? id]) {
  final t = app.terminals.terminals.where((x) => x.info.terminalId == (id ?? app.terminals.active));
  return t.isEmpty ? '' : t.first.screen.buffer.getText();
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('output, Stop, a structured error, the transport', (tester) async {
    final semantics = tester.ensureSemantics();
    final data = tempDir('axiom-fl-console-data-');
    final ws = tempDir('axiom-fl-console-ws-');
    addTearDown(() {
      data.deleteSync(recursive: true);
      ws.deleteSync(recursive: true);
    });
    writeFile(ws, 'README.md', '# console\n');
    final app = await boot(tester, data.path);
    await app.workspace.openFolder(ws.path);
    await until(tester, () => app.workspace.isOpen, what: 'the folder');

    // 7a. Mod+Shift+R focuses the prompt; stdout and stderr stay apart; the exit status shows.
    await key(tester, LogicalKeyboardKey.keyR, mod: true, shift: true);
    final view = find.byType(OutputView);
    await until(tester, () => view.evaluate().isNotEmpty, what: 'the Output view');
    final out = tester.state<OutputViewState>(view);
    await until(tester, () => out.commandFocus.hasFocus, what: 'the prompt focused');
    await typeFocused(
      tester,
      "sh -c 'echo out1; echo err1 >&2; echo out2; echo err2 >&2; echo out3; exit 2'",
    );
    await key(tester, LogicalKeyboardKey.enter);
    await until(tester, () => _rows(app).contains('Failed with exit code 2'), what: 'the exit');
    final rows = app.tasks.rows;
    expect(rows.any((r) => r.stream == StreamKind.stdout && r.text == 'out3'), isTrue);
    expect(rows.any((r) => r.stream == StreamKind.stderr && r.text == 'err2'), isTrue);
    expect(rows.last.kind, RowKind.statusFailed);
    expect(named('stderr: err2'), findsOneWidget, reason: 'a stderr row says so');
    expect(app.tasks.history.first, startsWith('sh -c'));

    // 7b. Stop (Shift+F5) cancels a running command.
    out.command.text = 'sleep 30';
    await key(tester, LogicalKeyboardKey.enter);
    await until(tester, () => app.tasks.running, what: 'running');
    await tester.pump(const Duration(milliseconds: 300));
    final t0 = DateTime.now();
    await key(tester, LogicalKeyboardKey.f5, shift: true);
    await until(
      tester,
      () => _rows(app).contains('■ Cancelled'),
      timeout: const Duration(seconds: 6),
      what: 'the cancel',
    );
    expect(DateTime.now().difference(t0).inSeconds, lessThan(6));

    // 7c. A missing program is a structured error.
    app.tasks.runArgv(['no-such-program-axiom']);
    await until(
      tester,
      () => _rows(app).contains('Could not start no-such-program-axiom'),
      what: 'spawn_failed',
    );

    // 14. The transport runs, counts the time, and ends in success; a failure is its lamp.
    final transport = find.byType(Transport);
    TransportState tr() => tester.state<TransportState>(transport);
    app.tasks.runArgv(['sh', '-c', 'sleep 3; echo done']);
    await until(tester, () => tr().state == 'run' && tr().elapsed != null, what: 'running');
    expect(app.tasks.label, 'sh -c sleep 3; echo done');
    expect(named('Stop'), findsWidgets);
    final first = tr().elapsed;
    await until(tester, () => tr().elapsed != first, what: 'the time advancing');
    expect(tr().elapsed, matches(RegExp(r'^00:0\d$')));
    await until(
      tester,
      () => tr().state == 'on',
      timeout: const Duration(seconds: 8),
      what: 'success',
    );
    expect(tr().elapsed, isNull);
    app.tasks.runArgv(['sh', '-c', 'exit 3']);
    await until(tester, () => tr().state == 'err', what: 'failure');
    expect(named('Rerun: ${app.tasks.history.first}'), findsOneWidget);

    await quit(tester, app);
    semantics.dispose();
  });

  testWidgets('provider tasks in the palette and their problems', (tester) async {
    final semantics = tester.ensureSemantics();
    final data = tempDir('axiom-fl-build-data-');
    final ws = tempDir('axiom-fl-build-ws-');
    addTearDown(() {
      data.deleteSync(recursive: true);
      ws.deleteSync(recursive: true);
    });
    writeFile(
      ws,
      'Cargo.toml',
      '[package]\nname = "broken"\nversion = "0.1.0"\nedition = "2021"\n\n[workspace]\n',
    );
    writeFile(
      ws,
      'src/main.rs',
      'fn main() {\n    let x: i32 = "text";\n    println!("{x}");\n}\n',
    );
    final app = await boot(tester, data.path);
    await app.workspace.openFolder(ws.path);
    await until(
      tester,
      () => app.tasks.providerTasks.any((t) => t.id == 'build'),
      timeout: const Duration(seconds: 15),
      what: 'the provider tasks',
    );

    // 25a. "Build: cargo build" is a palette command; it runs through the transport, which then
    // offers it as Rerun; its error is a problem.
    await key(tester, LogicalKeyboardKey.keyK, mod: true);
    await until(tester, () => find.byType(Palette).evaluate().isNotEmpty, what: 'the palette');
    await typeFocused(tester, 'Build: cargo');
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.textContaining('Build: cargo build'), findsWidgets);
    await key(tester, LogicalKeyboardKey.enter);
    await until(
      tester,
      () => !app.tasks.running && app.tasks.lastOutcome != null,
      timeout: const Duration(minutes: 2),
      what: 'cargo build',
    );
    expect(app.tasks.lastOutcome, Outcome.failed);
    expect(named('Rerun: Build: cargo build'), findsOneWidget);
    await until(tester, () => app.problems.count(Severity.error) >= 1, what: 'the build problem');

    // 25b. The E0308 row opens src/main.rs at line 2, column 18.
    app.prefs.set('console.tab', 'problems');
    await tester.pump(const Duration(milliseconds: 200));
    const label = 'Error: mismatched types (E0308) at src/main.rs:2:18, from rustc';
    await until(tester, () => named(label).evaluate().isNotEmpty, what: 'the Problems row');
    // A click on the row opens the file there (and focuses the editor).
    await tester.tap(named(label));
    await until(
      tester,
      () =>
          app.documents.activePath == 'src/main.rs' &&
          app.documents.cursorText.startsWith('Ln 2, Col 18'),
      what: 'src/main.rs at 2:18',
      detail: () => '${app.documents.activePath} ${app.documents.cursorText}',
    );

    await quit(tester, app);
    semantics.dispose();
  });

  testWidgets('the terminal', (tester) async {
    final semantics = tester.ensureSemantics();
    final data = tempDir('axiom-fl-term-data-');
    final ws = tempDir('axiom-fl-term-ws-');
    addTearDown(() {
      data.deleteSync(recursive: true);
      ws.deleteSync(recursive: true);
    });
    writeFile(ws, 'README.md', '# terminal\n');
    final app = await boot(tester, data.path, env: {'SHELL': '/bin/sh'});
    await app.workspace.openFolder(ws.path);
    await until(tester, () => app.workspace.isOpen, what: 'the folder');
    final real = ws.resolveSymbolicLinksSync();

    // 1. Mod+Shift+` opens a terminal in the folder; a typed command runs.
    await key(tester, LogicalKeyboardKey.backquote, mod: true, shift: true);
    await until(
      tester,
      () => app.terminals.activeTab != null && find.byType(TerminalPane).evaluate().isNotEmpty,
      what: 'a terminal',
    );
    final pane = tester.state<TerminalPaneState>(find.byType(TerminalPane));
    String id() => app.terminals.active!;
    await until(tester, () => pane.focusFor(id()).hasFocus, what: 'the terminal focused');
    Future<void> type(String line) async {
      app.terminals.activeTab!.screen.textInput(line);
      await tester.pump(const Duration(milliseconds: 30));
      await key(tester, LogicalKeyboardKey.enter);
    }

    await type(r'pwd; echo ran-$$');
    await until(
      tester,
      () => _screen(app).contains(real) && RegExp(r'ran-\d+').hasMatch(_screen(app)),
      what: 'the command output',
      detail: () => _screen(app),
    );

    // 2. The console's width reaches the shell; hiding the side bar gives it more columns.
    int? cols() {
      final m = RegExp(r'size (\d+) (\d+)').allMatches(_screen(app)).lastOrNull;
      return m == null ? null : int.parse(m.group(2)!);
    }

    await type(r'echo size $(stty size)');
    await until(tester, () => cols() != null, what: 'stty size');
    final before = cols()!;
    expect(before, app.terminals.activeTab!.screen.viewWidth);
    await key(tester, LogicalKeyboardKey.keyB, mod: true);
    await tester.pump(const Duration(milliseconds: 400));
    pane.focusFor(id()).requestFocus();
    await tester.pump(const Duration(milliseconds: 50));
    await type('clear; echo size \$(stty size)');
    await until(
      tester,
      () => (cols() ?? 0) > before,
      what: 'more columns',
      detail: () => '$before → ${cols()}',
    );
    await key(tester, LogicalKeyboardKey.keyB, mod: true);

    // 3. Typing: a key's echo is on screen within 50 ms (p95).
    pane.focusFor(id()).requestFocus();
    await tester.pump(const Duration(milliseconds: 100));
    final lat = <int>[];
    for (var i = 0; i < 20; i++) {
      final ch = String.fromCharCode(0x61 + i);
      final count = _screen(app).split(ch).length;
      final sw = Stopwatch()..start();
      app.terminals.activeTab!.screen.textInput(ch);
      while (_screen(app).split(ch).length == count && sw.elapsedMilliseconds < 1000) {
        await tester.pump(const Duration(milliseconds: 1));
      }
      lat.add(sw.elapsedMilliseconds);
    }
    lat.sort();
    final p95 = lat[(lat.length * 0.95).floor() - 1];
    // ignore: avoid_print
    print('terminal echo latency ms: p50 ${lat[lat.length ~/ 2]} p95 $p95 max ${lat.last}');
    expect(p95, lessThanOrEqualTo(50));
    await key(tester, LogicalKeyboardKey.keyC, ctrl: true);

    // 4. A second terminal is a tab; Mod+` focuses the shown terminal.
    final firstId = id();
    await tester.tap(named('New terminal'));
    await until(tester, () => app.terminals.terminals.length == 2, what: 'a second terminal');
    expect(id(), isNot(firstId));
    FocusManager.instance.primaryFocus?.unfocus();
    await tester.pump();
    await key(tester, LogicalKeyboardKey.backquote, mod: true);
    await until(tester, () => pane.focusFor(id()).hasFocus, what: 'Mod+` focusing it');

    // 5. Mod+K from the terminal opens the palette; the shell does not get it.
    await key(tester, LogicalKeyboardKey.keyK, mod: true);
    await until(tester, () => find.byType(Palette).evaluate().isNotEmpty, what: 'the palette');
    await key(tester, LogicalKeyboardKey.escape);
    await tester.pump(const Duration(milliseconds: 300));

    // 7. `exit` marks the terminal's tab ended.
    pane.focusFor(id()).requestFocus();
    await tester.pump(const Duration(milliseconds: 50));
    final ended = id();
    await type('exit');
    await until(
      tester,
      () => app.terminals.terminals.any(
        (t) => t.info.terminalId == ended && app.terminals.label(t).endsWith('(ended)'),
      ),
      what: 'the tab ended',
    );

    // 8. Quitting leaves no shell.
    final pids = [
      for (final t in app.terminals.terminals)
        if (t.info.running && t.info.pid != null) t.info.pid!,
    ];
    expect(pids, isNotEmpty);
    await quit(tester, app);
    final end = DateTime.now().add(const Duration(seconds: 5));
    while (pids.any((p) => Directory('/proc/$p').existsSync()) && DateTime.now().isBefore(end)) {
      await Future<void>.delayed(const Duration(milliseconds: 100));
    }
    expect([
      for (final p in pids)
        if (Directory('/proc/$p').existsSync()) p,
    ], isEmpty);
    semantics.dispose();
  });
}
