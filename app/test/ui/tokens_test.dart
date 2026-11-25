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
      final ground = [('chassis', c.chassis.fill), ('bay', c.bay.fill)];
      final general = [...ground, ('plate', c.plate.fill), ('overlay', c.overlay.fill)];
      final hovered = [for (final (n, s) in general) ('$n hovered', Color.alphaBlend(c.hover, s))];
      final chosen = [
        for (final (n, s) in general) ('chosen on $n', Color.alphaBlend(c.selected, s)),
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
        check('ink1', c.ink1, [...everywhere, ...chosen], 7);
        for (final (n, k) in [
          ('ink2', c.ink2),
          ('ink3', c.ink3),
          ('liveText', c.liveText),
          ('attentionText', c.attentionText),
          ('faultText', c.faultText),
          ('doneText', c.doneText),
        ]) {
          check(n, k, everywhere);
        }
        // The faintest is for the grounds, and not where something is chosen.
        check('ink4', c.ink4, [
          for (final (n, s) in ground) ...[(n, s), ('$n hovered', Color.alphaBlend(c.hover, s))],
        ]);
        // Each recedes from the one before it.
        for (final (_, bg) in ground) {
          final r = [
            for (final k in [c.ink1, c.ink2, c.ink3, c.ink4]) ratio(k, bg),
          ];
          expect(r[0] - r[1], greaterThan(2));
          expect(r[1] - r[2], greaterThan(1));
          expect(r[2], greaterThan(r[3]));
        }
        final tints = [
          for (final h in c.services.entries)
            ('${h.key.name} tint', Color.alphaBlend(c.tinted(h.value), c.overlay.fill)),
        ];
        check('ink1', c.ink1, tints);
        check('ink3', c.ink3, tints);
        for (final (n, k) in [
          ('ink2', c.ink2),
          ('ink3', c.ink3),
          ('attention', c.attentionText),
          ('fault', c.faultText),
        ]) {
          check(n, k, chosen);
        }
        for (final (n, s) in [
          ('key', c.key),
          ('keyHover', c.keyHover),
          ('keyDown', c.keyDown),
          ('slab', c.slab),
          ('track', c.track),
          ('field', c.field),
          ('written', c.written),
        ]) {
          check('ink1', c.ink1, [(n, s.fill)]);
          check('ink3', c.ink3, [(n, s.fill)]);
        }
        check('placeholder', c.ink3, [('field', c.field.fill), ('written', c.written.fill)]);
        check('attention', c.attentionText, [('slab', c.slab.fill)]);
        check('fault', c.faultText, [('slab', c.slab.fill)]);
        for (final s in [c.primary, c.primaryHover, c.primaryDown]) {
          check('onPrimary', c.onPrimary, [('primary', s.fill)]);
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
          final b = c.block.fill;
          check('code $n', k, [
            ('block', b),
            ('bay', c.bay.fill),
            ('tile', c.codeTile),
            ('added', Color.alphaBlend(c.diffAdd, b)),
            ('removed', Color.alphaBlend(c.diffRemove, b)),
          ]);
        }
        expect(bad, isEmpty, reason: bad.join('\n'));
      });

      test('marks that carry meaning can be seen', () {
        final bad = <String>[];
        for (final e in {for (final m in c.services.entries) m.key.name: m.value}.entries) {
          final tint = Color.alphaBlend(c.tinted(e.value), c.overlay.fill);
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

      test('depth is a tone and soft light, and nothing is drawn round', () {
        double l(SurfaceSpec s) => lum(s.fill);
        // Recessed: a tone below the ground, and nothing else.
        for (final s in [c.track, c.block]) {
          expect((s.lip.a, s.border.a, s.shadows.isEmpty), (0, 0, true));
          expect(l(s), lessThan(l(c.bay)));
        }
        // Raised: a tone above what it stands on, in a soft shade.
        for (final s in [c.plate, c.key, c.keyHover, c.written]) {
          expect(s.shadows, isNotEmpty);
        }
        expect(l(c.plate), greaterThan(l(c.bay)));
        expect(l(c.written), greaterThan(l(c.bay)));
        expect(l(c.slab), greaterThan(l(c.track)));
        expect(l(c.canvas), greaterThan(l(c.chassis)));
        // Where an agent is written to has no edge of any kind.
        expect((c.written.lip.a, c.written.border.a), (0, 0));
        // Light on an upper edge is faint; no shade is a line.
        for (final s in [c.plate, c.slab, c.key, c.written, c.overlay, c.canvas, c.primary]) {
          expect(s.lip.a, lessThan(.1));
          for (final x in s.shadows) {
            expect(x.blur, greaterThanOrEqualTo(2));
          }
        }
        // No edge has a hue.
        for (final k in [c.plate.lip, c.plate.border, c.key.border, c.overlay.border]) {
          expect((k.r - k.b).abs(), lessThan(.06));
        }
      });

      // The owner's choices: aqua B (2026-11-25), and keys without a hue (round 12, A).
      test('aqua is only what runs and what has the keyboard; keys are neutral', () {
        for (final k in [c.live, c.liveText, c.focus]) {
          expect(HSLColor.fromColor(k).hue, inInclusiveRange(192, 201));
        }
        for (final k in [
          c.primary.fill,
          c.onPrimary,
          c.key.fill,
          c.chassis.fill,
          c.bay.fill,
          c.plate.fill,
          c.overlay.fill,
          c.codeInline,
          c.ink1,
          c.ink3,
        ]) {
          expect((k.r - k.b).abs(), lessThan(.05), reason: '$k');
        }
        for (final k in [c.codeKeyword, c.codeString, c.codeCall, c.codeType, c.codeNumber]) {
          expect(k, isNot(anyOf(c.live, c.liveText, c.focus)));
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
