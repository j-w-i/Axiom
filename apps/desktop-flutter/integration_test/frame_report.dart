// Frame timings from the engine (FrameTiming: build and raster per frame), summarised.

import 'dart:ui' show FramePhase;

import 'package:flutter/scheduler.dart';

const budgetMs = 16.7;

final class Report {
  Report(this.name, this.frames);
  final String name;
  final List<FrameTiming> frames;

  double _ms(Duration d) => d.inMicroseconds / 1000;
  List<double> get build => [for (final f in frames) _ms(f.buildDuration)];
  List<double> get raster => [for (final f in frames) _ms(f.rasterDuration)];
  /// The time from one frame's vsync to the next one's: 16.7 ms when it draws at 60 Hz.
  List<double> get gaps => [
    for (var i = 1; i < frames.length; i++)
      (frames[i].timestampInMicroseconds(FramePhase.vsyncStart) -
              frames[i - 1].timestampInMicroseconds(FramePhase.vsyncStart)) /
          1000,
  ];

  /// From vsync to the frame's raster finishing (what the person waits for).
  List<double> get span => [for (final f in frames) _ms(f.totalSpan)];

  int get over => frames
      .where((f) => _ms(f.buildDuration) > budgetMs || _ms(f.rasterDuration) > budgetMs)
      .length;

  double _p(List<double> v, double q) {
    if (v.isEmpty) return 0;
    final s = [...v]..sort();
    return s[((s.length - 1) * q).round()];
  }

  Map<String, Object> toJson() => {
    'frames': frames.length,
    'build_p50_ms': _p(build, .5),
    'build_p99_ms': _p(build, .99),
    'build_max_ms': _p(build, 1),
    'raster_p50_ms': _p(raster, .5),
    'raster_p99_ms': _p(raster, .99),
    'raster_max_ms': _p(raster, 1),
    'gap_p50_ms': _p(gaps, .5),
    'gap_p90_ms': _p(gaps, .9),
    'span_p50_ms': _p(span, .5),
    'span_p99_ms': _p(span, .99),
    'over_budget': over,
  };

  @override
  String toString() {
    final j = toJson();
    String f(String k) => (j[k]! as double).toStringAsFixed(1);
    return '${name.padRight(22)} frames ${'${j['frames']}'.padLeft(4)}  build p50 ${f('build_p50_ms')} p99 ${f('build_p99_ms')} max ${f('build_max_ms')}  '
        'raster p50 ${f('raster_p50_ms')} p99 ${f('raster_p99_ms')} max ${f('raster_max_ms')}  gap p50 ${f('gap_p50_ms')} p90 ${f('gap_p90_ms')}  span p50 ${f('span_p50_ms')} p99 ${f('span_p99_ms')}  over ${j['over_budget']}';
  }
}

/// Collects the frames [body] produces (and the ones reported shortly after).
Future<Report> measureFrames(String name, Future<void> Function() body) async {
  final frames = <FrameTiming>[];
  void cb(List<FrameTiming> t) => frames.addAll(t);
  SchedulerBinding.instance.addTimingsCallback(cb);
  await body();
  await Future<void>.delayed(const Duration(milliseconds: 600));
  SchedulerBinding.instance.removeTimingsCallback(cb);
  return Report(name, frames);
}
