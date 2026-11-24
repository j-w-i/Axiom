// File glyphs (ADR-0024): a small shape for what a file is (code, prose, settings, data, an image,
// a lock), tinted by its family; shared by the explorer's rows and the editor's tabs.

import 'package:flutter/widgets.dart';

import 'icon_data.dart';
import 'theme.dart';

enum GlyphShape { code, prose, settings, data, image, lock, script, page }

enum GlyphHue { orange, blue, teal, green, violet, neutral }

/// The shape and hue of a file [name].
(GlyphShape, GlyphHue) glyphFor(String name) {
  final lower = name.toLowerCase();
  final dot = lower.lastIndexOf('.');
  final ext = dot > 0 ? lower.substring(dot + 1) : '';
  if (ext == 'lock' || lower.endsWith('-lock.json')) return (GlyphShape.lock, GlyphHue.neutral);
  if (lower == 'dockerfile' || lower == 'makefile' || lower == 'justfile') {
    return (GlyphShape.settings, GlyphHue.neutral);
  }
  if (lower.startsWith('license') || lower.startsWith('readme')) {
    return (GlyphShape.prose, GlyphHue.blue);
  }
  return switch (ext) {
    'rs' || 'java' || 'kt' || 'swift' || 'zig' => (GlyphShape.code, GlyphHue.orange),
    'ts' ||
    'tsx' ||
    'py' ||
    'c' ||
    'h' ||
    'cc' ||
    'cpp' ||
    'hpp' ||
    'dart' ||
    'go' ||
    'cs' => (GlyphShape.code, GlyphHue.blue),
    'js' || 'jsx' || 'mjs' || 'cjs' || 'vue' || 'svelte' => (GlyphShape.code, GlyphHue.green),
    'html' ||
    'xml' ||
    'svg' ||
    'rb' ||
    'php' ||
    'lua' ||
    'ex' ||
    'hs' => (GlyphShape.code, GlyphHue.violet),
    'css' || 'scss' || 'less' => (GlyphShape.code, GlyphHue.teal),
    'sh' || 'bash' || 'zsh' || 'fish' || 'ps1' || 'bat' => (GlyphShape.script, GlyphHue.green),
    'md' || 'mdx' || 'rst' || 'txt' || 'adoc' => (GlyphShape.prose, GlyphHue.blue),
    'toml' ||
    'yaml' ||
    'yml' ||
    'ini' ||
    'cfg' ||
    'conf' ||
    'env' => (GlyphShape.settings, GlyphHue.teal),
    'json' || 'jsonc' || 'csv' || 'sql' || 'graphql' || 'proto' => (GlyphShape.data, GlyphHue.teal),
    'png' || 'jpg' || 'jpeg' || 'gif' || 'webp' || 'ico' => (GlyphShape.image, GlyphHue.violet),
    _ =>
      lower.startsWith('.')
          ? (GlyphShape.settings, GlyphHue.neutral)
          : (GlyphShape.page, GlyphHue.neutral),
  };
}

const _icons = {
  GlyphShape.code: IconData16([IconPath('M6 4.5L2.5 8L6 11.5M10 4.5L13.5 8L10 11.5')]),
  GlyphShape.prose: IconData16([IconPath('M3 4.5h10M3 8h10M3 11.5h6')]),
  GlyphShape.settings: IconData16([
    IconPath('M2.5 5h11M2.5 11h11'),
    IconCircle(6, 5, 1.6, fill: true),
    IconCircle(10, 11, 1.6, fill: true),
  ]),
  GlyphShape.data: IconData16([
    IconPath('M6 3C4 3 4.2 4.2 4.2 5.5S4 8 2.5 8C4 8 4.2 9.2 4.2 10.5S4 13 6 13'),
    IconPath('M10 3C12 3 11.8 4.2 11.8 5.5S12 8 13.5 8C12 8 11.8 9.2 11.8 10.5S12 13 10 13'),
  ]),
  GlyphShape.image: IconData16([
    IconRect(2.5, 3.5, 11, 9, rx: 1.5),
    IconPath('M4.5 11L7 8L9 10L10 9L11.5 11z', fill: true),
  ]),
  GlyphShape.lock: IconData16([
    IconRect(4, 7.5, 8, 5.5, rx: 1, fill: true),
    IconPath('M5.5 7.5V5.5a2.5 2.5 0 015 0v2'),
  ]),
  GlyphShape.script: IconData16([IconPath('M3 4.5l3.5 3.5L3 11.5M8.5 11.5H13')]),
  GlyphShape.page: IconData16([IconPath('M4 2.5h5l3 3v8H4z'), IconPath('M9 2.5v3h3')]),
};

/// The glyph of the file [name]; decorative (the row or tab names the file).
class FileGlyph extends StatelessWidget {
  const FileGlyph(this.name, {super.key});
  final String name;

  @override
  Widget build(BuildContext context) {
    final g = context.colors.glyphs;
    final (shape, hue) = glyphFor(name);
    return AxiomIcon(
      _icons[shape]!,
      size: 15,
      color: switch (hue) {
        GlyphHue.orange => g.orange,
        GlyphHue.blue => g.blue,
        GlyphHue.teal => g.teal,
        GlyphHue.green => g.green,
        GlyphHue.violet => g.violet,
        GlyphHue.neutral => g.neutral,
      },
    );
  }
}
