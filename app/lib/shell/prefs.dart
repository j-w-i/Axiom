import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/widgets.dart';

import '../ui/prefs.dart';

/// Where what a person chose about the window is kept. Nothing secret, and nothing an agent has to
/// do with: the host does not read it.
class PrefsFile {
  PrefsFile(this.path);
  final String path;

  /// Larger is not what was written here.
  static const _most = 64 * 1024;

  /// Read at the start. What is not there or cannot be read is as it comes.
  Prefs read() {
    try {
      final f = File(path);
      if (f.lengthSync() > _most) return const Prefs();
      return Prefs.decode(jsonDecode(f.readAsStringSync()));
    } on Object {
      return const Prefs();
    }
  }

  Future<void> _last = Future.value();

  /// One after another, whole or not at all.
  Future<void> write(Prefs p) => _last = _last.then((_) async {
    try {
      await Directory(File(path).parent.path).create(recursive: true);
      final next = File('$path.new');
      await next.writeAsString(jsonEncode(p.encode()), flush: true);
      await next.rename(path);
    } on FileSystemException catch (e) {
      stderr.writeln('axiom: the settings are not kept: ${e.message}');
    }
  });

  /// Kept from now on.
  ValueNotifier<Prefs> open() {
    final v = ValueNotifier(read());
    v.addListener(() => unawaited(write(v.value)));
    return v;
  }
}
