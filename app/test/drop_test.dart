import 'dart:io';

import 'package:axiom/host/host_store.dart';
import 'package:axiom/host/proto.dart';
import 'package:axiom/host/session_model.dart';
import 'package:axiom/shell/drop.dart';
import 'package:axiom/shell/find.dart';
import 'package:axiom/shell/new_session.dart';
import 'package:axiom/shell/shell.dart';
import 'package:axiom/ui/tokens.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'shot.dart';

/// The system, as the window's own code tells of it.
class FakeDrags implements Drags {
  void Function(String)? over;
  void Function()? out;
  void Function(List<String>)? dropped;
  @override
  void listen({
    required void Function(String folder) over,
    required void Function() out,
    required void Function(List<String> paths) dropped,
  }) {
    this.over = over;
    this.out = out;
    this.dropped = dropped;
  }

  @override
  void stop() => over = out = dropped = null;
}

/// Writes down all that the host is asked to do.
class Asked extends HostStore {
  Asked() : super(paths: HostPaths('/nonexistent'), binary: null, spawn: false);
  final did = <String>[];
  @override
  Future<String> create(String title, {String? workspace}) async {
    did.add('create');
    return 's';
  }

  @override
  Future<String> addAgent(
    String session,
    String name,
    Execution x,
    Access access, {
    List<Instructions> instructions = const [],
    bool trust = false,
    bool apart = false,
  }) async {
    did.add('add');
    return 'a';
  }

  @override
  Future<void> prompt(String session, String agent, String text, {String? key}) async =>
      did.add('prompt');
  @override
  Future<void> open(String id) async => did.add('open');
}

Asked store() => Asked()
  ..phase = HostPhase.connected
  ..current = (SessionModel('s')
    ..apply([
      const Event(1, 1, SessionCreated('Double token rotation', '/me/web')),
      const Event(
        2,
        2,
        AgentAdded('a', 'Lead', Execution(harness: 'axiom'), null, access: Access.ask),
      ),
      const Event(3, 3, RunStarted('r', 'a')),
      const Event(4, 4, ItemStarted('i', 'r', 'a', ItemKind.prose, '', null)),
      const Event(5, 5, ItemEnded('i', Outcome.completed, 'What was said.', null)),
    ]));

