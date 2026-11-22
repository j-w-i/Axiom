// Environments (FL-ENV-001), as the Tauri smoke suite's 24a–24e, against the real daemon and two
// user providers whose single-file "toolchains" come from a loopback HTTP server (the daemon
// allows plain HTTP to loopback only with AXIOM_TEST_INSECURE_LOOPBACK_HTTP): `demo` downloads
// at once, `slowdemo` trickles so its install can be cancelled mid-download.
// 24a the page lists the folder's toolchains (Rust's tools; Demo missing with Plan install…) and
//     the rail says how many need attention; 24b the plan shows every step, Install runs it and
//     the row is Ready with the managed version; 24c the transport shows an install, Cancel stops
//     it and leaves nothing behind; 24d Remove names the directory, removes it, and the row is
//     missing again; 24e Files returns to the editor with its tabs.
// Run: apps/desktop-flutter/scripts/e2e.sh env_test.dart

import 'dart:convert';
import 'dart:io';

import 'package:axiom_desktop/app/app_state.dart';
import 'package:axiom_desktop/console/output.dart';
import 'package:axiom_desktop/env/env_page.dart';
import 'package:axiom_desktop/state/environments.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'support.dart';

String _tool(String name, String version, [int pad = 0]) =>
    '#!/bin/sh\necho "$name $version"\n${'# padding\n' * ((pad / 10).ceil())}';

