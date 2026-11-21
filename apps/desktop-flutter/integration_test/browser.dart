// Helpers for the suites that need a real page (M5): the repository root, a Chrome, a copy of
// the inspector fixture, and a headless Chrome whose page the test clicks like a person would
// (over the browser's debugging port: the test's hand, not the product's; the product uses no
// CDP, ADR-0019 §1).

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'support.dart';

/// The repository's root (the directory holding scripts/check.sh) above the current one.
String repoRoot() {
  var d = Directory.current;
  while (!File('${d.path}/scripts/check.sh').existsSync()) {
    final up = d.parent;
    if (up.path == d.path) {
      throw StateError('the repository root is not above ${Directory.current}');
    }
    d = up;
  }
  return d.path;
}

/// Chrome or Chromium on PATH.
String? findChrome() {
  for (final dir in (Platform.environment['PATH'] ?? '').split(':')) {
    for (final n in ['google-chrome', 'google-chrome-stable', 'chromium', 'chromium-browser']) {
      if (File('$dir/$n').existsSync()) return '$dir/$n';
    }
  }
  return null;
}

/// A headless Chrome on [url] with a throw-away profile; its page's debugging socket.
final class Page {
  Page._(this.process, this.profile, this._ws);
  final Process process;
  final Directory profile;
  final WebSocket _ws;
  var _id = 0;
  final _replies = <int, Completer<Object?>>{};

  static Future<Page> open(String chrome, String url) async {
    final profile = tempDir('axiom-fl-inspector-chrome-');
    final p = await Process.start(chrome, [
      '--headless=new',
      '--no-first-run',
      '--no-default-browser-check',
      '--disable-gpu',
      '--window-size=1024,768',
      '--remote-debugging-port=0',
      '--user-data-dir=${profile.path}',
      url,
    ]);
    unawaited(p.stdout.drain<void>());
    unawaited(p.stderr.drain<void>());
    final portFile = File('${profile.path}/DevToolsActivePort');
    final end = DateTime.now().add(const Duration(seconds: 20));
    while (!portFile.existsSync() || portFile.readAsLinesSync().isEmpty) {
      if (DateTime.now().isAfter(end)) throw StateError('Chrome did not open its debugging port');
      await Future<void>.delayed(const Duration(milliseconds: 100));
    }
    final port = int.parse(portFile.readAsLinesSync().first.trim());
    String? wsUrl;
    while (wsUrl == null) {
      if (DateTime.now().isAfter(end)) throw StateError('Chrome has no page target');
      final http = HttpClient();
      try {
        final rq = await http.getUrl(Uri.parse('http://127.0.0.1:$port/json/list'));
        final body = await (await rq.close()).transform(utf8.decoder).join();
        final targets = (jsonDecode(body) as List).cast<Map<String, Object?>>();
        wsUrl = targets
            .where((t) => t['type'] == 'page' && '${t['url']}'.startsWith(url))
            .map((t) => t['webSocketDebuggerUrl'] as String?)
            .firstOrNull;
      } finally {
        http.close();
      }
      if (wsUrl == null) await Future<void>.delayed(const Duration(milliseconds: 100));
    }
    final ws = await WebSocket.connect(wsUrl);
    final page = Page._(p, profile, ws);
    ws.listen((m) {
      final j = jsonDecode(m as String) as Map<String, Object?>;
      final c = page._replies.remove(j['id']);
      c?.complete(j['result']);
    });
    return page;
  }

  Future<Object?> _send(String method, Map<String, Object?> params) {
    final id = ++_id;
    final c = _replies[id] = Completer<Object?>();
    _ws.add(jsonEncode({'id': id, 'method': method, 'params': params}));
    return c.future.timeout(const Duration(seconds: 10));
  }

  /// The person's click at (x, y) in CSS px: move there, press, release.
  Future<void> click(double x, double y) async {
    for (final type in ['mouseMoved', 'mousePressed', 'mouseReleased']) {
      await _send('Input.dispatchMouseEvent', {
        'type': type,
        'x': x,
        'y': y,
        if (type != 'mouseMoved') ...{'button': 'left', 'clickCount': 1},
      });
    }
  }

  Future<void> close() async {
    await _ws.close();
    process.kill();
    await process.exitCode.timeout(const Duration(seconds: 5), onTimeout: () => -1);
    try {
      profile.deleteSync(recursive: true);
    } on FileSystemException {
      // Chrome may still be writing its profile; the temp dir is the system's to clear.
    }
  }
}

/// The inspector fixture's sources in [ws], its `node_modules` linked, and a `.gitignore` that
/// keeps checkpoints away from the dependencies.
void copyInspectFixture(String fixture, Directory ws) {
  for (final f in ['package.json', 'vite.config.js', 'index.html']) {
    File('$fixture/$f').copySync('${ws.path}/$f');
  }
  for (final d in ['src', 'lib']) {
    Directory('${ws.path}/$d').createSync();
    for (final e in Directory('$fixture/$d').listSync().whereType<File>()) {
      e.copySync('${ws.path}/$d/${e.uri.pathSegments.last}');
    }
  }
  Link('${ws.path}/node_modules').createSync('$fixture/node_modules');
  writeFile(ws, '.gitignore', 'node_modules\n');
}

/// Why a page cannot be had here (no fixture dependencies, no Chrome), or null.
String? previewUnavailable(String fixture, String? chrome) =>
    !File('$fixture/node_modules/.bin/vite').existsSync()
    ? 'the fixture\'s dependencies are not installed (pnpm install in tests/fixtures/inspect-react)'
    : chrome == null
    ? 'no Chrome or Chromium on PATH'
    : null;
