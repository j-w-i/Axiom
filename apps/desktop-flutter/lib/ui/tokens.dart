// Design tokens: the Tauri shell's design (FL-DS-002; ADR-0013's M1.6 design as the Tauri shell
// shipped it, apps/desktop-tauri/src/styles/tokens.css at the tag `tauri-shell-final`). Every
// colour, size and duration the shell uses comes from here; widgets never write a literal colour
// (a test greps).

import 'package:flutter/widgets.dart';

/// The palette of one theme: the Tauri shell's semantic tokens (tokens.css at `tauri-shell-final`).
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
    required this.raisedTop,
    required this.sheet,
    required this.ink,
    required this.ink2,
    required this.ink3,
    required this.onInk,
    required this.hairline,
    required this.groove,
    required this.lineInput,
    required this.litEdge,
    required this.contactShade,
    required this.wellShadow,
    required this.lift,
    required this.sheetShadow,
    required this.scrim,
    required this.hover,
    required this.pressed,
    required this.selected,
    required this.selection,
    required this.accent,
    required this.accentHi,
    required this.accentLo,
    required this.primaryBorder,
    required this.accentText,
    required this.accentSoft,
    required this.focus,
    required this.onAccent,
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
    required this.thumbTop,
    required this.thumbBottom,
    required this.syntax,
    required this.glyphs,
  });

  final Brightness brightness;

  /// The window behind every region (the seams show it).
  final Color canvas;

  /// −1: wells (fields, the console, the command field, keycap sockets).
  final Color recessed;

  /// 0: the editor and page backgrounds.
  final Color ground;

  /// Chrome: the title row, rail, tab strip, status bar.
  final Color bar;

  /// Flat regions: the side bar, the console, the AI dock.
  final Color panel;

  /// +1: the lower stop of the raised gradient (buttons, keycaps, chips).
  final Color raised;

  /// The upper stop of the raised gradient.
  final Color raisedTop;

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

  /// 1 px at the top inside edge of a raised surface.
  final Color litEdge;

  /// 1 px under a raised surface.
  final Color contactShade;

  /// The inner shadow along a well's top.
  final Color wellShadow;

  /// White, which hovered keys are mixed toward (Tauri's `color-mix(…, white)`).
  final Color lift;

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

  /// Brass: focus, the running state, the primary action, the region in focus.
  final Color accent;

  /// The primary gradient's upper stop.
  final Color accentHi;

  /// The primary gradient's lower stop.
  final Color accentLo;
  final Color primaryBorder;
  final Color accentText;
  final Color accentSoft;
  final Color focus;
  final Color onAccent;
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

  /// The switch thumb's gradient.
  final Color thumbTop;
  final Color thumbBottom;

  /// Code colours (checked against the editor's ground).
  final SyntaxColors syntax;

  /// File-type chips (explorer rows, editor tabs).
  final GlyphColors glyphs;

  bool get isDark => brightness == Brightness.dark;

  static const dark = AxiomColors(
    brightness: Brightness.dark,
    canvas: Color(0xFF0B0C0D),
    recessed: Color(0xFF111213),
    ground: Color(0xFF17181A),
    bar: Color(0xFF141517),
    panel: Color(0xFF1E1F22),
    raised: Color(0xFF28292C),
    raisedTop: Color(0xFF303135),
    sheet: Color(0xFF26272A),
    ink: Color(0xFFE6E3DC),
    ink2: Color(0xFFA3A097),
    ink3: Color(0xFFA19D94),
    onInk: Color(0xFF17181A),
    hairline: Color(0xFF2B2C30),
    groove: Color(0xFF0A0A0B),
    lineInput: Color(0xFF6C6962),
    litEdge: Color(0x16FFFFFF),
    contactShade: Color(0x38000000),
    wellShadow: Color(0x47000000),
    lift: Color(0xFFFFFFFF),
    sheetShadow: Color(0x8C000000),
    scrim: Color(0x80060607),
    hover: Color(0x0BFFFFFF),
    pressed: Color(0x47000000),
    selected: Color(0x13FFFFFF),
    selection: Color(0x33C9A35B),
    accent: Color(0xFFC9A35B),
    accentHi: Color(0xFFDDB96F),
    accentLo: Color(0xFFA8853F),
    primaryBorder: Color(0xFF654F26),
    accentText: Color(0xFFD9B672),
    accentSoft: Color(0x21C9A35B),
    focus: Color(0xFFD2AC63),
    onAccent: Color(0xFF1A1307),
    ok: Color(0xFF8FBF7F),
    okText: Color(0xFF9FCA90),
    warn: Color(0xFFE8904F),
    warnText: Color(0xFFEFA673),
    danger: Color(0xFFE5645D),
    dangerText: Color(0xFFF27C74),
    dangerSoft: Color(0x1FE5645D),
    diffAdd: Color(0x1F8FBF7F),
    diffDel: Color(0x1FE5645D),
    approvalBorder: Color(0x80C9A35B),
    folder: Color(0xFF8E8A82),
    treeGuide: Color(0x0DFFFFFF),
    thumbTop: Color(0xFFE2DED6),
    thumbBottom: Color(0xFFB7B2A8),
    syntax: SyntaxColors(
      text: Color(0xFFDCD8D0),
      comment: Color(0xFF8A867E),
      keyword: Color(0xFFD18E6E),
      type: Color(0xFF8EC2B2),
      string: Color(0xFFA9C47F),
      number: Color(0xFFC4A7DE),
      punct: Color(0xFF9D998F),
      macro: Color(0xFFD9A0C4),
      function: Color(0xFFDDC783),
      variable: Color(0xFF9EC6E4),
      lineNumber: Color(0xFF6C6962),
      lineNumberActive: Color(0xFFA3A097),
      currentLine: Color(0x07FFFFFF),
    ),
    glyphs: GlyphColors(
      rs: Color(0xFFDD9E80),
      rsBg: Color(0x2BD18E6E),
      md: Color(0xFFA9C1DF),
      mdBg: Color(0x299FB8D8),
      toml: Color(0xFF95C6B6),
      tomlBg: Color(0x268EC2B2),
      other: Color(0xFFA3A097),
      otherBg: Color(0x12FFFFFF),
    ),
  );

  static const light = AxiomColors(
    brightness: Brightness.light,
    canvas: Color(0xFFC4C1BB),
    recessed: Color(0xFFFFFFFF),
    ground: Color(0xFFFCFBF9),
    bar: Color(0xFFE6E4DF),
    panel: Color(0xFFEFEDE9),
    raised: Color(0xFFF1EFEB),
    raisedTop: Color(0xFFFFFFFF),
    sheet: Color(0xFFFCFBF9),
    ink: Color(0xFF1D1D1B),
    ink2: Color(0xFF5A5852),
    ink3: Color(0xFF5D5A54),
    onInk: Color(0xFFFCFBF9),
    hairline: Color(0xFFDCD9D3),
    groove: Color(0xFFC3C0BA),
    lineInput: Color(0xFF86827C),
    litEdge: Color(0xE6FFFFFF),
    contactShade: Color(0x38000000),
    wellShadow: Color(0x24000000),
    lift: Color(0xFFFFFFFF),
    sheetShadow: Color(0x2E281E16),
    scrim: Color(0x2E3C372D),
    hover: Color(0x0D281E0A),
    pressed: Color(0x1A281E0A),
    selected: Color(0x14281E0A),
    selection: Color(0x38B88A3A),
    accent: Color(0xFFB88A3A),
    accentHi: Color(0xFFC99A49),
    accentLo: Color(0xFF9A722C),
    primaryBorder: Color(0xFF5C441A),
    accentText: Color(0xFF785616),
    accentSoft: Color(0x24B88A3A),
    focus: Color(0xFF9A722C),
    onAccent: Color(0xFF1D1505),
    ok: Color(0xFF4F8F45),
    okText: Color(0xFF33682B),
    warn: Color(0xFFA44E07),
    warnText: Color(0xFF8F4406),
    danger: Color(0xFFBC342E),
    dangerText: Color(0xFFB02C26),
    dangerSoft: Color(0x1ABC342E),
    diffAdd: Color(0x1F4F8F45),
    diffDel: Color(0x1FBC342E),
    approvalBorder: Color(0x80B88A3A),
    folder: Color(0xFF7A766E),
    treeGuide: Color(0x12000000),
    thumbTop: Color(0xFFE2DED6),
    thumbBottom: Color(0xFFB7B2A8),
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
      rs: Color(0xFFA8502E),
      rsBg: Color(0x1FA8502E),
      md: Color(0xFF3B6A9E),
      mdBg: Color(0x1F3B6A9E),
      toml: Color(0xFF2F7A68),
      tomlBg: Color(0x1F2F7A68),
      other: Color(0xFF5B5852),
      otherBg: Color(0x0F000000),
    ),
  );
}

