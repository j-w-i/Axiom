// The M2 exit scenario in Flutter (FL-ENV-001; the Tauri suite env.mjs), on a machine without
// Rust, through the UI and the protocol only: a Cargo project → Rust is missing, selected by
// `.axiom/project.toml` → the install plan → install a managed toolchain → `cargo build` runs
// inside it → the compile error opens at its line from Problems → Remove deletes that toolchain
// directory and nothing else.
// "Without Rust": the daemon gets a PATH without any directory that holds cargo, rustc or
// rustup, and a fresh empty HOME. It needs the internet (static.rust-lang.org), so it runs only
// with AXIOM_E2E_NETWORK=1 (CI on Ubuntu). Version: AXIOM_E2E_RUST_VERSION (default 1.84.0).
// Run: AXIOM_E2E_NETWORK=1 apps/desktop-flutter/scripts/e2e.sh env_rust_test.dart

import 'dart:io';

import 'package:axiom_desktop/env/env_page.dart';
import 'package:axiom_desktop/shell/palette.dart';
import 'package:axiom_desktop/state/environments.dart';
import 'package:axiom_desktop/ui/controls.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'support.dart';

/// Every path under [dir], relative, sorted.
List<String> snapshot(Directory dir) {
  if (!dir.existsSync()) return const [];
  final out = [
    for (final e in dir.listSync(recursive: true, followLinks: false))
      e.path.substring(dir.path.length + 1),
  ]..sort();
  return out;
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  final network = Platform.environment['AXIOM_E2E_NETWORK'] == '1';
  final version = Platform.environment['AXIOM_E2E_RUST_VERSION'] ?? '1.84.0';

  testWidgets('the M2 exit scenario: install Rust, build, Problems, Remove', skip: !network, (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    final data = tempDir('axiom-fl-envrust-data-');
    final ws = tempDir('axiom-fl-envrust-ws-');
    final home = tempDir('axiom-fl-envrust-home-');
    addTearDown(() {
      for (final d in [data, ws, home]) {
        d.deleteSync(recursive: true);
      }
    });
    writeFile(
      ws,
      'Cargo.toml',
      '[package]\nname = "exit"\nversion = "0.1.0"\nedition = "2021"\n\n[workspace]\n',
    );
    writeFile(
      ws,
      'src/main.rs',
      'fn main() {\n    let x: i32 = "text";\n    println!("{x}");\n}\n',
    );
    writeFile(ws, '.axiom/project.toml', '[environment]\nrust = "$version"\n');
    final path = (Platform.environment['PATH'] ?? '')
        .split(':')
        .where(
          (d) =>
              d.isNotEmpty && !['cargo', 'rustc', 'rustup'].any((t) => File('$d/$t').existsSync()),
        )
        .join(':');
    final app = await boot(tester, data.path, env: {'PATH': path, 'HOME': home.path});
    await app.workspace.openFolder(ws.path);
    await until(tester, () => app.workspace.isOpen, what: 'the folder');
    ToolchainRow rust() => app.environments.rows().firstWhere((r) => r.providerId == 'rust');

    // 1. Rust is missing, selected by .axiom/project.toml, with Plan install…
    // The rail's Environments (its name carries the count once detection has run).
    Finder rail() =>
        find.byWidgetPredicate((w) => w is AxIconButton && w.label.startsWith('Environments'));
    await until(tester, () => app.environments.status == 'ready', what: 'detection');
    await tester.tap(rail());
    await until(
      tester,
      () => app.environments.status == 'ready' && rust().kind == RowKind.missing,
      timeout: const Duration(seconds: 30),
      what: 'Rust missing',
    );
    expect(rust().selected, version);
    expect(rust().detected, isNull);
    // The page shows it once a frame has rebuilt it (the store changes first).
    await until(
      tester,
      () => named('Plan install…').evaluate().isNotEmpty,
      what: 'Plan install…',
      detail: () => 'destination ${app.destination}',
    );

    // 2. The plan shows every step.
    await app.environments.planInstall('rust');
    await until(
      tester,
      () => named('Install').evaluate().isNotEmpty,
      timeout: const Duration(seconds: 60),
      what: 'the plan',
    );
    final plan = tester.widget<PlanDialog>(find.byType(PlanDialog)).plan;
    expect(plan.steps.length, 6);
    expect(find.textContaining('Download rustup-init'), findsOneWidget);
    expect(find.textContaining('.sha256'), findsOneWidget);
    expect(find.textContaining('toolchain install $version'), findsOneWidget);
    expect(find.text('Check that cargo runs'), findsOneWidget);
    expect(find.text('Check that rustc runs'), findsOneWidget);

    // 3. Install, through to Ready with the managed toolchain.
    await tester.tap(named('Install'));
    await until(
      tester,
      () => rust().kind == RowKind.ready || rust().kind == RowKind.error,
      timeout: const Duration(minutes: 15),
      what: 'Rust installed',
    );
    expect(rust().kind, RowKind.ready, reason: rust().error);
    expect(rust().detected, version);
    expect(rust().source?.kind, 'managed');
    final root = Directory('${data.path}/toolchains/rust/$version');
    expect(root.existsSync(), isTrue);
    bool notToolchain(String p) =>
        !p.startsWith('cache') && !RegExp(r'^shell-local\.json(\.tmp)?$').hasMatch(p);
    final afterInstall = snapshot(data).where(notToolchain).toList();

    // 4. Build: cargo build from the palette runs inside it and fails on the type error.
    await tester.tap(named('Files'));
    await tester.pump(const Duration(milliseconds: 300));
    await key(tester, LogicalKeyboardKey.keyK, mod: true);
    await until(tester, () => find.byType(Palette).evaluate().isNotEmpty, what: 'the palette');
    await typeFocused(tester, 'Build: cargo build');
    await tester.pump(const Duration(milliseconds: 100));
    await key(tester, LogicalKeyboardKey.enter);
    await until(
      tester,
      () => !app.tasks.running && app.tasks.lastOutcome != null,
      timeout: const Duration(minutes: 5),
      what: 'the build',
    );
    expect(app.tasks.allText(), matches(RegExp(r'error\[E0308\]|mismatched types')));

    // 5. The error is in Problems and opens src/main.rs at 2:18.
    app.prefs.set('console.tab', 'problems');
    app.consoleVisible = true;
    const label = 'Error: mismatched types (E0308) at src/main.rs:2:18, from rustc';
    await until(tester, () => named(label).evaluate().isNotEmpty, what: 'the problem');
    await tester.tap(named(label));
    await until(
      tester,
      () =>
          app.documents.activePath == 'src/main.rs' &&
          app.documents.cursorText.startsWith('Ln 2, Col 18'),
      what: 'the file at the error',
    );

    // 6. Remove deletes that directory and nothing else; HOME has no .cargo or .rustup.
    await tester.tap(rail());
    await until(tester, () => named('Remove Rust $version').evaluate().isNotEmpty, what: 'Remove');
    await tester.tap(named('Remove Rust $version'));
    await until(tester, () => named('Remove').evaluate().isNotEmpty, what: 'the question');
    expect(find.textContaining(version), findsWidgets);
    await tester.tap(named('Remove'));
    await until(
      tester,
      () => rust().kind == RowKind.missing,
      timeout: const Duration(minutes: 2),
      what: 'Rust missing again',
    );
    final prefix = 'toolchains/rust/$version';
    final expected = afterInstall.where((p) => p != prefix && !p.startsWith('$prefix/')).toList();
    expect(snapshot(data).where(notToolchain).toList(), expected);
    expect(root.existsSync(), isFalse);
    final homeAfter = snapshot(home)
        .where((p) => !RegExp(r'^(\.cache|\.local|\.config)').hasMatch(p));
    expect(homeAfter.where((p) => p.startsWith('.cargo') || p.startsWith('.rustup')), isEmpty);

    await quit(tester, app);
    semantics.dispose();
  });
}
