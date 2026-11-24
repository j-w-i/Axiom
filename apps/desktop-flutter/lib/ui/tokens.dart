// Design tokens (UI-001, ADR-0024: graphite and brass in three clear surface steps, the editor the
// lightest, flat controls, one job per hue). Every colour, size and duration the shell uses comes
// from here; widgets never write a literal colour (a test greps).

import 'package:flutter/widgets.dart';

/// The palette of one theme. Surfaces step up towards the work: [bar] (the frame), [panel], then
/// [ground] (the editor), in both themes. Each hue has one job: brass ([accent]) is focus, the
/// primary action and what waits for the person; [info] is change and progress; [agent] is what an
/// agent does; [ok], [warn] and [danger] are outcomes.
@immutable
final class AxiomColors {
  const AxiomColors({
    required this.brightness,
    required this.canvas,
    required this.recessed,
    required this.ground,
    required this.bar,
    required this.panel,
    required this.raised,
    required this.sheet,
    required this.ink,
    required this.ink2,
    required this.ink3,
    required this.onInk,
    required this.hairline,
    required this.groove,
    required this.lineInput,
    required this.litEdge,
    required this.sheetShadow,
    required this.scrim,
    required this.hover,
    required this.pressed,
    required this.selected,
    required this.selection,
    required this.accent,
    required this.accentText,
    required this.accentSoft,
    required this.focus,
    required this.onAccent,
    required this.info,
    required this.infoText,
    required this.infoSoft,
    required this.agent,
    required this.agentText,
    required this.agentSoft,
    required this.ok,
    required this.okText,
    required this.warn,
    required this.warnText,
    required this.danger,
    required this.dangerText,
    required this.dangerSoft,
    required this.diffAdd,
    required this.diffDel,
    required this.approvalBorder,
    required this.folder,
    required this.treeGuide,
    required this.syntax,
    required this.glyphs,
  });

  final Brightness brightness;

  /// The window behind every region (the seams show it).
  final Color canvas;

  /// Wells: fields, code blocks, the terminal.
  final Color recessed;

  /// The work: the editor and page backgrounds (the lightest step).
  final Color ground;

  /// The frame: the title row, rail, tab strip and status bar (the darkest step).
  final Color bar;

  /// The middle step: the side bar, the console, the right panel.
  final Color panel;

  /// A pressable's fill (buttons, chips).
  final Color raised;

  /// +2: menus, dialogs, the palette, tooltips.
  final Color sheet;
  final Color ink;
  final Color ink2;
  final Color ink3;

  /// Text and icons on an ink fill (the primary circle).
  final Color onInk;

  /// Seams inside a region (under a panel head, between status segments).
  final Color hairline;

  /// The seam between regions.
  final Color groove;

  /// A field's border.
  final Color lineInput;

  /// 1 px along the top of the primary action and of a sheet.
  final Color litEdge;

  /// The sheet's soft shadow.
  final Color sheetShadow;

  /// Behind a modal dialog.
  final Color scrim;
  final Color hover;
  final Color pressed;

  /// A selected row.
  final Color selected;

  /// Selected text.
  final Color selection;

  /// Brass: focus, the primary action, and what waits for the person.
  final Color accent;

  final Color accentText;
  final Color accentSoft;
  final Color focus;
  final Color onAccent;

  /// Change and progress: modified files, running tasks, links, hints.
  final Color info;
  final Color infoText;
  final Color infoSoft;

  /// Verdigris: what an agent does or says.
  final Color agent;
  final Color agentText;
  final Color agentSoft;
  final Color ok;
  final Color okText;
  final Color warn;
  final Color warnText;

  /// Errors and destructive actions only.
  final Color danger;
  final Color dangerText;
  final Color dangerSoft;

  /// Behind an added and a removed line in a diff (the ok and danger hues at 12 %); the
  /// contrast test checks the code and diff text colours on them.
  final Color diffAdd;
  final Color diffDel;

