// Every package the client ships must carry a licence we allow (docs/decisions/0011).
import 'dart:convert';
import 'dart:io';

const allowed = {
  'MIT': ['Permission is hereby granted, free of charge'],
  'BSD': ['Redistribution and use in source and binary forms'],
  'Apache-2.0': ['Apache License', 'Version 2.0'],
};

void main() {
  final cfg = json.decode(
    File('.dart_tool/package_config.json').readAsStringSync(),
  ) as Map<String, dynamic>;
  final bad = <String>[];
  var n = 0;
  for (final p in (cfg['packages'] as List).cast<Map<String, dynamic>>()) {
    final name = p['name'] as String;
    if (name == 'axiom') continue;
    final root = Uri.file('${Directory.current.path}/.dart_tool/').resolve(p['rootUri'] as String);
    // Packages of the Flutter SDK share its licence at the SDK's root.
    var dir = Directory.fromUri(root);
    File? f;
    for (var up = 0; up < 5 && f == null; up++) {
      for (final c in ['LICENSE', 'LICENSE.md', 'LICENSE.txt']) {
        final x = File('${dir.path}/$c');
        if (x.existsSync()) f = x;
      }
      dir = dir.parent;
    }
    final text = f?.readAsStringSync().replaceAll(RegExp(r'\s+'), ' ') ?? '';
    final ok = allowed.values.any((marks) => marks.every(text.contains));
    if (!ok || text.contains('GNU GENERAL PUBLIC')) bad.add('$name: ${f?.path ?? 'no licence'}');
    n++;
  }
  if (bad.isNotEmpty) {
    stderr.writeln('licences not allowed:\n${bad.join('\n')}');
    exit(1);
  }
  stdout.writeln('licences: $n packages ok');
}
