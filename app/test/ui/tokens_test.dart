import 'dart:io';
import 'dart:math';

import 'package:axiom/ui/tokens.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

double lum(Color c) {
  double f(double v) => v <= .04045 ? v / 12.92 : pow((v + .055) / 1.055, 2.4).toDouble();
  return .2126 * f(c.r) + .7152 * f(c.g) + .0722 * f(c.b);
}

double ratio(Color fg, Color bg) {
  final f = Color.alphaBlend(fg, bg);
  final (a, b) = (lum(f), lum(bg));
  return (max(a, b) + .05) / (min(a, b) + .05);
}

void main() {
  for (final (name, c) in [('dark', AxColors.dark), ('light', AxColors.light)]) {
    group(name, () {
      // Where each colour may be used as text. A hovered surface is included.
      List<(String, Color)> stops(String n, SurfaceSpec s) => [
        ('$n top', s.top),
        ('$n bottom', s.bottom),
      ];
      final general = [
        ...stops('chassis', c.chassis),
        ...stops('plate', c.plate),
        ...stops('plateHover', c.plateHover),
        ...stops('plateSelected', c.plateSelected),
        ...stops('bay', c.bay),
        ...stops('overlay', c.overlay),
        ('row', c.rowSelectedTop),
        ('row', c.rowSelectedBottom),
      ];
      // A selected row and a selected plate take no hover tint.
      final hovered = [
        for (final (n, s) in general)
          if (n != 'row' && !n.startsWith('plateSelected'))
            ('$n hovered', Color.alphaBlend(c.hover, s)),
      ];

      final chosen = [
        ('row', c.rowSelectedTop),
        ('row', c.rowSelectedBottom),
        for (final (n, s) in stops('bay', c.bay)) ('tab on $n', Color.alphaBlend(c.tabSelected, s)),
      ];

      test('text reads on every surface it may sit on', () {
        final bad = <String>[];
        void check(String n, Color fg, Iterable<(String, Color)> on, [double need = 4.5]) {
          for (final (s, bg) in on) {
            final r = ratio(fg, bg);
            if (r < need) bad.add('$n on $s: ${r.toStringAsFixed(2)} < $need');
          }
        }

        final everywhere = [...general, ...hovered];
        check('ink1', c.ink1, everywhere, 7);
        for (final (n, k) in [
          ('ink2', c.ink2),
          ('ink3', c.ink3),
          ('ink4', c.ink4),
          ('liveText', c.liveText),
          ('attentionText', c.attentionText),
          ('faultText', c.faultText),
          ('doneText', c.doneText),
        ]) {
          check(n, k, everywhere);
        }
        final tints = [
          for (final h in c.services.entries)
            ('${h.key.name} tint', Color.alphaBlend(c.tinted(h.value), c.overlay.top)),
        ];
        check('ink1', c.ink1, tints);
        check('ink3', c.ink3, tints);
        for (final (n, k) in [
          ('ink1', c.ink1),
          ('ink2', c.ink2),
          ('ink3', c.ink3),
          ('attention', c.attentionText),
        ]) {
          check(n, k, chosen);
        }
        check('voice', c.voice, stops('bay', c.bay));
        check('ink1', c.ink1, [...stops('key', c.key), ...stops('keyHover', c.keyHover)]);
        check('ink1', c.ink1, stops('keyDown', c.keyDown));
        check('ink2', c.ink2, stops('key', c.key));
        check('ink4 placeholder', c.ink4, stops('bay', c.bay));
        for (final s in [c.primary, c.primaryHover, c.primaryDown]) {
          check('onPrimary', c.onPrimary, stops('primary', s));
        }
        for (final (n, k) in [('ink1', c.ink1), ('ink3', c.ink3), ('attention', c.attentionText)]) {
          check(n, k, stops('plate (a request)', c.plate));
        }
        for (final (n, k) in [
          ('plain', c.codePlain),
          ('keyword', c.codeKeyword),
          ('string', c.codeString),
          ('call', c.codeCall),
          ('type', c.codeType),
          ('number', c.codeNumber),
          ('comment', c.codeComment),
          ('inline', c.codeInline),
        ]) {
          final b = c.block.bottom;
          check('code $n', k, [
            ('block', b),
            ('bay', c.bay.bottom),
            ('added', Color.alphaBlend(c.diffAdd, b)),
            ('removed', Color.alphaBlend(c.diffRemove, b)),
          ]);
        }
        expect(bad, isEmpty, reason: bad.join('\n'));
      });

      test('marks that carry meaning can be seen', () {
        final bad = <String>[];
        for (final e in {
          for (final m in c.services.entries) m.key.name: m.value,
          'read': c.kindRead,
          'edit': c.kindEdit,
          'search': c.kindSearch,
          'run': c.kindRun,
          'web': c.kindWeb,
          'think': c.kindThink,
        }.entries) {
          final tint = Color.alphaBlend(c.tinted(e.value), c.overlay.top);
          final track = [
            for (final (s, bg) in general) ('track on $s', Color.alphaBlend(c.meterTrack, bg)),
          ];
          for (final (s, bg) in [...general, ...track, ('its tint', tint)]) {
            final r = ratio(e.value, bg);
            if (r < 3) bad.add('${e.key} on $s: ${r.toStringAsFixed(2)}');
          }
        }
        for (final (n, k) in [
          ('live', c.live),
          ('attention', c.attention),
          ('fault', c.fault),
          ('done', c.done),
          ('idle', c.idle),
          ('focus', c.focus),
        ]) {
          for (final (s, bg) in [...general, ...chosen]) {
            final r = ratio(k, bg);
            if (r < 3) bad.add('$n on $s: ${r.toStringAsFixed(2)}');
          }
        }
        expect(bad, isEmpty, reason: bad.join('\n'));
      });

      test('gradients stay within a few percent', () {
        for (final s in [c.chassis, c.plate, c.key, c.bay, c.overlay, c.primary]) {
          double l(Color k) => HSLColor.fromColor(k).lightness;
          expect((l(s.top) - l(s.bottom)).abs(), lessThan(.045));
        }
      });

      // The owner's choice of three (B, 2026-11-25), after cobalt, violet and a truer blue.
      test('the accent is the aqua that was chosen', () {
        for (final k in [c.live, c.primary.top, c.primary.bottom]) {
          final h = HSLColor.fromColor(k);
          expect(h.hue, inInclusiveRange(192, 201));
          expect(h.lightness, lessThan(.67));
        }
      });
    });
  }

  final sources = Directory('lib')
      .listSync(recursive: true)
      .whereType<File>()
      .where((f) => f.path.endsWith('.dart'))
      .toList();

  test('no colour, duration or spring is named outside tokens.dart', () {
    final bad = <String>[];
    final re = RegExp(
      r'Color\(0x(?!00000000)|Color\.from|(?<!Ax)Colors\.|SpringDescription\(|CupertinoColors',
    );
    for (final f in sources) {
      if (f.path.endsWith('tokens.dart')) continue;
      for (final (i, l) in f.readAsLinesSync().indexed) {
        if (re.hasMatch(l)) bad.add('${f.path}:${i + 1}: ${l.trim()}');
      }
    }
    expect(bad, isEmpty, reason: bad.join('\n'));
  });

  test('no Material, no Cupertino', () {
    for (final f in sources) {
      expect(
        f.readAsStringSync().contains(
          RegExp("import 'package:(flutter/(material|cupertino)|material_ui|cupertino_ui)"),
        ),
        isFalse,
        reason: f.path,
      );
    }
  });

  test('the product does not import the gallery or its specimen', () {
    for (final f in sources.where((f) => !f.path.contains('gallery'))) {
      expect(f.readAsStringSync().contains('gallery/'), isFalse, reason: f.path);
    }
  });

  test('the client opens no network connection and starts only the host (SI-14)', () {
    final net = RegExp(
      r'HttpClient|RawSocket|WebSocket|NetworkImage|Image\.network|ServerSocket|Process\.run',
    );
    for (final f in sources) {
      expect(net.hasMatch(f.readAsStringSync()), isFalse, reason: f.path);
    }
    final spawns = sources.where((f) => f.readAsStringSync().contains('Process.start'));
    expect(spawns.map((f) => f.uri.pathSegments.last), ['host_store.dart']);
    final sockets = sources.where((f) => f.readAsStringSync().contains('Socket.connect'));
    expect(sockets.map((f) => f.uri.pathSegments.last), ['link.dart']);
    expect(sockets.single.readAsStringSync(), contains('InternetAddressType.unix'));
  });

  test('no surface has a strip of colour along one side', () {
    final one = RegExp(r'Border\(\s*(left|right|top)\s*:|BorderDirectional');
    for (final f in Directory('lib').listSync(recursive: true).whereType<File>()) {
      if (!f.path.endsWith('.dart')) continue;
      final hits = one.allMatches(f.readAsStringSync()).length;
      // A quotation's rule, in the ink of its words, is typography.
      expect(hits, f.path.endsWith('transcript_view.dart') ? 1 : 0, reason: f.path);
    }
  });
}
