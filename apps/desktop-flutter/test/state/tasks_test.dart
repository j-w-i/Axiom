// The task output's line assembly (FL-CONSOLE-001; the Tauri shell's OutputBuffer rules).

import 'package:axiom_desktop/daemon/protocol.g.dart';
import 'package:axiom_desktop/state/tasks.dart';
import 'package:flutter_test/flutter_test.dart';

List<String> lines(OutputBuffer b) => [
  for (final (text, stream, _) in b.take(1000)) '${stream?.wire ?? '-'}:$text',
];

void main() {
  test('lines per stream, in seq order, with late and duplicate chunks', () {
    final b = OutputBuffer()
      ..append(1, StreamKind.stderr, 'err1\n')
      ..append(0, StreamKind.stdout, 'ou')
      ..append(0, StreamKind.stdout, 'dup')
      ..append(2, StreamKind.stdout, 't1\nout2\n');
    expect(lines(b), ['stderr:err1', 'stdout:out1', 'stdout:out2']);
  });

  test('ANSI and control characters are stripped; CR keeps the last overwrite', () {
    final b = OutputBuffer()
      ..append(0, StreamKind.stdout, '\x1B[32mgreen\x1B[0m\r\n')
      ..append(1, StreamKind.stdout, '10%\r50%\r100%\n')
      ..append(2, StreamKind.stdout, 'bell\x07!\n');
    expect(lines(b), ['stdout:green', 'stdout:100%', 'stdout:bell!']);
  });

  test('the end of the task flushes partial lines', () {
    final b = OutputBuffer()..append(0, StreamKind.stdout, 'no newline');
    expect(lines(b), isEmpty);
    b.finish();
    expect(lines(b), ['stdout:no newline']);
  });

  test('durations read as the Tauri shell wrote them', () {
    expect(formatDuration(850), '850 ms');
    expect(formatDuration(1530), '1.5 s');
    expect(formatDuration(125000), '2 min 5 s');
  });

  test('output with no line breaks is cut into bounded lines, in linear time (AUD-026)', () {
    final b = OutputBuffer();
    final chunk = 'a' * 4096;
    final sw = Stopwatch()..start();
    for (var i = 0; i < 2000; i++) {
      b.append(i, StreamKind.stdout, chunk); // 8 MiB, no newline
    }
    b.finish();
    sw.stop();
    final out = lines(b);
    expect(out, isNotEmpty);
    for (final l in out) {
      expect(l.length, lessThan(OutputBuffer.maxLineLength + 64));
    }
    expect(out.first, contains('line cut at 64 KiB'));
    expect(sw.elapsedMilliseconds, lessThan(5000));
  });
}