  /// The border of an approval card (the accent at 50 %).
  final Color approvalBorder;

  /// Folder icons in the tree.
  final Color folder;

  /// Indent guides in the tree.
  final Color treeGuide;

  /// Code colours (checked against the editor's ground).
  final SyntaxColors syntax;

  /// File glyphs (explorer rows, editor tabs).
  final GlyphColors glyphs;

  bool get isDark => brightness == Brightness.dark;

  static const dark = AxiomColors(
    brightness: Brightness.dark,
    canvas: Color(0xFF121211),
    recessed: Color(0xFF161614),
    ground: Color(0xFF2B2A28),
    bar: Color(0xFF1A1A18),
    panel: Color(0xFF222120),
    raised: Color(0xFF373633),
    sheet: Color(0xFF32312E),
    ink: Color(0xFFEFECE5),
    ink2: Color(0xFFC8C5BD),
    ink3: Color(0xFFB2AFA6),
    onInk: Color(0xFF1B1A18),
    hairline: Color(0xFF393835),
    groove: Color(0xFF131311),
    lineInput: Color(0xFF78756D),
    litEdge: Color(0x38FFFFFF),
    sheetShadow: Color(0x8C000000),
    scrim: Color(0x8C0A0A09),
    hover: Color(0x0EFFFFFF),
    pressed: Color(0x1CFFFFFF),
    selected: Color(0x17FFFFFF),
    selection: Color(0x3DD3AB5E),
    accent: Color(0xFFD3AB5E),
    accentText: Color(0xFFE0BC77),
    accentSoft: Color(0x1FD3AB5E),
    focus: Color(0xFFDDB76C),
    onAccent: Color(0xFF1C1404),
    info: Color(0xFF7FAEE3),
    infoText: Color(0xFF9BC1EE),
    infoSoft: Color(0x1F7FAEE3),
    agent: Color(0xFF6FBFA7),
    agentText: Color(0xFF86D0BA),
    agentSoft: Color(0x1A6FBFA7),
    ok: Color(0xFF93C47D),
    okText: Color(0xFFA6D293),
    warn: Color(0xFFEC9455),
    warnText: Color(0xFFF4AF80),
    danger: Color(0xFFEB6F68),
    dangerText: Color(0xFFFA978F),
    dangerSoft: Color(0x1FEB6F68),
    diffAdd: Color(0x2493C47D),
    diffDel: Color(0x24EB6F68),
    approvalBorder: Color(0x80D3AB5E),
    folder: Color(0xFFB5B1A8),
    treeGuide: Color(0x14FFFFFF),
    syntax: SyntaxColors(
      text: Color(0xFFE6E2DA),
      comment: Color(0xFFA19D94),
      keyword: Color(0xFFDF9C7B),
      type: Color(0xFF8EC2B2),
      string: Color(0xFFA9C47F),
      number: Color(0xFFC4A7DE),
      punct: Color(0xFFB0ACA2),
      macro: Color(0xFFD9A0C4),
      function: Color(0xFFDDC783),
      variable: Color(0xFF9EC6E4),
      lineNumber: Color(0xFF7F7C74),
      lineNumberActive: Color(0xFFC8C5BD),
      currentLine: Color(0x0AFFFFFF),
    ),
    glyphs: GlyphColors(
      orange: Color(0xFFE0A07F),
      blue: Color(0xFF9BC1EE),
      teal: Color(0xFF8CC9B8),
      green: Color(0xFFA6D293),
      violet: Color(0xFFC9ADE3),
      neutral: Color(0xFFAFACA3),
    ),
  );

