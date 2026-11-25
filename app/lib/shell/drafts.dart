import 'dart:math';

import 'package:flutter/widgets.dart';

/// What a person is writing to one agent, and what went wrong with it. It outlives the part of the
/// window it is written in.
class Draft {
  final text = TextEditingController();
  final problem = ValueNotifier<String?>(null);
  String? _said, _key;

  /// The name that saying [s] has: the same one if it is said again because nobody answered, so that
  /// it is heard once.
  String keyFor(String s) {
    if (s != _said) _key = freshKey();
    _said = s;
    return _key!;
  }

  /// It was heard: the same words again are said again.
  void heard() => _said = null;

  void dispose() {
    text.dispose();
    problem.dispose();
  }
}

/// A name nothing else has.
String freshKey() {
  final r = Random.secure();
  return [for (var i = 0; i < 16; i++) r.nextInt(256).toRadixString(16).padLeft(2, '0')].join();
}

/// The drafts of all agents, as long as the window is open.
class Drafts {
  final _all = <(String, String), Draft>{};

  /// Empty ones are forgotten first.
  static const most = 64, atAll = 256;

  Draft of(String session, String agent) {
    final d = _all.remove((session, agent)) ?? Draft();
    _all[(session, agent)] = d;
    if (_all.length > most) {
      final old = _all.entries.take(_all.length - 1);
      final drop =
          old.where((e) => e.value.text.text.isEmpty).firstOrNull ??
          (_all.length > atAll ? old.first : null);
      if (drop != null) {
        _all.remove(drop.key);
        // It may be on the screen for this frame still.
        WidgetsBinding.instance.addPostFrameCallback((_) => drop.value.dispose());
      }
    }
    return d;
  }

  void dispose() {
    for (final d in _all.values) {
      d.dispose();
    }
  }
}
