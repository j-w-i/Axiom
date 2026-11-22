import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';

/// The shell's own preferences (layout: splitter sizes, the open destination, panels shown),
/// in `shell-local.json`. Written atomically (temp file + rename), debounced, and flushed on
/// quit. The daemon's settings hold everything else.
class LocalPrefs extends ChangeNotifier {
  LocalPrefs(this.path);

  final String path;
  Map<String, Object?> _values = {};
  Timer? _save;

  static Future<LocalPrefs> load(String dir) async {
    final p = LocalPrefs('$dir${Platform.pathSeparator}shell-local.json');
    // A damaged file falls back to the copy kept by the previous save (AUD-089).
    for (final path in [p.path, '${p.path}.bak']) {
      try {
        final f = File(path);
        if (await f.exists()) {
          final v = jsonDecode(await f.readAsString());
          if (v is Map) {
            p._values = v.cast<String, Object?>();
            break;
          }
        }
      } on Object {
        debugPrint('$path could not be read; trying the backup');
      }
    }
    return p;
  }

  T? get<T>(String key) {
    final v = _values[key];
    return v is T ? v : null;
  }

  double number(String key, double fallback) => (get<num>(key))?.toDouble() ?? fallback;
  bool flag(String key, bool fallback) => get<bool>(key) ?? fallback;
  String string(String key, String fallback) => get<String>(key) ?? fallback;

  void set(String key, Object? value) {
    if (_values[key] == value) return;
    _values[key] = value;
    notifyListeners();
    _save?.cancel();
    _save = Timer(const Duration(milliseconds: 300), flush);
  }

  Future<void>? _writing;
  int _writes = 0;

  /// Writes now (on quit, and after the debounce). Writes run one after another, each to a temp
  /// file of its own, so two flushes cannot interleave into a damaged file (AUD-089).
  Future<void> flush() {
    _save?.cancel();
    _save = null;
    final text = const JsonEncoder.withIndent('  ').convert(_values);
    final before = _writing ?? Future<void>.value();
    final next = before.then((_) => _write(text));
    _writing = next;
    return next;
  }

  Future<void> _write(String text) async {
    try {
      final f = File(path);
      await f.parent.create(recursive: true);
      final tmp = File('$path.$pid.${_writes++}.tmp');
      await tmp.writeAsString(text, flush: true);
      if (await f.exists()) {
        try {
          await f.copy('$path.bak');
        } on Object {
          // The backup is best effort.
        }
      }
      await tmp.rename(path);
    } on Object catch (e) {
      debugPrint('shell-local.json not saved: $e');
    }
  }

  @override
  void dispose() {
    _save?.cancel();
    super.dispose();
  }
}