  static const light = AxiomColors(
    brightness: Brightness.light,
    canvas: Color(0xFFC9C6BF),
    recessed: Color(0xFFFFFFFF),
    ground: Color(0xFFFCFBF8),
    bar: Color(0xFFDEDBD4),
    panel: Color(0xFFECE9E3),
    raised: Color(0xFFE3E0D9),
    sheet: Color(0xFFFBFAF7),
    ink: Color(0xFF1B1A18),
    ink2: Color(0xFF45433E),
    ink3: Color(0xFF5B5852),
    onInk: Color(0xFFFCFBF8),
    hairline: Color(0xFFD9D5CD),
    groove: Color(0xFFCBC7BF),
    lineInput: Color(0xFF86827B),
    litEdge: Color(0x66FFFFFF),
    sheetShadow: Color(0x2E281E16),
    scrim: Color(0x3D3C372D),
    hover: Color(0x0F281E0A),
    pressed: Color(0x1F281E0A),
    selected: Color(0x17281E0A),
    selection: Color(0x42BE8E3A),
    accent: Color(0xFFBE8E3A),
    accentText: Color(0xFF6E4E10),
    accentSoft: Color(0x24BE8E3A),
    focus: Color(0xFF946C27),
    onAccent: Color(0xFF1D1505),
    info: Color(0xFF2F6FB5),
    infoText: Color(0xFF22588F),
    infoSoft: Color(0x1F2F6FB5),
    agent: Color(0xFF1F8068),
    agentText: Color(0xFF14614E),
    agentSoft: Color(0x1A1F8068),
    ok: Color(0xFF4F8F45),
    okText: Color(0xFF2F6128),
    warn: Color(0xFFA44E07),
    warnText: Color(0xFF864004),
    danger: Color(0xFFBC342E),
    dangerText: Color(0xFFA62822),
    dangerSoft: Color(0x1ABC342E),
    diffAdd: Color(0x244F8F45),
    diffDel: Color(0x1FBC342E),
    approvalBorder: Color(0x99946C27),
    folder: Color(0xFF6F6B63),
    treeGuide: Color(0x1A281E0A),
    syntax: SyntaxColors(
      text: Color(0xFF2B2925),
      comment: Color(0xFF6B675F),
      keyword: Color(0xFFA8502E),
      type: Color(0xFF2F7A68),
      string: Color(0xFF557A25),
      number: Color(0xFF7A4EA8),
      punct: Color(0xFF6D6961),
      macro: Color(0xFF9C3F72),
      function: Color(0xFF7D6114),
      variable: Color(0xFF2D6896),
      lineNumber: Color(0xFF8A867E),
      lineNumberActive: Color(0xFF5A5852),
      currentLine: Color(0x093C2D14),
    ),
    glyphs: GlyphColors(
      orange: Color(0xFFA8502E),
      blue: Color(0xFF2F6FB5),
      teal: Color(0xFF2F7A68),
      green: Color(0xFF4F8F45),
      violet: Color(0xFF7A4EA8),
      neutral: Color(0xFF6F6B63),
    ),
  );
}

/// File glyph tints, one per family of files (glyph.dart).
@immutable
final class GlyphColors {
  const GlyphColors({
    required this.orange,
    required this.blue,
    required this.teal,
    required this.green,
    required this.violet,
    required this.neutral,
  });
  final Color orange, blue, teal, green, violet, neutral;
}

/// The editor's code colours.
@immutable
final class SyntaxColors {
  const SyntaxColors({
    required this.text,
    required this.comment,
    required this.keyword,
    required this.type,
    required this.string,
    required this.number,
    required this.punct,
    required this.macro,
    required this.function,
    required this.variable,
    required this.lineNumber,
    required this.lineNumberActive,
    required this.currentLine,
  });
  final Color text, comment, keyword, type, string, number, punct, macro;

  /// Functions and methods (declared and called), and variables, parameters and properties:
  /// the roles VS Code colours apart from the text (2026-11-22).
  final Color function, variable;
  final Color lineNumber, lineNumberActive, currentLine;
}

/// Sizes, spacing and shape (a 4 px grid; controls 5 px, sheets 8 px, rows and regions square).
abstract final class Metrics {
  static const s1 = 2.0, s2 = 4.0, s3 = 6.0, s4 = 8.0, s5 = 12.0, s6 = 16.0, s7 = 24.0;

