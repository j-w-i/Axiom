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
    try {
      final f = File(p.path);
      if (await f.exists()) {
        final v = jsonDecode(await f.readAsString());
        if (v is Map) p._values = v.cast<String, Object?>();
      }
    } on Object {
      // A damaged file is replaced on the next save.
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

  /// Writes now (on quit, and after the debounce).
  Future<void> flush() async {
    _save?.cancel();
    _save = null;
    try {
      final f = File(path);
      await f.parent.create(recursive: true);
      final tmp = File('$path.tmp');
      await tmp.writeAsString(const JsonEncoder.withIndent('  ').convert(_values), flush: true);
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
