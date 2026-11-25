// dart compile exe bench.dart && ./bench.exe <rust target dir>
import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:isolate';
import 'dart:typed_data';

const n = 400000;
late String dir;

class Ev { Ev(this.session, this.seq, this.item, this.text); final Object session; final int seq; final Object item; final String text; }
Ev fromJson(Map<String, dynamic> m) => Ev(m['session'] as String, m['seq'] as int, m['item'] as String, m['text'] as String);

/// Reassembles u32-length frames.
class Framer {
  Uint8List _b = Uint8List(1 << 20); int _r = 0, _w = 0;
  void add(Uint8List c, void Function(Uint8List) f) {
    if (_w + c.length > _b.length) {
      final live = _w - _r;
      final nb = live + c.length > _b.length ? Uint8List((live + c.length) * 2) : _b;
      nb.setRange(0, live, _b, _r); _b = nb; _r = 0; _w = live;
    }
    _b.setRange(_w, _w + c.length, c); _w += c.length;
    final d = ByteData.sublistView(_b);
    while (_w - _r >= 4) {
      final len = d.getUint32(_r, Endian.little);
      if (_w - _r - 4 < len) break;
      f(Uint8List.sublistView(_b, _r + 4, _r + 4 + len)); _r += 4 + len;
    }
  }
}

List<Ev> decodeBin(Uint8List f) {
  final d = ByteData.sublistView(f); final out = <Ev>[]; var p = 0;
  while (p < f.length) {
    final len = d.getUint32(p + 16, Endian.little);
    out.add(Ev(d.getUint32(p, Endian.little), d.getUint64(p + 4, Endian.little), d.getUint32(p + 12, Endian.little), utf8.decode(Uint8List.sublistView(f, p + 20, p + 20 + len))));
    p += 20 + len;
  }
  return out;
}
final _ju = utf8.decoder.fuse(json.decoder);
List<Ev> decodeJson(Uint8List f) => [for (final m in _ju.convert(f) as List) fromJson(m as Map<String, dynamic>)];

Future<(Process, Socket)> start(String fmt, int batch) async {
  final sock = '${Directory.systemTemp.path}/p1-$pid.sock';
  final p = await Process.start('$dir/server', [sock, fmt, '$n', '$batch']);
  await p.stdout.first;
  return (p, await Socket.connect(InternetAddress(sock, type: InternetAddressType.unix), 0));
}

void report(String name, int us, int seen, int busyUs) {
  if (seen != n) throw 'lost events: $seen';
  print('${name.padRight(46)} ${(n / us * 1e6 / 1000).toStringAsFixed(0).padLeft(6)} k ev/s   receiver busy ${(busyUs / n).toStringAsFixed(2)} us/ev  => ${(busyUs / n * 1000 / 1e6 * 100).toStringAsFixed(2)} % of a core at 1k ev/s');
}

Future<void> socketRun(String fmt, int batch) async {
  final (p, s) = await start(fmt, batch);
  final sw = Stopwatch()..start(); final busy = Stopwatch(); var seen = 0; final done = Completer<void>();
  final fr = Framer(); var rest = '';
  s.listen((c) {
    busy.start();
    if (fmt == 'ndjson') {
      final lines = (rest + utf8.decode(c)).split('\n'); rest = lines.removeLast();
      for (final l in lines) { fromJson(json.decode(l) as Map<String, dynamic>); seen++; }
    } else {
      fr.add(c, (f) => seen += (fmt == 'bin' ? decodeBin(f) : decodeJson(f)).length);
    }
    busy.stop();
    if (seen >= n && !done.isCompleted) done.complete();
  });
  await done.future; report('socket $fmt batch=$batch, decode on receiver', sw.elapsedMicroseconds, seen, busy.elapsedMicroseconds);
  s.destroy(); p.kill();
}

/// Socket + decode on a worker isolate; the main isolate gets lists of objects.
Future<void> isolateRun(String fmt, int batch) async {
  final rp = ReceivePort(); final d = dir;
  await Isolate.spawn((SendPort sp) async {
    dir = d;
    final (p, s) = await start(fmt, batch); final fr = Framer(); var seen = 0;
    s.listen((c) {
      final out = <Ev>[];
      fr.add(c, (f) => out.addAll(fmt == 'bin' ? decodeBin(f) : decodeJson(f)));
      if (out.isNotEmpty) sp.send(out);
      seen += out.length; if (seen >= n) { s.destroy(); p.kill(); }
    });
  }, rp.sendPort);
  final sw = Stopwatch()..start(); final busy = Stopwatch(); var seen = 0; final done = Completer<void>();
  rp.listen((m) { busy.start(); for (final _ in m as List<Ev>) { seen++; } busy.stop(); if (seen >= n) done.complete(); });
  await done.future; report('socket $fmt batch=$batch, decode on isolate', sw.elapsedMicroseconds, seen, busy.elapsedMicroseconds); rp.close();
}

Future<void> ffiRun(int batch) async {
  final lib = DynamicLibrary.open('$dir/libp1_ipc.dylib');
  final run = lib.lookupFunction<Void Function(Pointer<Void>, Int64, Uint64, Uint64), void Function(Pointer<Void>, int, int, int)>('p1_run');
  final rp = ReceivePort(); final sw = Stopwatch()..start(); final busy = Stopwatch(); var seen = 0; final done = Completer<void>();
  rp.listen((m) { busy.start(); for (final x in json.decode(m as String) as List) { fromJson(x as Map<String, dynamic>); seen++; } busy.stop(); if (seen >= n) done.complete(); });
  run(NativeApi.postCObject.cast(), rp.sendPort.nativePort, n, batch);
  await done.future; report('FFI port json batch=$batch, decode on receiver', sw.elapsedMicroseconds, seen, busy.elapsedMicroseconds); rp.close();
}

Future<void> latency() async {
  final (p, s) = await start('bin', 64); var got = 0; Completer<void>? c; final fr = Framer(); var warm = Completer<void>();
  s.listen((d) { if (!warm.isCompleted) { fr.add(d, (f) => got += decodeBin(f).length); if (got >= n) warm.complete(); } else { c?.complete(); } });
  await warm.future; final t = <int>[];
  for (var i = 0; i < 5000; i++) { c = Completer(); final w = Stopwatch()..start(); s.add([1]); await c!.future; t.add(w.elapsedMicroseconds); }
  t.sort(); print('socket round trip: p50 ${t[2500]} us  p99 ${t[4950]} us  max ${t.last} us'); s.destroy(); p.kill();
}

Future<void> main(List<String> a) async {
  dir = a[0];
  for (var warm = 0; warm < 1; warm++) { await socketRun('ljson', 32); }
  print('--- (the first line above is warm-up)');
  await socketRun('ndjson', 1);
  for (final b in [1, 32]) { await socketRun('ljson', b); await socketRun('bin', b); }
  await isolateRun('ljson', 32); await isolateRun('bin', 32);
  for (final b in [1, 32]) { await ffiRun(b); }
  await latency(); exit(0);
}
