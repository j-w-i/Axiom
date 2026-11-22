// Chat's frame timings (ADR-0020 §5, FL-CHAT-001 criterion 3): a long Markdown answer (prose,
// lists, tables, code blocks; about 3000 words) streamed by the scripted model in small chunks,
// then the transcript flung, in a profile build at the owner's scale, against the real daemon.
//   AXIOM_FRAMES_TARGET=chat_frames_test.dart apps/desktop-flutter/scripts/frames.sh

import 'dart:convert';
import 'dart:io';

import 'package:axiom_desktop/chat/chat_page.dart';
import 'package:axiom_desktop/state/agent.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'frame_report.dart';
import 'support.dart';

/// A long answer, in the chunks it streams in.
List<String> longAnswer() {
  final chunks = <String>[];
  for (var s = 1; s <= 12; s++) {
    chunks.add('## Section $s\n\n');
    for (var p = 0; p < 3; p++) {
      final words = List.generate(
        40,
        (i) => ['the', 'daemon', '**owns**', 'every', '`process`', 'and', 'file', 'write'][i % 8],
      );
      for (var i = 0; i < words.length; i += 6) {
        chunks.add('${words.skip(i).take(6).join(' ')} ');
      }
      chunks.add('\n\n');
    }
    chunks.add('- first point of $s\n- second point, with `code`\n- third point\n\n');
    chunks.add('| name | value |\n|---|---|\n| a$s | ${s * 3} |\n| b$s | ${s * 7} |\n\n');
    chunks.add('```rust\n');
    for (var l = 0; l < 8; l++) {
      chunks.add('fn step_${s}_$l(x: i64) -> i64 { x * $l + 0x2A } // line $l\n');
    }
    chunks.add('```\n\n');
  }
  return chunks;
}

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  binding.framePolicy = LiveTestWidgetsFlutterBindingFramePolicy.fullyLive;

  testWidgets('chat frame timings', (tester) async {
    final data = tempDir('axiom-fl-chat-frames-');
    addTearDown(() => data.deleteSync(recursive: true));
    final chunks = longAnswer();
    final script = '${data.path}/script.json';
    File(script).writeAsStringSync(
      jsonEncode({
        'turns': [
          {
            'expect': {'userContains': 'long'},
            'events': [
              for (final c in chunks) ...[
                {'type': 'text', 'text': c},
                {'type': 'delay', 'ms': 12},
              ],
            ],
          },
        ],
      }),
    );
    final app = await boot(
      tester,
      data.path,
      env: {
        'AXIOM_TEST_SCRIPTED_MODEL': script,
        'AXIOM_TEST_CREDENTIALS_FILE': '${data.path}/credentials.json',
      },
    );
    await tester.tap(named('Chat'));
    await until(tester, () => app.agent.status == 'ready', what: 'the providers');
    final view = tester.state<ChatViewState>(find.byType(ChatView));
    await until(tester, () => view.composerFocus.hasFocus, what: 'the composer');
    await Future<void>.delayed(const Duration(seconds: 1));

    final reports = <Report>[];
    reports.add(
      await measureFrames('streaming answer', () async {
        // `flutter drive` has no test text input: the text goes into the composer's controller.
        view.composer.text = 'a long answer please';
        await key(tester, LogicalKeyboardKey.enter);
        await until(
          tester,
          () => app.agent.activeId != null && !app.agent.active.running,
          timeout: const Duration(seconds: 60),
          what: 'the answer',
          detail: () =>
              'active ${app.agent.activeId}, running ${app.agent.active.running}, '
              'streamed ${app.agent.active.streaming.length} chars, '
              'composer "${view.composer.text}", error "${app.agent.sendError}${app.agent.error}"',
        );
      }),
    );
    expect(
      app.agent.items().whereType<AssistantItem>().map((a) => a.text).join(),
      contains('Section 12'),
    );
    final list = find.descendant(of: find.byType(ChatView), matching: find.byType(Scrollable));
    reports.add(
      await measureFrames('transcript fling', () async {
        for (var i = 0; i < 4; i++) {
          await tester.fling(list.first, Offset(0, i.isEven ? 1500 : -1500), 5000);
          await Future<void>.delayed(const Duration(milliseconds: 900));
        }
      }),
    );
    final size = tester.view.physicalSize;
    // ignore: avoid_print
    print(
      'window ${size.width.round()}x${size.height.round()} physical, dpr ${tester.view.devicePixelRatio}, '
      '${chunks.length} chunks',
    );
    for (final r in reports) {
      // ignore: avoid_print
      print(r);
    }
    binding.reportData = {for (final r in reports) r.name: r.toJson()};
    await quit(tester, app);
    if (Platform.environment['AXIOM_FRAMES_ASSERT'] != '0') {
      expect([
        for (final r in reports)
          if (r.over > 0) '${r.name}: ${r.over} of ${r.frames.length}',
      ], isEmpty);
    }
  });
}