void main() {
  late Directory dir;
  setUp(() {
    dir = Directory.systemTemp.createTempSync('axiom-drop');
    dir = Directory(dir.resolveSymbolicLinksSync());
  });
  tearDown(() => dir.deleteSync(recursive: true));

  group('what was dropped', () {
    test('a folder is where a session could begin, by its real name', () async {
      final d = Directory('${dir.path}/a b')..createSync();
      expect(await whatWasDropped([d.path]), (folder: d.path, words: null));
      // As macOS names one.
      if (!Platform.isWindows) {
        expect((await whatWasDropped(['${d.path}/'])).folder, d.path);
      }
    });

    test('what is no folder is said in words', () async {
      final f = File('${dir.path}/notes.txt')..writeAsStringSync('x');
      for (final (paths, words) in [
        ([f.path], 'notes.txt is a file. A session works in a folder.'),
        ([dir.path, f.path], 'Only one folder can be dropped at a time.'),
        ([dir.path, dir.path], 'Only one folder can be dropped at a time.'),
        (<String>[], 'That is not a folder on this computer.'),
        (['${dir.path}/gone'], 'gone is not there any more.'),
        (['relative/x'], 'x is not on this computer.'),
        ([''], contains('is not on this computer.')),
        // What looks like something else, or is endless.
        (
          ['${dir.path}/a\u{202e}b\nc'],
          allOf(startsWith('a'), isNot(contains('\u{202e}')), isNot(contains('\n'))),
        ),
        (['${dir.path}/${'x' * 200}'], allOf(endsWith('any more.'), hasLength(lessThan(100)))),
        (['${dir.path}/${'x' * 2000}'], startsWith('xxx')),
      ]) {
        final d = await whatWasDropped(paths);
        expect(d.folder, isNull, reason: '$paths');
        expect(d.words, words, reason: '$paths');
      }
      expect(f.readAsStringSync(), 'x');
    });

    test('a link is followed, and that is said', () async {
      final real = Directory('${dir.path}/real')..createSync();
      final f = File('${dir.path}/f')..writeAsStringSync('');
      Link('${dir.path}/to').createSync(real.path);
      Link('${dir.path}/lost').createSync('${dir.path}/nowhere');
      Link('${dir.path}/file').createSync(f.path);
      Link('${dir.path}/round').createSync('${dir.path}/round');
      expect(await whatWasDropped(['${dir.path}/to']), (
        folder: real.path,
        words: 'to is a link. The folder is ${real.path}.',
      ));
      for (final (name, words) in [
        ('lost', 'lost is a link that leads nowhere.'),
        ('round', 'round is a link that leads nowhere.'),
        ('file', 'file is a file. A session works in a folder.'),
      ]) {
        expect(await whatWasDropped(['${dir.path}/$name']), (folder: null, words: words));
      }
      // Through a link that was not itself dropped: the real name, and no words of it.
      Directory('${real.path}/in').createSync();
      expect(await whatWasDropped(['${dir.path}/to/in']), (folder: '${real.path}/in', words: null));
    }, skip: Platform.isWindows ? 'links need rights there' : null);

    test('what is neither is no folder', () async {
      final p = await Process.run('mkfifo', ['${dir.path}/pipe']);
      expect(p.exitCode, 0);
      expect((await whatWasDropped(['${dir.path}/pipe'])).words, 'pipe is not a folder.');
      // Nor can all be looked at.
      Directory('${dir.path}/shut/in').createSync(recursive: true);
      await Process.run('chmod', ['000', '${dir.path}/shut']);
      final d = await whatWasDropped(['${dir.path}/shut/in']);
      await Process.run('chmod', ['700', '${dir.path}/shut']);
      expect(d.folder, isNull);
      expect(d.words, isNotEmpty);
    }, skip: Platform.isWindows ? 'there are none' : null);
  });

  testWidgets('a folder dropped on the window fills in the page, and begins nothing', (t) async {
    final s = store(), drags = FakeDrags();
    await mount(t, Shell(s, drags: drags), size: const Size(1100, 760));
    Future<void> drop(List<String> paths) async {
      await t.runAsync(() async {
        drags.dropped!(paths);
        // It looks at the disk.
        await Future<void>.delayed(const Duration(milliseconds: 150));
      });
      await t.pump();
      await t.pump(const Duration(milliseconds: 300));
    }

    expect(find.byType(StartPage), findsNothing);
    final web = Directory('${dir.path}/web-store')..createSync();
    await drop([web.path]);
    expect(find.byType(StartPage), findsOneWidget);
    expect(find.bySemanticsLabel('Folder: ${web.path}'), findsOneWidget);
    expect(find.textContaining('link'), findsNothing);
    expect(s.did, isEmpty);
    // What has been written stays; only the folder is another.
    await t.enterText(find.byType(EditableText).first, 'My task');
    await t.pump();
    final docs = Directory('${dir.path}/docs')..createSync();
    await drop([docs.path]);
    expect(find.bySemanticsLabel('Folder: ${docs.path}'), findsOneWidget);
    expect(find.text('My task'), findsOneWidget);
    // What cannot be is said, and changes nothing.
    await drop([docs.path, web.path]);
    expect(find.text('Only one folder can be dropped at a time.'), findsOneWidget);
    expect(find.bySemanticsLabel('Folder: ${docs.path}'), findsOneWidget);
    // Until something can.
    await drop([web.path]);
    expect(find.textContaining('Only one'), findsNothing);
    expect(s.did, isEmpty);
    await t.pumpWidget(const SizedBox());
    expect(drags.dropped, isNull);
    s.dispose();
  });

  testWidgets('what cannot be leaves one where one was, in a narrow window too', (t) async {
    for (final size in [const Size(1100, 760), const Size(380, 260)]) {
      final s = store(), drags = FakeDrags();
      await mount(t, Shell(s, drags: drags), size: size);
      await t.runAsync(() async {
        drags.dropped!(['${dir.path}/gone']);
        await Future<void>.delayed(const Duration(milliseconds: 100));
      });
      await t.pump();
      expect(find.byType(StartPage), findsNothing);
      expect(find.text('gone is not there any more.'), findsOneWidget);
      expect(
        t.widget<Text>(find.text('gone is not there any more.')).style!.color,
        AxColors.dark.faultText,
      );
      await t.pumpWidget(const SizedBox());
      s.dispose();
    }
  });

  testWidgets('that it was a link is said, and is no failure', (t) async {
    final s = store(), drags = FakeDrags();
    await mount(t, Shell(s, drags: drags), size: const Size(1100, 760));
    Directory('${dir.path}/real').createSync();
    Link('${dir.path}/to').createSync('${dir.path}/real');
    await t.runAsync(() async {
      drags.dropped!(['${dir.path}/to']);
      await Future<void>.delayed(const Duration(milliseconds: 150));
    });
    await t.pump();
    expect(find.bySemanticsLabel('Folder: ${dir.path}/real'), findsOneWidget);
    final words = find.textContaining('to is a link. The folder is');
    expect(words, findsOneWidget);
    expect(t.widget<Text>(words).style!.color, AxColors.dark.ink2);
    await t.pumpWidget(const SizedBox());
    s.dispose();
  }, skip: Platform.isWindows);

  testWidgets('of two drops the last counts', (t) async {
    final s = store(), drags = FakeDrags();
    await mount(t, Shell(s, drags: drags), size: const Size(1100, 760));
    final a = Directory('${dir.path}/first')..createSync();
    await t.runAsync(() async {
      drags.dropped!([a.path]);
      drags.dropped!(['${dir.path}/gone']);
      await Future<void>.delayed(const Duration(milliseconds: 150));
    });
    await t.pump();
    expect(find.byType(StartPage), findsNothing);
    expect(find.text('gone is not there any more.'), findsOneWidget);
    await t.pumpWidget(const SizedBox());
    s.dispose();
  });

  testWidgets('it closes the search it comes over, as a terminal\'s folder does', (t) async {
    for (final terminal in [true, false]) {
      final s = store(), drags = FakeDrags();
      await mount(t, Shell(s, drags: drags), size: const Size(1100, 760));
      await t.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await t.sendKeyEvent(LogicalKeyboardKey.keyK);
      await t.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await t.pump();
      expect(find.byType(SearchBox), findsOneWidget);
      if (terminal) {
        s.onFolder!(dir.path);
      } else {
        await t.runAsync(() async {
          drags.dropped!([dir.path]);
          await Future<void>.delayed(const Duration(milliseconds: 150));
        });
      }
      await t.pump();
      expect(find.byType(SearchBox), findsNothing);
      expect(find.byType(StartPage), findsOneWidget);
      await t.pumpWidget(const SizedBox());
      s.dispose();
    }
  });

  testWidgets('while a folder is held over it, the window says where it goes', (t) async {
    for (final (size, name) in [
      (const Size(1100, 760), 'drop'),
      (const Size(380, 260), 'drop-narrow'),
    ]) {
      final s = store(), drags = FakeDrags();
      await mount(t, Shell(s, drags: drags), size: size);
      expect(find.textContaining('New session in'), findsNothing);
      drags.over!('/me/web-store');
      await t.pump();
      await t.pump(const Duration(milliseconds: 300));
      expect(find.text('New session in web-store'), findsOneWidget);
      expect(t.takeException(), isNull);
      await snap(t, name);
      // Over the canvas and not over the list.
      if (size.width > 900) {
        final words = t.getRect(find.text('New session in web-store'));
        expect(words.left, greaterThan(264));
        expect(find.bySemanticsLabel('Search').hitTestable(), findsOneWidget);
      }
      // Another one.
      drags.over!('/me/${'long ' * 80}');
      await t.pump();
      expect(t.takeException(), isNull);
      drags.out!();
      await t.pump();
      await t.pump(const Duration(milliseconds: 300));
      expect(find.textContaining('New session in'), findsNothing);
      // And what is let go is over it no more.
      drags.over!('/me/web-store');
      await t.pump();
      await t.runAsync(() async {
        drags.dropped!(const []);
        await Future<void>.delayed(const Duration(milliseconds: 50));
      });
      await t.pump();
      await t.pump(const Duration(milliseconds: 300));
      expect(find.text('New session in web-store'), findsNothing);
      expect(find.text('That is not a folder on this computer.'), findsOneWidget);
      expect(s.did, isEmpty);
      await t.pumpWidget(const SizedBox());
      s.dispose();
    }
  });

  testWidgets('as the system tells it', (t) async {
    final said = <Object>[];
    final drags = SystemDrags()
      ..listen(over: said.add, out: () => said.add('out'), dropped: said.add);
    Future<void> tell(String what, [Object? args]) =>
        t.binding.defaultBinaryMessenger.handlePlatformMessage(
          'axiom/drop',
          const StandardMethodCodec().encodeMethodCall(MethodCall(what, args)),
          (_) {},
        );
    await tell('over', '/a');
    await tell('out');
    await tell('drop', ['/a', '/b']);
    expect(said, [
      '/a',
      'out',
      ['/a', '/b'],
    ]);
    said.clear();
    // Whatever else comes is nothing.
    await tell('over', 3);
    await tell('over');
    await tell('drop', '/a');
    await tell('drop', ['/a', 3, null]);
    await tell('else');
    expect(said, [
      ['/a'],
    ]);
    said.clear();
    drags.stop();
    await tell('over', '/a');
    expect(said, isEmpty);
  });
}
