import 'dart:convert';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:axiom_desktop/app/app_state.dart';
import 'package:axiom_desktop/chat/chat_page.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'support.dart';

const lib = '''
/// Orbital mechanics for small bodies.

/// Standard gravitational parameter of the Sun, in km³/s².
pub const MU_SUN: f64 = 1.327_124_4e11;

/// An elliptical orbit around a central body.
#[derive(Debug, Clone, PartialEq)]
pub struct Orbit {
    /// Semi-major axis, in km.
    pub axis: f64,
    /// Eccentricity: 0 is a circle.
    pub eccentricity: f64,
}

impl Orbit {
    pub fn new(axis: f64, eccentricity: f64) -> Self {
        Self { axis, eccentricity }
    }

    /// Closest distance to the central body.
    pub fn periapsis(&self) -> f64 {
        self.axis * (1.0 - self.eccentricity)
    }

    /// Farthest distance from the central body.
    pub fn apoapsis(&self) -> f64 {
        self.axis * (1.0 + self.eccentricity)
    }
''';

Future<void> loadFonts() async {
  for (final (family, files) in [
    ('IBM Plex Sans', ['IBMPlexSans-Regular', 'IBMPlexSans-Medium', 'IBMPlexSans-SemiBold', 'IBMPlexSans-Italic']),
    ('IBM Plex Mono', ['IBMPlexMono-Regular', 'IBMPlexMono-Medium', 'IBMPlexMono-Italic']),
  ]) {
    final l = FontLoader(family);
    for (final f in files) {
      l.addFont(Future.value(ByteData.sublistView(File('assets/fonts/$f.ttf').readAsBytesSync())));
    }
    await l.load();
  }
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  testWidgets('shots', (tester) async {
    final out = Platform.environment['AXIOM_SHOTS']!;
    final only = Platform.environment['AXIOM_ONLY']?.split(',');
    Directory(out).createSync(recursive: true);
    await loadFonts();
    debugDefaultTargetPlatformOverride = TargetPlatform.macOS;
    final errors = <String>[];
    final previous = FlutterError.onError;
    FlutterError.onError = (d) => errors.add(d.toString().split('\n').take(12).join('\n'));
    final data = tempDir('axiom-shots-data-');
    final wsRoot = tempDir('axiom-shots-ws-');
    final ws = Directory('${wsRoot.path}/orbit')..createSync();
    writeFile(ws, 'Cargo.toml', '[package]\nname = "orbit"\nversion = "0.1.0"\nedition = "2021"\n\n[workspace]\n');
    writeFile(ws, '.gitignore', 'target/\n');
    writeFile(ws, 'src/lib.rs', '$lib}\n');
    writeFile(ws, 'src/main.rs', 'use orbit::Orbit;\n\nfn main() {\n    let mars = Orbit::new(227_939_366.0, 0.0935);\n    println!("{}", mars.periapsis());\n}\n');
    writeFile(ws, 'docs/guide.md', '# Guide\n');
    writeFile(ws, 'web/app.ts', 'export {}\n');
    writeFile(ws, 'web/index.html', '<p>\n');
    writeFile(ws, 'web/theme.css', 'p{}\n');
    writeFile(ws, 'data.json', '{}\n');
    writeFile(ws, 'run.sh', 'ls\n');
    writeFile(ws, 'README.md', '# orbit\n');
    Process.runSync('git', ['init', '-q', '-b', 'main'], workingDirectory: ws.path);
    Process.runSync('git', ['add', '-A'], workingDirectory: ws.path);
    Process.runSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@e', 'commit', '-qm', 'i'], workingDirectory: ws.path);
    writeFile(ws, 'src/lib.rs', '$lib\n    /// Whether the orbit is closer to a circle than most planets\' are.\n    pub fn nearly_circular(&self) -> bool {\n        self.eccentricity < 0.02\n    }\n}\n');
    writeFile(ws, 'notes.txt', 'n\n');
    final script = '${data.path}/script.json';
    File(script).writeAsStringSync(jsonEncode({
      'turns': [
        {
          'expect': {'userContains': 'guard'},
          'events': [
            {'type': 'text', 'text': 'I will reject eccentricities that do not describe an ellipse.'},
            {
              'type': 'toolCall', 'id': 'c1', 'name': 'fs.applyPatch',
              'input': {'path': 'src/lib.rs', 'edits': [{'search': '        Self { axis, eccentricity }', 'replace': '        assert!((0.0..1.0).contains(&eccentricity), "not an ellipse");\n        Self { axis, eccentricity }'}]},
            },
          ],
        },
        {
          'expect': {'toolResult': {'callId': 'c1', 'isError': false}},
          'events': [{'type': 'text', 'text': 'Done: `Orbit::new` now panics outside `0..1`.'}],
        },
        {
          'expect': {'userContains': 'difference'},
          'events': [{'type': 'text', 'text': 'They are the two ends of an ellipse: **periapsis** is the closest point to the central body, **apoapsis** the farthest.\n\n| Point | Formula | Ceres |\n|---|---|---|\n| Periapsis | `a (1 - e)` | 381.5 million km |\n| Apoapsis | `a (1 + e)` | 446.5 million km |\n\nIn this crate:\n\n```rust\npub fn periapsis(&self) -> f64 {\n    self.axis * (1.0 - self.eccentricity)\n}\n```\n'}],
        },
      ],
    }));
    tester.view.devicePixelRatio = 2;
    tester.view.physicalSize = const Size(2880, 1800);
    Future<void> settle([int n = 14]) async {
      for (var i = 0; i < n; i++) {
        await tester.pump(const Duration(milliseconds: 50));
      }
    }
    Future<void> shot(String name) async {
      if (only != null && !only.any(name.contains)) return;
      await settle();
      final layer = tester.binding.renderViews.first.debugLayer! as OffsetLayer;
      final img = await layer.toImage(const Offset(0, 0) & const Size(2880, 1800), pixelRatio: 0.5);
      final png = await img.toByteData(format: ui.ImageByteFormat.png);
      File('$out/$name.png').writeAsBytesSync(png!.buffer.asUint8List());
    }
    AppState? app;
    try {
      app = await boot(tester, data.path, env: {
        'AXIOM_TEST_SCRIPTED_MODEL': script,
        'AXIOM_TEST_CREDENTIALS_FILE': '${data.path}/credentials.json',
      });
      await app.settings.setValue('appearance.theme', 'dark');
      await shot('01-welcome');
      await app.workspace.openFolder(ws.path);
      await until(tester, () => app!.workspace.rootLoaded, what: 'root');
      app.workspace.setExpanded('src', true);
      app.workspace.setExpanded('web', true);
      await app.documents.open('src/main.rs');
      await app.documents.open('src/lib.rs');
      await until(tester, () => app!.git.marks.isNotEmpty, what: 'marks');
      await settle(30);
      await shot('02-editor');
      await run(tester, app, shot, settle);
      // ignore: avoid_print
      print('SHOTS DONE');
    } finally {
      for (final e in errors) {
        // ignore: avoid_print
        print('FRAMEWORK ERROR: $e');
      }
      if (app != null) await quit(tester, app);
      FlutterError.onError = previous;
      debugDefaultTargetPlatformOverride = null;
      tester.view.resetPhysicalSize();
      tester.view.resetDevicePixelRatio();
      data.deleteSync(recursive: true);
      wsRoot.deleteSync(recursive: true);
    }
  });
}

