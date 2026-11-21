import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

/// ADR-0020 §1: the shell is built on the widgets layer only.
void main() {
  test('no Material or Cupertino import in lib/', () {
    final offenders = <String>[];
    for (final f in Directory('lib').listSync(recursive: true).whereType<File>()) {
      if (!f.path.endsWith('.dart')) continue;
      final text = f.readAsStringSync();
      if (RegExp(r"import 'package:(flutter/(material|cupertino)|material_ui|cupertino_ui)")
          .hasMatch(text)) {
        offenders.add(f.path);
      }
    }
    expect(offenders, isEmpty);
  });
}
