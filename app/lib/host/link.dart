import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:isolate';
import 'dart:typed_data';

import 'proto.dart';

/// Splits bytes into lines; mirrors `Lines` in `axiom-proto`.
class LineSplitterBytes {
  static const maxLine = 8 * 1024 * 1024;
  BytesBuilder _rest = BytesBuilder(copy: false);

  Iterable<Uint8List> add(Uint8List chunk) sync* {
    var from = 0;
    for (var i = chunk.indexOf(10); i >= 0; i = chunk.indexOf(10, from)) {
      final part = Uint8List.sublistView(chunk, from, i);
      from = i + 1;
      if (_rest.isEmpty) {
        yield part;
      } else {
        _rest.add(part);
        yield _rest.takeBytes();
      }
    }
    if (from < chunk.length) {
      // Copied: the socket may reuse its buffer.
      _rest.add(Uint8List.fromList(Uint8List.sublistView(chunk, from)));
      if (_rest.length > maxLine) {
        _rest = BytesBuilder(copy: false);
        throw const FormatException('a line is longer than 8 MiB');
      }
    }
  }
}

class LinkClosed implements Exception {
  const LinkClosed(this.why);
  final String why;
  @override
  String toString() => why;
}

/// The connection to the host. Reading and decoding happen on another isolate, so a large page of
/// history never holds up a frame.
class HostLink {
  HostLink._(this._isolate, this._fromWorker, this._toWorker);

  final Isolate _isolate;
  final ReceivePort _fromWorker;
  final SendPort _toWorker;
  final _waiting = <int, Completer<Object?>>{};
  final _messages = StreamController<Message>();
  final _closed = Completer<String>();
  int _id = 0;

  /// Events and resync notices. Single subscription.
  Stream<Message> get messages => _messages.stream;

  /// Completes with the reason once the connection is gone.
  Future<String> get closed => _closed.future;

  static Future<HostLink> connect({
    required String endpoint,
    required String token,
    String client = 'axiom-app/0.0.1',
  }) async {
    final rp = ReceivePort();
    final iso = await Isolate.spawn(_worker, (rp.sendPort, endpoint), debugName: 'host link');
    final first = Completer<Object?>();
    HostLink? link;
    rp.listen((m) {
      if (!first.isCompleted) {
        first.complete(m);
      } else {
        link?._receive(m);
      }
    });
    final m = await first.future;
    if (m is! SendPort) {
      rp.close();
      throw LinkClosed('$m');
    }
    link = HostLink._(iso, rp, m);
    try {
      await link.call('hello', {'token': token, 'client': client, 'protocol': protocol});
    } on Object {
      link.close();
      rethrow;
    }
    return link;
  }

  Future<Object?> call(String method, [Json params = const {}]) {
    if (_closed.isCompleted) return Future.error(const LinkClosed('the host is gone'));
    final id = ++_id;
    final c = _waiting[id] = Completer<Object?>();
    _toWorker.send(
      Uint8List.fromList(
        utf8.encode('${json.encode({'id': id, 'method': method, 'params': params})}\n'),
      ),
    );
    return c.future;
  }

  void _receive(Object? m) {
    switch (m) {
      case final List<Message> batch:
        for (final x in batch) {
          switch (x) {
            case Answered(:final id, :final result):
              _waiting.remove(id)?.complete(result);
            case Failed(:final id):
              // Id 0: the host could not read a line. Nothing can be matched to it.
              if (id == 0) {
                _down(x.message);
              } else {
                _waiting.remove(id)?.completeError(x);
              }
            case _:
              _messages.add(x);
          }
        }
      case final String why:
        _down(why);
    }
  }

  void _down(String why) {
    if (_closed.isCompleted) return;
    _closed.complete(why);
    for (final c in _waiting.values) {
      c.completeError(LinkClosed(why));
    }
    _waiting.clear();
    unawaited(_messages.close());
    _fromWorker.close();
    _isolate.kill(priority: Isolate.immediate);
  }

  void close() => _down('closed');
}

Future<void> _worker((SendPort, String) a) async {
  final (out, endpoint) = a;
  final Socket s;
  try {
    s = await Socket.connect(InternetAddress(endpoint, type: InternetAddressType.unix), 0);
  } on Object catch (e) {
    out.send('$e');
    return;
  }
  final rp = ReceivePort();
  out.send(rp.sendPort);
  rp.listen((m) => s.add(m as Uint8List));
  final lines = LineSplitterBytes();
  final decode = utf8.decoder.fuse(json.decoder);
  try {
    await for (final chunk in s) {
      final batch = <Message>[];
      for (final l in lines.add(chunk)) {
        batch.add(Message.decode(decode.convert(l)! as Json));
      }
      if (batch.isNotEmpty) out.send(batch);
    }
    out.send('the host closed the connection');
  } on Object catch (e) {
    out.send('$e');
  }
  s.destroy();
  rp.close();
}