Future<void> run(WidgetTester tester, AppState app, Future<void> Function(String) shot, Future<void> Function([int]) settle) async {
  Future<void> cmd(LogicalKeyboardKey k, {bool shift = false}) => key(tester, k, mod: true, shift: shift);
  await cmd(LogicalKeyboardKey.keyK);
  await shot('03-palette');
  await key(tester, LogicalKeyboardKey.escape);
  await cmd(LogicalKeyboardKey.keyF, shift: true);
  await settle();
  app.search.update(query: 'eccentricity');
  await settle(20);
  await shot('04-search');
  await cmd(LogicalKeyboardKey.keyG, shift: true);
  await settle();
  app.git.openDiff('src/lib.rs');
  await shot('05-diff');
  app.git.closeDiff();
  await cmd(LogicalKeyboardKey.keyE, shift: true);
  app.documents.activate('src/lib.rs');
  await cmd(LogicalKeyboardKey.keyJ);
  app.prefs.set('console.tab', 'output');
  await settle();
  app.tasks.run('ls -la && echo done');
  await settle(30);
  await shot('06-console');
  await cmd(LogicalKeyboardKey.keyJ);
  await cmd(LogicalKeyboardKey.keyL);
  await until(tester, () => app.agent.status == 'ready', what: 'providers');
  await settle();
  final v = tester.state<ChatViewState>(find.byType(ChatView).first);
  v.composer.text = 'Add a guard against invalid eccentricity';
  v.composerFocus.requestFocus();
  await settle(4);
  await key(tester, LogicalKeyboardKey.enter);
  await until(tester, () => app.agent.approvals.isNotEmpty, what: 'approval');
  await shot('07-approval');
  await tester.tap(named('Allow once').first);
  await until(tester, () => !app.agent.active.running && app.agent.approvals.isEmpty, what: 'turn end');
  v.composer.text = 'What is the difference between periapsis and apoapsis?';
  v.composerFocus.requestFocus();
  await settle(4);
  await key(tester, LogicalKeyboardKey.enter);
  await settle(30);
  await shot('08-dock');
}
