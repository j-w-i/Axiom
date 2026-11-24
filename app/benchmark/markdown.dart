// Incremental Markdown, headless. Run by scripts/bench.sh.
import 'dart:io';

import 'package:axiom/transcript/markdown.dart';

const para =
    '## Findings\n\nThe expiry check happens **before** the lock is held, so the second tab acts on a '
    'token the first already replaced. I am moving the read inside `refresh()`.\n\n- one entry point\n'
    '- the store keeps its interface\n\n```ts\nconst lock = await this.locks.acquire(id);\n'
    'if (!expired(session.token)) return session;\n```\n\n';

void main() {
  for (final size in [100 * 1024, 1024 * 1024, 10 * 1024 * 1024]) {
    final text = para * (size ~/ para.length);
    for (var warm = 0; warm < 2; warm++) {
      final d = MarkdownDoc();
      final t = <int>[];
      for (var i = 0; i < text.length; i += 4096) {
        final sw = Stopwatch()..start();
        d.append(text.substring(i, i + 4096 > text.length ? text.length : i + 4096));
        d.tail;
        t.add(sw.elapsedMicroseconds);
      }
      if (warm == 0) continue;
      t.sort();
      stdout.writeln(
        '{"bench":"markdown","bytes":$size,"blocks":${d.closed.length},'
        '"chunk":4096,"p50_us":${t[t.length ~/ 2]},"p99_us":${t[t.length * 99 ~/ 100]},"max_us":${t.last}}',
      );
    }
  }
}
