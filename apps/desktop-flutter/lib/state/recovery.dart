// Unsaved text kept on disk while it is unsaved (AUD-001; the owed "recovery after a crash" of
// verification/FL.md): a dirty document's text is written to the shell's directory a moment
// after each change and removed when it is saved, closed or discarded. When the folder opens
// again (after a crash of the shell or the daemon) the text comes back as unsaved changes.
//
// Layout: <shellDir>/recovery/<hash of the root>/<hash of the path>.json ({"root", "path",
// "savedVersion"}) beside <hash of the path>.txt (the text as it is, not JSON-encoded: a large
// document is written without building a second copy of it). Each written to a temporary file
// and renamed, the text first.

import 'dart:async';
import 'dart:convert';
import 'dart:io';

final class RecoveredText {
  const RecoveredText({required this.path, required this.text, required this.savedVersion});
  final String path;
  final String text;
  final String savedVersion;
}

/// FNV-1a (64 bit) as hex: a stable file name for a root or a path.
String fnv64(String s) {
  var h = 0xcbf29ce484222325;
  for (final b in utf8.encode(s)) {
    h ^= b;
    h *= 0x100000001b3; // wraps at 64 bits on the VM
  }
  String half(int v) => (v & 0xFFFFFFFF).toRadixString(16).padLeft(8, '0');
  return '${half(h >> 32)}${half(h)}';
}

class RecoveryStore {
  RecoveryStore(this.dir, {this.delay = const Duration(milliseconds: 800)});

  /// `<shellDir>/recovery`; null keeps nothing (tests that do not need it).
  final String? dir;
  final Duration delay;
  final _timers = <String, Timer>{};
  final _pending = <String, ({String root, String path, String Function() text, String version})>{};

  String _file(String root, String path) => '$dir/${fnv64(root)}/${fnv64(path)}.json';
  String _key(String root, String path) => '$root\u0000$path';
  String _textFile(String json) => '${json.substring(0, json.length - 5)}.txt';

  /// Schedules [path]'s unsaved text to be kept (the text is read when the timer fires, so a
  /// burst of typing writes once).
  void keep(String root, String path, String Function() text, String savedVersion) {
    if (dir == null || root.isEmpty) return;
    final k = _key(root, path);
    _pending[k] = (root: root, path: path, text: text, version: savedVersion);
    _timers[k]?.cancel();
    _timers[k] = Timer(delay, () => unawaited(_write(k)));
  }

  Future<void> _write(String k) async {
    _timers.remove(k);
    final p = _pending.remove(k);
    if (p == null) return;
    try {
      final f = File(_file(p.root, p.path));
      await f.parent.create(recursive: true);
      final textFile = File(_textFile(f.path));
      final tmpText = File('${textFile.path}.tmp');
      await tmpText.writeAsString(p.text(), flush: true);
      await tmpText.rename(textFile.path);
      final tmp = File('${f.path}.tmp');
      await tmp.writeAsString(
        jsonEncode({'root': p.root, 'path': p.path, 'savedVersion': p.version}),
        flush: true,
      );
      await tmp.rename(f.path);
    } on Object {
      // Best effort: recovery must never stop editing.
    }
  }

  /// Writes every scheduled text now (before quitting).
  Future<void> flush() async {
    for (final k in [..._pending.keys]) {
      _timers.remove(k)?.cancel();
      await _write(k);
    }
  }

  /// [path] is saved, closed or discarded: nothing to recover.
  void forget(String root, String path) {
    if (dir == null || root.isEmpty) return;
    final k = _key(root, path);
    _timers.remove(k)?.cancel();
    _pending.remove(k);
    try {
      final f = File(_file(root, path));
      if (f.existsSync()) f.deleteSync();
      final t = File(_textFile(f.path));
      if (t.existsSync()) t.deleteSync();
    } on Object {
      // Ignore.
    }
  }

  /// The texts kept for [root].
  List<RecoveredText> entries(String root) {
    if (dir == null || root.isEmpty) return const [];
    final d = Directory('$dir/${fnv64(root)}');
    if (!d.existsSync()) return const [];
    final out = <RecoveredText>[];
    for (final e in d.listSync()) {
      if (e is! File || !e.path.endsWith('.json')) continue;
      try {
        final j = jsonDecode(e.readAsStringSync()) as Map<String, Object?>;
        if (j['root'] != root) continue;
        final t = File(_textFile(e.path));
        if (!t.existsSync()) continue;
        out.add(
          RecoveredText(
            path: j['path']! as String,
            text: t.readAsStringSync(),
            savedVersion: j['savedVersion'] as String? ?? '',
          ),
        );
      } on Object {
        // A damaged entry is skipped.
      }
    }
    return out;
  }

  void dispose() {
    for (final t in _timers.values) {
      t.cancel();
    }
    _timers.clear();
  }
}
