// M5.5-UI-001 (ADR-0021 §5): a language server that is not installed is offered above the editor;
// Install… shows the plan, approving installs it and the server starts; Not now hides the offer for
// the language; with "Install language servers automatically" on, it installs with no dialog.
//
// The servers are user providers whose "download" is tests/helpers/fake-lsp, served from loopback
// (AXIOM_TEST_INSECURE_LOOPBACK_HTTP, as env_test.dart) under a name that is not on PATH, so it
// runs only once installed. Run: apps/desktop-flutter/scripts/e2e.sh lsp_install_test.dart

import 'dart:io';

import 'package:axiom_desktop/daemon/protocol.g.dart';
import 'package:axiom_desktop/env/env_page.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'support.dart';

Future<String> _sha256(String path) async {
  final r = Platform.isMacOS
      ? await Process.run('shasum', ['-a', '256', path])
      : await Process.run('sha256sum', [path]);
  return (r.stdout as String).split(' ').first.trim();
}

String _provider(String id, String name, String language, String program, String url, String sha) {
  final os = Platform.isMacOS ? 'macos' : 'linux';
  return '''
schema = "axiom.provider/1"
id = "$id"
name = "$name"
description = "Test: fake-lsp as a $language server, installed from loopback."
version = "0.1.0"

[[tools]]
id = "$program"
program = "$program"
version_regex = '(\\d+\\.\\d+\\.\\d+)'
version_args = []
required = false

[[install]]
kind = "archive"
versions = { kind = "static", versions = ["1.0.0"] }
bin_dirs = ["."]

[[install.artifacts]]
platform = { os = "$os" }
url = "$url"
format = "file"
file_name = "$program"
checksum = { kind = "sha256", values = { "1.0.0" = "$sha" } }

[[lsp]]
id = "$program"
languages = ["$language"]
command = ["$program"]
''';
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('a missing server: offered, Not now, Install…, and installed automatically', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    final data = tempDir('axiom-fl-lspi-data-');
    final ws = tempDir('axiom-fl-lspi-ws-');
    addTearDown(() {
      data.deleteSync(recursive: true);
      ws.deleteSync(recursive: true);
    });
    final fake = '${File(Platform.environment['AXIOMD_UNDER_TEST']!).parent.path}/fake-lsp';
    final bytes = File(fake).readAsBytesSync();
    final sha = await _sha256(fake);
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    addTearDown(() => server.close(force: true));
    server.listen((req) async {
      req.response.contentLength = bytes.length;
      if (req.method != 'HEAD') req.response.add(bytes);
      await req.response.close();
    });
    final base = 'http://127.0.0.1:${server.port}';
    for (final (id, name, lang, prog) in [
      ('lsp-fakelua', 'Fake Lua server', 'lua', 'fake-lua-lsp'),
      ('lsp-fakezig', 'Fake Zig server', 'zig', 'fake-zig-lsp'),
      ('lsp-fakenim', 'Fake Nim server', 'nim', 'fake-nim-lsp'),
    ]) {
      writeFile(
        data,
        'providers/$id/provider.toml',
        _provider(id, name, lang, prog, '$base/$prog', sha),
      );
    }
    writeFile(ws, 'main.lua', 'print("hi")\n');
    writeFile(ws, 'main.zig', 'const x = 1;\n');
    writeFile(ws, 'main.nim', 'echo "hi"\n');

    final app = await boot(
      tester,
      data.path,
      env: {'AXIOM_TEST_INSECURE_LOOPBACK_HTTP': '1', 'FAKE_LSP_LOG': '${data.path}/fake.log'},
    );
    await app.workspace.openFolder(ws.path);
    await until(tester, () => app.workspace.isOpen, what: 'the folder');
    ServerInfo? server0(String id) => app.lsp.servers.where((s) => s.serverId == id).firstOrNull;

    // 1. Lua: the offer; Not now hides it for Lua.
    await app.documents.open('main.lua');
    await until(
      tester,
      () => find.text('Lua has a language server: Fake Lua server.').evaluate().isNotEmpty,
      what: 'the Lua offer',
      detail: () => '${server0('fake-lua-lsp')?.toJson()}',
    );
    await tester.tap(named('Not now'));
    await until(
      tester,
      () => find.textContaining('Fake Lua server').evaluate().isEmpty,
      what: 'the offer hidden',
    );

    // 2. Zig: Install… shows the plan; Install runs it; the server starts.
    await app.documents.open('main.zig');
    await until(tester, () => named('Install…').evaluate().isNotEmpty, what: 'the Zig offer');
    await tester.tap(named('Install…'));
    await until(tester, () => named('Install').evaluate().isNotEmpty, what: 'the plan');
    expect(find.byType(PlanDialog), findsOneWidget);
    await tester.tap(named('Install'));
    await until(
      tester,
      () =>
          server0('fake-zig-lsp')?.state == ServerState.ready ||
          server0('fake-zig-lsp')?.state == ServerState.idle,
      timeout: const Duration(seconds: 30),
      what: 'the Zig server installed',
      detail: () => '${server0('fake-zig-lsp')?.toJson()} ${app.environments.failures}',
    );
    expect(Directory('${data.path}/toolchains/lsp-fakezig/1.0.0').existsSync(), isTrue);
    await until(
      tester,
      () => find.textContaining('Fake Zig server').evaluate().isEmpty,
      what: 'the Zig offer gone',
    );

    // 3. Nim, with the switch on: no dialog, it installs and the server starts.
    await app.settings.setValue('lsp.autoInstall', true);
    await app.documents.open('main.nim');
    await until(
      tester,
      () => Directory('${data.path}/toolchains/lsp-fakenim/1.0.0').existsSync(),
      timeout: const Duration(seconds: 30),
      what: 'the Nim server installed on its own',
      detail: () => '${server0('fake-nim-lsp')?.toJson()} ${app.environments.failures}',
    );
    expect(find.byType(PlanDialog), findsNothing);
    await until(
      tester,
      () => server0('fake-nim-lsp')?.state != ServerState.missing,
      timeout: const Duration(seconds: 30),
      what: 'the Nim server no longer missing',
    );
    // Lua stayed dismissed and uninstalled.
    expect(Directory('${data.path}/toolchains/lsp-fakelua').existsSync(), isFalse);

    await quit(tester, app);
    semantics.dispose();
  });
}
