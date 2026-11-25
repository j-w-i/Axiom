import 'dart:io';

import 'package:flutter/services.dart';

import '../transcript/safe_text.dart';

/// What the system tells of what is dragged over the window and dropped on it. Tests put their own in
/// its place.
abstract interface class Drags {
  /// [over]: one folder is held over the window; [out]: no more; [dropped]: what was let go, as paths.
  void listen({
    required void Function(String folder) over,
    required void Function() out,
    required void Function(List<String> paths) dropped,
  });
  void stop();
}

/// As the window's own code in macos, linux and windows tells it.
class SystemDrags implements Drags {
  static const _channel = MethodChannel('axiom/drop');

  @override
  void listen({
    required void Function(String folder) over,
    required void Function() out,
    required void Function(List<String> paths) dropped,
  }) => _channel.setMethodCallHandler((call) async {
    switch (call) {
      case MethodCall(method: 'over', arguments: final String folder):
        over(folder);
      case MethodCall(method: 'out'):
        out();
      case MethodCall(method: 'drop', arguments: final List<Object?> paths):
        dropped([
          for (final p in paths)
            if (p is String) p,
        ]);
    }
  });

  @override
  void stop() => _channel.setMethodCallHandler(null);
}

/// What came of a drop: the folder a session could begin in, or why there is none; with both, what is to
/// be known about the folder.
typedef Dropped = ({String? folder, String? words});

/// Looks at what was dropped, and reads nothing of it.
Future<Dropped> whatWasDropped(List<String> paths) async {
  Dropped no(String words) => (folder: null, words: words);
  if (paths.isEmpty) return no('That is not a folder on this computer.');
  if (paths.length > 1) return no('Only one folder can be dropped at a time.');
  final path = paths.single;
  final tail = path
      .split(RegExp(Platform.isWindows ? r'[/\\]' : '/'))
      .lastWhere((x) => x.isNotEmpty, orElse: () => path);
  final name = oneLine(literal(tail), 60);
  try {
    if (!File(path).isAbsolute) return no('$name is not on this computer.');
    final link = await FileSystemEntity.isLink(path);
    final what = await FileSystemEntity.type(path);
    if (what == FileSystemEntityType.notFound) {
      return no(link ? '$name is a link that leads nowhere.' : '$name is not there any more.');
    }
    if (what == FileSystemEntityType.directory) {
      // As it is really called: that is where the work will be. Windows gives what is on a server a
      // name that cannot be used, so there only where it is needed.
      final real = link || !Platform.isWindows
          ? await Directory(path).resolveSymbolicLinks()
          : path;
      return (
        folder: real,
        words: link ? '$name is a link. The folder is ${oneLine(literal(real), 200)}.' : null,
      );
    }
    return no(
      what == FileSystemEntityType.file
          ? '$name is a file. A session works in a folder.'
          : '$name is not a folder.',
    );
  } on FileSystemException catch (e) {
    final why = e.osError?.message ?? e.message;
    return no('$name could not be looked at: ${oneLine(literal(why), 120)}');
  }
}
