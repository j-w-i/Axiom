// File-type glyph chips (the Tauri shell's glyph.css and `glyphFor`): a two-letter mono label on a
// tint of a syntax hue, shared by the explorer's rows and the editor's tabs.

import 'package:flutter/widgets.dart';

import 'theme.dart';
import 'tokens.dart';

enum GlyphKind { rs, md, toml, other }

/// The chip of a file [name]: its label and hue family.
(String, GlyphKind) glyphFor(String name) {
  final dot = name.lastIndexOf('.');
  final ext = dot > 0 ? name.substring(dot + 1).toLowerCase() : '';
  return switch (ext) {
    'rs' => ('rs', GlyphKind.rs),
    'md' => ('md', GlyphKind.md),
    'toml' => ('tm', GlyphKind.toml),
    'lock' => ('lk', GlyphKind.toml),
    _ => (ext.length > 2 ? ext.substring(0, 2) : ext, GlyphKind.other),
  };
}

/// A 15 px chip for the file [name]; decorative (the row or tab names the file).
class FileGlyph extends StatelessWidget {
  const FileGlyph(this.name, {super.key});
  final String name;

  @override
  Widget build(BuildContext context) {
    final g = context.colors.glyphs;
    final (label, kind) = glyphFor(name);
    final (fg, bg) = switch (kind) {
      GlyphKind.rs => (g.rs, g.rsBg),
      GlyphKind.md => (g.md, g.mdBg),
      GlyphKind.toml => (g.toml, g.tomlBg),
      GlyphKind.other => (g.other, g.otherBg),
    };
    return ExcludeSemantics(
      child: Container(
        width: 15,
        height: 15,
        alignment: Alignment.center,
        decoration: BoxDecoration(color: bg, borderRadius: BorderRadius.circular(3)),
        child: Text(
          label,
          style: TextStyle(
            fontFamily: AxiomType.mono,
            fontSize: 8.5,
            height: 1,
            fontWeight: FontWeight.w500,
            letterSpacing: -0.17,
            color: fg,
          ),
        ),
      ),
    );
  }
}