/// The SHA-256 of [bytes], from the system's tool (no package for one fixture).
Future<String> _sha256(Directory dir, List<int> bytes) async {
  final f = File('${dir.path}/.digest')..writeAsBytesSync(bytes);
  final r = Platform.isMacOS
      ? await Process.run('shasum', ['-a', '256', f.path])
      : await Process.run('sha256sum', [f.path]);
  f.deleteSync();
  return (r.stdout as String).split(' ').first.trim();
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('environments: plan, install, cancel, remove', (tester) async {
    final semantics = tester.ensureSemantics();
    final data = tempDir('axiom-fl-env-data-');
    final ws = tempDir('axiom-fl-env-ws-');
    addTearDown(() {
      data.deleteSync(recursive: true);
      ws.deleteSync(recursive: true);
    });
    final files = {
      '/demotool': (body: utf8.encode(_tool('demo-tool', '1.0.0')), slow: false),
      '/slowtool': (body: utf8.encode(_tool('slow-tool', '2.0.0', 96 * 1024)), slow: true),
    };
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    addTearDown(() => server.close(force: true));
    server.listen((req) async {
      final f = files[req.uri.path];
      final res = req.response;
      if (f == null) {
        res.statusCode = 404;
        await res.close();
        return;
      }
      res.contentLength = f.body.length;
      if (req.method == 'HEAD') {
        await res.close();
        return;
      }
      if (!f.slow) {
        res.add(f.body);
        await res.close();
        return;
      }
      try {
        for (var at = 0; at < f.body.length; at += 1024) {
          res.add(f.body.sublist(at, at + 1024 > f.body.length ? f.body.length : at + 1024));
          await res.flush();
          await Future<void>.delayed(const Duration(milliseconds: 20));
        }
        await res.close();
      } on Object {
        // The client went away (the cancel).
      }
    });
    final base = 'http://127.0.0.1:${server.port}';
    for (final (id, name, program, version) in [
      ('demo', 'Demo', 'demotool', '1.0.0'),
      ('slowdemo', 'Slow Demo', 'slowtool', '2.0.0'),
    ]) {
      final digest = await _sha256(data, files['/$program']!.body);
      final tool = program == 'demotool' ? 'demo-tool' : 'slow-tool';
      writeFile(
        data,
        'providers/$id/provider.toml',
        'schema = "axiom.provider/1"\nid = "$id"\nname = "$name"\ndescription = "e2e fixture"\n'
            'version = "0.1.0"\n[project]\nany_file = ["Cargo.toml"]\n'
            '[[tools]]\nid = "$program"\nprogram = "$program"\n'
            "version_regex = '$tool (\\d+\\.\\d+\\.\\d+)'\n"
            '[[install]]\nkind = "archive"\nversions = { kind = "static", versions = ["$version"] }\n'
            'bin_dirs = ["."]\n'
            // The same single file on each OS the suite runs on.
            '${[
              for (final os in ['linux', 'macos']) '[[install.artifacts]]\nplatform = { os = "$os" }\n'
                    'url = "$base/$program"\nformat = "file"\n'
                    'checksum = { kind = "sha256", values = { "$version" = "$digest" } }\n',
            ].join()}',
      );
    }
    writeFile(
      ws,
      'Cargo.toml',
      '[package]\nname = "e"\nversion = "0.1.0"\nedition = "2021"\n\n[workspace]\n',
    );
    writeFile(ws, 'src/main.rs', 'fn main() {}\n');

    final app = await boot(tester, data.path, env: {'AXIOM_TEST_INSECURE_LOOPBACK_HTTP': '1'});
    await app.workspace.openFolder(ws.path);
    await until(tester, () => app.workspace.isOpen, what: 'the folder');
    await app.documents.open('src/main.rs');
    await tester.pump(const Duration(milliseconds: 200));
    ToolchainRow row(String id) => app.environments.rows().firstWhere((r) => r.providerId == id);

    // 24a. The rail's Environments: the page lists Rust's tools, Demo missing with Plan install…
    await until(
      tester,
      () => app.environments.status == 'ready' && app.environments.needAttention > 0,
      what: 'detection',
    );
    await tester.tap(
      find
          .byWidgetPredicate(
            (w) => w is Semantics && (w.properties.label ?? '').startsWith('Environments'),
          )
          .first,
    );
    await until(
      tester,
      () => find.byType(EnvironmentsPage).evaluate().isNotEmpty,
      what: 'the page',
    );
    expect(app.destination, Destination.environments);
    expect(row('rust').tools, ['cargo', 'rustc', 'rustup']);
    expect(row('demo').kind, RowKind.missing);
    expect(named('Plan install…'), findsWidgets);
    expect(find.bySemanticsLabel(RegExp(r'^Environments, \d+ need attention$')), findsOneWidget);

    // 24b. The plan shows every step; Install runs it; the row is Ready, managed.
    await app.environments.planInstall('demo');
    await until(tester, () => find.byType(PlanDialog).evaluate().isNotEmpty, what: 'the plan');
    final plan = tester.widget<PlanDialog>(find.byType(PlanDialog)).plan;
    expect(find.text('Install Demo 1.0.0'), findsOneWidget);
    expect(plan.steps.length, 3);
    expect(find.textContaining('Download demotool'), findsOneWidget);
    expect(find.textContaining('$base/demotool'), findsOneWidget);
    expect(find.textContaining('SHA-256'), findsOneWidget);
    expect(find.text('Check that demotool runs'), findsOneWidget);
    await until(tester, () => named('Steps').evaluate().isNotEmpty, what: 'the named steps');
    await tester.tap(named('Install'));
    await until(
      tester,
      () => row('demo').kind == RowKind.ready && row('demo').source?.kind == 'managed',
      timeout: const Duration(seconds: 20),
      what: 'Demo ready',
    );
    expect(row('demo').detected, '1.0.0');
    expect(row('demo').removable, '1.0.0');

    // 24c. A slow install shows in the transport; Cancel stops it and leaves nothing behind.
    await tester.pump(const Duration(milliseconds: 400));
    await app.environments.planInstall('slowdemo');
    await until(tester, () => named('Install').evaluate().isNotEmpty, what: 'slow plan');
    await tester.tap(named('Install'));
    await until(tester, () => row('slowdemo').kind == RowKind.installing, what: 'installing');
    await tester.pump(const Duration(milliseconds: 300));
    expect(named('Installing Slow Demo'), findsWidgets, reason: 'the transport shows it');
    expect(find.byType(Transport), findsOneWidget);
    await tester.tap(named('Cancel'));
    await until(
      tester,
      () => row('slowdemo').kind == RowKind.error,
      timeout: const Duration(seconds: 10),
      what: 'cancelled',
    );
    expect(row('slowdemo').error, 'The install was cancelled.');
    await tester.pump(const Duration(milliseconds: 300));
    List<String> left(String dir) => Directory(dir).existsSync()
        ? [for (final e in Directory(dir).listSync()) e.path.split('/').last]
        : const [];
    expect(left('${data.path}/toolchains/slowdemo'), isEmpty);
    expect(left('${data.path}/cache').where((f) => f.startsWith('.part')), isEmpty);

    // 24d. Remove names the directory, removes it; the row is missing again.
    await tester.tap(named('Remove Demo 1.0.0'));
    await until(
      tester,
      () => find.textContaining('toolchains/demo/1.0.0').evaluate().isNotEmpty,
      what: 'the remove question',
    );
    await until(tester, () => named('Remove').evaluate().isNotEmpty, what: 'its Remove');
    await tester.tap(named('Remove'));
    await until(
      tester,
      () => row('demo').kind == RowKind.missing,
      timeout: const Duration(seconds: 10),
      what: 'Demo missing again',
    );
    expect(Directory('${data.path}/toolchains/demo/1.0.0').existsSync(), isFalse);

    // 24e. Files returns to the explorer and the editor with its tabs.
    await tester.tap(named('Files'));
    await until(
      tester,
      () => find.byType(EnvironmentsPage).evaluate().isEmpty,
      what: 'the editor back',
    );
    expect(app.documents.docs.map((d) => d.path), contains('src/main.rs'));

    await quit(tester, app);
    semantics.dispose();
  });
}
