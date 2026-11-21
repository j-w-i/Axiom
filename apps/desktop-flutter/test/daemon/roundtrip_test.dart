// The generated wire types read and write what the real daemon sends (FL-CORE-001): each
// answer decoded into its type and encoded again equals the JSON on the wire, apart from
// absent-versus-null for optional fields.

import 'dart:io';

import 'package:axiom_desktop/daemon/client.dart';
import 'package:axiom_desktop/daemon/protocol.g.dart';
import 'package:axiom_desktop/daemon/protocol_runtime.dart';
import 'package:flutter_test/flutter_test.dart';

import 'client_test.dart' show daemonPath;

/// JSON with null-valued object members removed (the encoder omits null optionals).
Object? dropNulls(Object? v) {
  if (v is Map) {
    return {
      for (final e in v.entries)
        if (e.value != null) e.key as String: dropNulls(e.value),
    };
  }
  if (v is List) return [for (final x in v) dropNulls(x)];
  return v;
}

void main() {
  test('real answers round-trip through the generated types', () async {
    final data = Directory.systemTemp.createTempSync('axiom-fl-rt-data-');
    final ws = Directory.systemTemp.createTempSync('axiom-fl-rt-ws-');
    addTearDown(() {
      data.deleteSync(recursive: true);
      ws.deleteSync(recursive: true);
    });
    File('${ws.path}/Cargo.toml').writeAsStringSync('[package]\nname = "rt"\nversion = "0.1.0"\n');
    Directory('${ws.path}/src').createSync();
    File('${ws.path}/src/main.rs').writeAsStringSync('fn main() {}\n');
    Process.runSync('git', ['init', '-q', ws.path]);
    final client = DaemonClient(
      DaemonConfig(program: daemonPath(), args: ['--data-dir', data.path]),
    );
    addTearDown(client.shutdown);
    expect(await client.start(), isA<DaemonReady>());

    final checked = <String>[];
    Future<Object?> check(String method, Object? params, Wire Function(Object? json) decode) async {
      final raw = await client.callRaw(method, params);
      expect(dropNulls(decode(raw).toJson()), dropNulls(raw), reason: method);
      checked.add(method);
      return raw;
    }

    final open = await check('workspace/open', {'path': ws.path}, OpenResult.fromJson);
    final id = asMap(open)['workspaceId']! as String;
    await check('fs/list', {'workspaceId': id, 'path': ''}, ListResult.fromJson);
    await check('fs/readFile', {'workspaceId': id, 'path': 'src/main.rs'}, ReadFileResult.fromJson);
    await check('workspace/recent', const <String, Object?>{}, RecentResult.fromJson);
    await check('settings/get', const <String, Object?>{}, GetResult.fromJson);
    await check('git/status', {'workspaceId': id}, GitStatus.fromJson);
    await check('env/detect', {'workspaceId': id}, DetectResult.fromJson);
    await check('build/tasks', {'workspaceId': id}, TasksResult.fromJson);
    await check('workspace/search', {'workspaceId': id, 'query': 'main'}, SearchResult.fromJson);
    await check('agent/models', const <String, Object?>{}, ModelsResult.fromJson);
    await check('lsp/servers', {'workspaceId': id}, ServersResult.fromJson);
    expect(checked, hasLength(11));
  });
}