  /// Marks inside a control (a checkbox, a keycap, a tag).
  static const radiusMark = 3.0;
  static const radiusControl = 5.0;
  static const radiusRaised = 6.0;
  static const radiusSheet = 8.0;

  /// Fully rounded ends (badges, the switch's track).
  static const radiusPill = 999.0;

  /// The soft focus halo around a field: the control's radius plus the ring's width.
  static const radiusHalo = radiusControl + focusWidth;
  static const rowHeight = 24.0;
  static const controlHeight = 28.0;
  static const controlHeightSmall = 24.0;
  static const tabHeight = 36.0;
  static const titleRowHeight = 42.0;
  static const headHeight = 36.0;
  static const statusBarHeight = 26.0;
  static const railWidth = 48.0;

  /// The inset of a side panel's content from its edges: text, fields and buttons sit 12 px in;
  /// a list is inset [sideListInset] and its rows pad their content by [rowInset], which lands
  /// the rows' text on the same 12 px line.
  static const sideGutter = s5;
  static const sideListInset = s2;
  static const rowInset = s4;

  /// The widest a page of reading gets (a conversation), and a page of rows (Environments,
  /// Settings): wider windows add margin, not line length.
  static const readingWidth = 760.0;
  static const pageWidth = 880.0;
  static const focusWidth = 2.0;
  static const iconSize = 16.0;
}

/// Type: IBM Plex Sans and Mono (ADR-0013), no tracking, tabular figures for numbers.
@immutable
final class AxiomType {
  const AxiomType(this.colors);
  final AxiomColors colors;

  static const sans = 'IBM Plex Sans';
  static const mono = 'IBM Plex Mono';
  static const _tabular = [FontFeature.tabularFigures()];

  TextStyle get body =>
      TextStyle(fontFamily: sans, fontSize: 13, height: 18 / 13, color: colors.ink);
  TextStyle get small =>
      TextStyle(fontFamily: sans, fontSize: 12, height: 16 / 12, color: colors.ink2);
  TextStyle get tiny =>
      TextStyle(fontFamily: sans, fontSize: 11, height: 14 / 11, color: colors.ink2);
  TextStyle get label => TextStyle(
    fontFamily: sans,
    fontSize: 12,
    height: 1.3,
    fontWeight: FontWeight.w500,
    color: colors.ink,
  );
  TextStyle get title => TextStyle(
    fontFamily: sans,
    fontSize: 15,
    height: 1.3,
    fontWeight: FontWeight.w600,
    color: colors.ink,
  );

  /// A heading inside a side panel (Source Control's groups, the Inspector's sections, Recent):
  /// one style for every equivalent heading.
  TextStyle get sectionHeading => TextStyle(
    fontFamily: sans,
    fontSize: 12,
    height: 16 / 12,
    fontWeight: FontWeight.w600,
    color: colors.ink2,
  );

  /// A page's heading line: the one large size in the shell.
  TextStyle get answer => TextStyle(
    fontFamily: sans,
    fontSize: 20,
    height: 26 / 20,
    fontWeight: FontWeight.w600,
    color: colors.ink,
  );
  TextStyle get code =>
      TextStyle(fontFamily: mono, fontSize: 13, height: 20 / 13, color: colors.ink);
  TextStyle get codeSmall =>
      TextStyle(fontFamily: mono, fontSize: 12, height: 1.4, color: colors.ink2);

  /// Numbers, times, counts.
  TextStyle get numeric => TextStyle(
    fontFamily: sans,
    fontSize: 12,
    height: 1.3,
    color: colors.ink2,
    fontFeatures: _tabular,
  );
}

/// The person's motion setting (Settings → Appearance), as in ADR-0013.
enum MotionMode { standard, reduced, off }

/// Durations and springs, as the Tauri shell moved: ease-out, no overshoot (springs are
/// critically damped); only in answer to something the person did; nothing loops while the window
/// is hidden.
@immutable
final class AxiomMotion {
  const AxiomMotion(this.mode);
  final MotionMode mode;

