import 'dart:async';
import 'dart:convert';
import 'dart:io';

/// Listens on a Unix socket, as a host does. On Windows behind a relay: see
/// engine/crates/host/examples/relay.rs, which is built with the host.
class UnixServer {
  UnixServer._(this._server, this._relay);
  final ServerSocket _server;
  final Process? _relay;

  static final _relayPath = File(
    '../engine/target/debug/examples/relay${Platform.isWindows ? '.exe' : ''}',
  ).absolute.path;

  /// [relayed] elsewhere too: so that the relay itself is tested where it is not needed.
  static Future<UnixServer> bind(String path, {bool? relayed}) async {
    if (!(relayed ?? Platform.isWindows)) {
      final s = await ServerSocket.bind(InternetAddress(path, type: InternetAddressType.unix), 0);
      return UnixServer._(s, null);
    }
    final s = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
    final p = await Process.start(_relayPath, [path, '${s.port}']);
    unawaited(p.stderr.drain<void>());
    final said = p.stdout
        .transform(utf8.decoder)
        .transform(const LineSplitter())
        .asBroadcastStream();
    if (await said.first != 'ready') throw StateError('the relay did not start');
    unawaited(said.drain<void>().catchError((Object _) {}));
    return UnixServer._(s, p);
  }

  StreamSubscription<Socket> listen(void Function(Socket) f) => _server.listen(f);

  Future<void> close() async {
    await _server.close();
    if (_relay case final p?) {
      p.kill();
      await p.exitCode;
    }
  }
}