/// File-type chip colours: a two-letter mono label on a tint of a syntax hue.
@immutable
final class GlyphColors {
  const GlyphColors({
    required this.rs,
    required this.rsBg,
    required this.md,
    required this.mdBg,
    required this.toml,
    required this.tomlBg,
    required this.other,
    required this.otherBg,
  });
  final Color rs, rsBg, md, mdBg, toml, tomlBg, other, otherBg;
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

/// Sizes, spacing and shape (the Tauri shell's tokens.css: a 4 px grid, small radii).
abstract final class Metrics {
  static const s1 = 2.0, s2 = 4.0, s3 = 6.0, s4 = 8.0, s5 = 12.0, s6 = 16.0, s7 = 24.0;
  static const radiusControl = 3.0;
  static const radiusRaised = 4.0;
  static const radiusSheet = 6.0;

  /// Fully rounded ends (badges, the switch's track).
  static const radiusPill = 999.0;

  /// The soft focus halo around a field: the control's radius plus the ring's width.
  static const radiusHalo = radiusControl + focusWidth;
  static const rowHeight = 22.0;
  static const controlHeight = 26.0;
  static const controlHeightSmall = 22.0;
  static const tabHeight = 32.0;
  static const titleRowHeight = 38.0;
  static const headHeight = 30.0;
  static const crumbHeight = 24.0;
  static const statusBarHeight = 24.0;
  static const railWidth = 44.0;

  /// The inset of a side panel's content from its edges: text, fields and buttons sit 12 px in;
  /// a list is inset [sideListInset] and its rows pad their content by [rowInset], which lands
  /// the rows' text on the same 12 px line.
  static const sideGutter = s5;
  static const sideListInset = s2;
  static const rowInset = s4;
  static const focusWidth = 2.0;
  static const iconSize = 16.0;
  static const underlineWidth = 16.0;
  static const underlineHeight = 2.0;
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

  /// A page's heading line (the Tauri welcome's title: 15 px, strong).
  TextStyle get answer => TextStyle(
    fontFamily: sans,
    fontSize: 15,
    height: 20 / 15,
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
