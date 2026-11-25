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
/// history never holds up a frame. What has been read but not yet [consumed] is bounded: past
/// [highWater] events the isolate stops reading, and the host's own bound takes over. Answers wait
/// behind those events, so whoever listens to [messages] must keep consuming.
class HostLink {
  HostLink._(this._fromWorker, this._toWorker, this._isolate);

  static const highWater = 8000;
  final ReceivePort _fromWorker;
  final SendPort _toWorker;
  final Isolate _isolate;
  final _waiting = <int, (Completer<Object?>, Timer)>{};
  final _messages = StreamController<Message>();
  final _closed = Completer<String>();

  /// Sessions whose messages belong to a follow that has been replaced or ended, and the id of the
  /// answer that lifts the fence (0: none will).
  final _fences = <String, int>{};
  int _id = 0;

  /// Events, resync and fault notices. Single subscription.
  Stream<Message> get messages => _messages.stream;

  /// Completes with the reason once the connection is gone.
  Future<String> get closed => _closed.future;

  static Future<HostLink> connect({
    required String endpoint,
    required String token,
    String client = 'axiom-app/0.0.1',
    Duration timeout = const Duration(seconds: 5),
  }) async {
    final rp = ReceivePort();
    final iso = await Isolate.spawn(
      _worker,
      (rp.sendPort, endpoint),
      debugName: 'host link',
      onError: rp.sendPort,
      onExit: rp.sendPort,
    );
    final first = Completer<Object?>();
    HostLink? link;
    rp.listen((m) {
      if (!first.isCompleted) {
        first.complete(m);
      } else {
        link?._receive(m);
      }
    });
    final m = await first.future.timeout(timeout, onTimeout: () => 'the host did not answer');
    if (m is! SendPort) {
      rp.close();
      iso.kill(priority: Isolate.immediate);
      throw LinkClosed(m is List ? '${m.first}' : '$m');
    }
    final l = link = HostLink._(rp, m, iso);
    try {
      await l._call('hello', {'token': token, 'client': client, 'protocol': protocol}, timeout);
    } on Object {
      l.close();
      rethrow;
    }
    return l;
  }

  Future<Object?> call(
    String method, [
    Json params = const {},
    Duration within = const Duration(seconds: 30),
  ]) => _call(method, params, within);

  Future<Object?> _call(String method, Json params, Duration timeout, {String? fence}) {
    if (_closed.isCompleted) return Future.error(const LinkClosed('the host is gone'));
    final id = ++_id;
    if (fence != null) _fences[fence] = method == 'session.follow' ? id : 0;
    final c = Completer<Object?>();
    _waiting[id] = (
      c,
      Timer(timeout, () {
        _waiting.remove(id);
        c.completeError(LinkClosed('the host did not answer $method in ${timeout.inSeconds} s'));
      }),
    );
    _toWorker.send(
      Uint8List.fromList(
        utf8.encode('${json.encode({'id': id, 'method': method, 'params': params})}\n'),
      ),
    );
    return c.future;
  }

  /// From the moment of the call until its answer, nothing of an earlier follow is delivered.
  Future<void> follow(String session, int after) => _call(
    'session.follow',
    {'session': session, 'after': after},
    const Duration(seconds: 30),
    fence: session,
  );

  Future<void> unfollow(String session) =>
      _call('session.unfollow', {'session': session}, const Duration(seconds: 30), fence: session);

  /// Tells the link that [events] events have been dealt with.
  void consumed(int events) {
    if (!_closed.isCompleted && events > 0) _toWorker.send(events);
  }

  void _receive(Object? m) {
    switch (m) {
      case final List<Message> batch:
        for (final x in batch) {
          _one(x);
        }
      case final String why:
        _down(why);
      case final List<Object?> error:
        _down('${error.first}');
      case null:
        _down('the connection ended');
    }
  }

  void _one(Message x) {
    // The worker may have sent more before it learned of the end.
    if (_closed.isCompleted) return;
    switch (x) {
      case Answered(:final id, :final result):
        _fences.removeWhere((_, f) => f == id);
        if (_waiting.remove(id) case (final c, final t)?) {
          t.cancel();
          c.complete(result);
        }
      case Failed(:final id):
        // Id 0: the host could not read a line. Nothing can be matched to it.
        if (id == 0) return _down(x.message);
        if (_waiting.remove(id) case (final c, final t)?) {
          t.cancel();
          c.completeError(x);
        }
      case Events(:final session, :final events):
        if (_fences.containsKey(session)) {
          consumed(events.length);
        } else {
          _messages.add(x);
        }
      case Resync(:final session) || Fault(:final session):
        if (!_fences.containsKey(session)) _messages.add(x);
      case WaitingChanged():
        _messages.add(x);
    }
  }

  void _down(String why) {
    if (_closed.isCompleted) return;
    _closed.complete(why);
    for (final (c, t) in _waiting.values) {
      t.cancel();
      c.completeError(LinkClosed(why));
    }
    _waiting.clear();
    unawaited(_messages.close());
    // The worker destroys the socket and ends by itself; the port closes when it has.
    _toWorker.send(null);
    final late = Timer(const Duration(seconds: 2), () => _isolate.kill());
    final gone = ReceivePort();
    _isolate.addOnExitListener(gone.sendPort);
    gone.listen((_) {
      late.cancel();
      gone.close();
      _fromWorker.close();
    });
  }

  void close() => _down('closed');
}

Future<void> _worker((SendPort, String) a) async {
  final (out, endpoint) = a;
  final Socket s;
  try {
    s = await Socket.connect(InternetAddress(endpoint, type: InternetAddressType.unix), 0);
  } on Object catch (e) {
    Isolate.exit(out, '$e');
  }
  final rp = ReceivePort();
  out.send(rp.sendPort);
  final lines = LineSplitterBytes();
  final decode = utf8.decoder.fuse(json.decoder);
  var owed = 0;
  // Ends with the isolate.
  // ignore: cancel_subscriptions
  late final StreamSubscription<Uint8List> sub;
  void end(String why) {
    s.destroy();
    rp.close();
    Isolate.exit(out, why);
  }

  sub = s.listen(
    (chunk) {
      try {
        final batch = <Message>[];
        for (final l in lines.add(chunk)) {
          final m = Message.decode(decode.convert(l)! as Json);
          if (m is Events) owed += m.events.length;
          batch.add(m);
        }
        if (batch.isNotEmpty) out.send(batch);
        if (owed > HostLink.highWater && !sub.isPaused) sub.pause();
      } on Object catch (e) {
        end('$e');
      }
    },
    onError: (Object e) => end('$e'),
    onDone: () => end('the host closed the connection'),
  );
  rp.listen((m) {
    switch (m) {
      case final Uint8List bytes:
        s.add(bytes);
      case final int n:
        owed -= n;
        if (owed < HostLink.highWater ~/ 2 && sub.isPaused) sub.resume();
      case _:
        end('closed');
    }
  });
}