  Duration _d(int ms) => switch (mode) {
    MotionMode.standard => Duration(milliseconds: ms),
    MotionMode.reduced => Duration(milliseconds: ms < 80 ? ms : 80),
    MotionMode.off => Duration.zero,
  };

  Duration get press => _d(60);
  Duration get fast => _d(90);
  Duration get medium => _d(140);
  Duration get slow => _d(200);

  /// Hover intent before a tooltip or hint shows.
  Duration get intent => const Duration(milliseconds: 450);

  /// Tabs' lit edge, rail selection, splitters snapping home.
  SpringDescription get settle =>
      SpringDescription.withDampingRatio(mass: 1, stiffness: 520, ratio: 1);

  /// Sheets and menus opening.
  SpringDescription get snap =>
      SpringDescription.withDampingRatio(mass: 1, stiffness: 700, ratio: 1);

  /// The send button and expanding cards.
  SpringDescription get expressive =>
      SpringDescription.withDampingRatio(mass: 1, stiffness: 420, ratio: 1);

  bool get animates => mode != MotionMode.off;

  /// Glides (tab underline, the rail's selection, the switch's thumb, springs): Standard only;
  /// at Reduced they jump (motion-system.md §6).
  bool get glides => mode == MotionMode.standard;

  /// Looping indicators (spinners, pulses): Standard only; Reduced and Off draw them still
  /// (motion-system.md §5.6).
  bool get loops => mode == MotionMode.standard;

  /// Presence (dialogs, menus, toasts) moves or scales only at Standard; Reduced fades.
  bool get travels => mode == MotionMode.standard;

  /// The enter offset of presence (`--travel`): 8 px at Standard, none otherwise.
  double get travel => travels ? 8 : 0;

  /// The press's half-pixel drop (Reduced keeps the pressed look without the movement).
  bool get pressMoves => mode == MotionMode.standard;

  /// The editor's caret blinks only at Standard: Reduced and Off make loops static (ADR-0013).
  bool get caretBlinks => mode == MotionMode.standard;

  /// How long the caret blinks after the last edit, caret move or focus change before it stays
  /// solid (GTK's gtk-cursor-blink-timeout). Each frame re-rasterises the whole window, so after
  /// this an editor left alone draws none (FL-PERF-001).
  Duration get caretBlinkTimeout => const Duration(seconds: 10);

  Curve get curve => const Cubic(0.2, 0.8, 0.2, 1);
}

/// The terminal's 16 ANSI colours (black, red, green, yellow, blue, magenta, cyan, white, then the
/// bright ones): the Tauri shell's tokens.css values, contrast-checked there (M4-TERM-002).
abstract final class AnsiColors {
  static const dark = [
    Color(0xFF2C2D30),
    Color(0xFFE5645D),
    Color(0xFFA9C47F),
    Color(0xFFE0B45C),
    Color(0xFF7FA6D6),
    Color(0xFFC58FB8),
    Color(0xFF8EC2B2),
    Color(0xFFCFCBC3),
    Color(0xFF8A867E),
    Color(0xFFF0857E),
    Color(0xFFBCD894),
    Color(0xFFF0C977),
    Color(0xFF9FC0E8),
    Color(0xFFD9A8CC),
    Color(0xFFA6D6C7),
    Color(0xFFF1EDE6),
  ];
  static const light = [
    Color(0xFF2B2925),
    Color(0xFFB23A33),
    Color(0xFF477113),
    Color(0xFF875E00),
    Color(0xFF2B5D9C),
    Color(0xFF8E3D7C),
    Color(0xFF1C7266),
    Color(0xFFD8D4CC),
    Color(0xFF6A655D),
    Color(0xFFB73A33),
    Color(0xFF477100),
    Color(0xFF8A5D00),
    Color(0xFF3567A6),
    Color(0xFF9B4788),
    Color(0xFF147365),
    Color(0xFFF6F3EE),
  ];
}
