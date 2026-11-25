import 'dart:convert'; import 'dart:io'; import 'dart:isolate'; import 'dart:typed_data';
int count(Uint8List b, List<int> needle) { // literal needle, memchr-style
  var hits = 0; final n0 = needle[0]; final end = b.length - needle.length;
  for (var i = 0; i <= end; i++) { if (b[i] != n0) continue; var k = 1; while (k < needle.length && b[i + k] == needle[k]) { k++; } if (k == needle.length) { hits++; while (i < b.length && b[i] != 10) { i++; } } }
  return hits; }
Future<void> main(List<String> a) async {
  if (a[0] == 'idle') { print('ready'); await Future<void>.delayed(const Duration(seconds: 3)); return; }
  final sw = Stopwatch()..start(); final files = <String>[];
  // skips .git only: no gitignore handling, which favours Dart here
  void walk(Directory d) { for (final e in d.listSync(followLinks: false)) { if (e is File) { files.add(e.path); } else if (e is Directory && !e.path.endsWith('/.git')) { walk(e); } } }
  walk(Directory(a[1])); final walkMs = sw.elapsedMilliseconds; final needle = utf8.encode(a[2]);
  final parts = [for (var k = 0; k < 8; k++) [for (var i = k; i < files.length; i += 8) files[i]]];
  final r = await Future.wait([for (final p in parts) Isolate.run(() { var h = 0; for (final f in p) { try { final b = File(f).readAsBytesSync(); final m = b.length < 1024 ? b.length : 1024; var bin = false; for (var i = 0; i < m; i++) { if (b[i] == 0) { bin = true; break; } } if (!bin) h += count(b, needle); } catch (_) {} } return h; })]);
  print('dart search: ${files.length} files, ${r.reduce((x, y) => x + y)} hits, ${sw.elapsedMilliseconds} ms (walk $walkMs ms)'); }
