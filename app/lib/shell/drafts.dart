import 'dart:math';

import 'package:flutter/widgets.dart';

/// A place in a conversation that a person goes on from in another one.
class Branching {
  Branching(this.item, {required this.apart, required this.files, this.words}) : withFiles = files;

  /// The item, and, if it is something a person said, those words: they say it differently.
  final String item;
  final String? words;

  /// The branch works in a copy of its own; what the files were there is kept, and it has them.
  final bool apart, files;
  bool withFiles;
  String _before = '';
}

/// What a person is writing to one agent, and what went wrong with it. It outlives the part of the
/// window it is written in.
class Draft {
  final text = TextEditingController();
  final problem = ValueNotifier<String?>(null);

  /// Where what is written goes, if not to the agent itself.
  final branching = ValueNotifier<Branching?>(null);

  /// What is written goes to a new agent that begins as the conversation was there. What had been
  /// written waits.
  void branchAt(Branching b) {
    cancelBranch();
    b._before = text.text;
    branching.value = b;
    if (b.words case final w?) _put(w);
  }

  /// It goes to the agent again.
  void cancelBranch() {
    final b = branching.value;
    if (b == null) return;
    branching.value = null;
    if (b.words != null) _put(b._before);
  }

  void _put(String s) => text.value = TextEditingValue(
    text: s,
    selection: TextSelection.collapsed(offset: s.length),
  );
  String? _said, _key;

  /// The name that saying [s] has: the same one if it is said again because nobody answered, so that
  /// it is heard once.
  String keyFor(String s) {
    if (s != _said) _key = freshKey();
    _said = s;
    return _key!;
  }

  /// Adds to what is being written.
  void add(String more) {
    final was = text.text.trimRight();
    final all = was.isEmpty ? more : '$was\n\n$more';
    text.value = TextEditingValue(
      text: all,
      selection: TextSelection.collapsed(offset: all.length),
    );
  }

  /// It was heard: the same words again are said again.
  void heard() => _said = null;

  void dispose() {
    text.dispose();
    problem.dispose();
    branching.dispose();
  }
}

/// A name like [name] that none of [taken] is.
String nextName(String name, Iterable<String> taken) {
  final m = RegExp(r'^(.*) (\d{1,3})$').firstMatch(name);
  final base = m?.group(1) ?? name;
  final all = taken.toSet();
  for (var n = m == null ? 2 : int.parse(m.group(2)!) + 1; ; n++) {
    final s = '$base $n';
    if (!all.contains(s)) return s;
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
