// Design tokens: "Precision Instrument × Temporal Tactility" (ADR-0020 §4). Every colour, size and
// duration the shell uses comes from here; widgets never write a literal colour (a test greps).

import 'package:flutter/widgets.dart';

/// The palette of one theme.
@immutable
final class AxiomColors {
  const AxiomColors({
    required this.brightness,
    required this.recessed,
    required this.ground,
    required this.bar,
    required this.raised,
    required this.sheet,
    required this.ink,
    required this.ink2,
    required this.ink3,
    required this.onInk,
    required this.hairline,
    required this.groove,
    required this.litEdge,
    required this.contactShade,
    required this.sheetShadow,
    required this.scrim,
    required this.hover,
    required this.pressed,
    required this.selection,
    required this.accent,
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
  });

  final Brightness brightness;

  // ---- surfaces, by depth (tone steps; no blur, no glass) ----
  /// −1: wells (the console, inputs, the composer's bezel, the explorer's track).
  final Color recessed;

  /// 0: the editor and page backgrounds.
  final Color ground;

  /// Chrome: the title row, rail, side bar, status bar.
  final Color bar;

  /// +1: raised items (the active tab, a selected row, chips). Gets a lit edge.
  final Color raised;

  /// +2: menus, dialogs, the palette, tooltips. Gets the one soft shadow.
  final Color sheet;

  // ---- ink ----
  final Color ink;
  final Color ink2;
  final Color ink3;

  /// Text and icons on an ink fill (the primary circle).
  final Color onInk;

  // ---- edges ----
  /// Between surfaces of the same tone.
  final Color hairline;

  /// The dark groove between rows and between regions.
  final Color groove;

  /// 1 px at the top inside edge of a raised surface.
  final Color litEdge;

  /// 1 px under a raised surface.
  final Color contactShade;

  /// The sheet's soft shadow.
  final Color sheetShadow;

  /// Behind a modal dialog.
  final Color scrim;

  // ---- states ----
  final Color hover;
  final Color pressed;
  final Color selection;

  // ---- meaning (one each) ----
  /// Focus and the running state: Axiom's brass.
  final Color accent;
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

  bool get isDark => brightness == Brightness.dark;

  static const dark = AxiomColors(
    brightness: Brightness.dark,
    recessed: Color(0xFF070809),
    ground: Color(0xFF0E0F11),
    bar: Color(0xFF15171A),
    raised: Color(0xFF1B1D21),
    sheet: Color(0xFF1F2125),
    ink: Color(0xFFF3F4F6),
    ink2: Color(0xFFA3A7AE),
    ink3: Color(0xFF8F939B),
    onInk: Color(0xFF0E0F11),
    hairline: Color(0x17FFFFFF),
    groove: Color(0xFF070809),
    litEdge: Color(0x1AFFFFFF),
    contactShade: Color(0x80000000),
    sheetShadow: Color(0x99000000),
    scrim: Color(0x99000000),
    hover: Color(0x0FFFFFFF),
    pressed: Color(0x1AFFFFFF),
    selection: Color(0x40C9A35B),
    accent: Color(0xFFC9A35B),
    accentText: Color(0xFFD9B672),
    accentSoft: Color(0x21C9A35B),
    focus: Color(0xFFD2AC63),
    onAccent: Color(0xFF1A1307),
    ok: Color(0xFF8FBF7F),
    okText: Color(0xFF9FCA90),
    warn: Color(0xFFE8904F),
    warnText: Color(0xFFEFA673),
    danger: Color(0xFFFF6259),
    dangerText: Color(0xFFFF7A72),
    dangerSoft: Color(0x24FF6259),
  );

  static const light = AxiomColors(
    brightness: Brightness.light,
    recessed: Color(0xFFE5E7EB),
    ground: Color(0xFFF2F3F5),
    bar: Color(0xFFFAFAFB),
    raised: Color(0xFFFFFFFF),
    sheet: Color(0xFFFFFFFF),
    ink: Color(0xFF0D0E10),
    ink2: Color(0xFF5B5F66),
    ink3: Color(0xFF63676E),
    onInk: Color(0xFFFFFFFF),
    hairline: Color(0x1A0D0E10),
    groove: Color(0xFFE1E3E7),
    litEdge: Color(0xBFFFFFFF),
    contactShade: Color(0x140D0E10),
    sheetShadow: Color(0x290D0E10),
    scrim: Color(0x4D0D0E10),
    hover: Color(0x0A0D0E10),
    pressed: Color(0x140D0E10),
    selection: Color(0x40B88A3A),
    accent: Color(0xFFB88A3A),
    accentText: Color(0xFF85601C),
    accentSoft: Color(0x24B88A3A),
    focus: Color(0xFF9A722C),
    onAccent: Color(0xFF1A1307),
    ok: Color(0xFF4F8F45),
    okText: Color(0xFF397330),
    warn: Color(0xFFA44E07),
    warnText: Color(0xFF8F4406),
    danger: Color(0xFFC8322B),
    dangerText: Color(0xFFB02C26),
    dangerSoft: Color(0x1AC8322B),
  );
}

/// Sizes, spacing and shape.
abstract final class Metrics {
  static const s1 = 2.0, s2 = 4.0, s3 = 6.0, s4 = 8.0, s5 = 12.0, s6 = 16.0, s7 = 24.0;
  static const radiusControl = 6.0;
  static const radiusRaised = 10.0;
  static const radiusSheet = 14.0;
  static const rowHeight = 24.0;
  static const controlHeight = 26.0;
  static const tabHeight = 32.0;
  static const titleRowHeight = 38.0;
  static const statusBarHeight = 24.0;
  static const railWidth = 44.0;
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

  TextStyle get body => TextStyle(fontFamily: sans, fontSize: 13, height: 1.4, color: colors.ink);
  TextStyle get small =>
      TextStyle(fontFamily: sans, fontSize: 12, height: 1.35, color: colors.ink2);
  TextStyle get tiny => TextStyle(fontFamily: sans, fontSize: 11, height: 1.3, color: colors.ink2);
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

  /// The page's answer line, the only large type (ADR-0020 §4, answer first).
  TextStyle get answer => TextStyle(
    fontFamily: sans,
    fontSize: 22,
    height: 1.2,
    fontWeight: FontWeight.w600,
    color: colors.ink,
  );
  TextStyle get code => TextStyle(fontFamily: mono, fontSize: 13, height: 1.5, color: colors.ink);
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

/// Durations and springs. Springs overshoot slightly (1.5–4 %) and only answer something the
/// person did; nothing loops while the window is hidden.
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

  /// About 3 % overshoot: tabs' underline, rail selection, splitters snapping home.
  SpringDescription get settle =>
      SpringDescription.withDampingRatio(mass: 1, stiffness: 520, ratio: 0.745);

  /// About 1.5 %: sheets and menus opening.
  SpringDescription get snap =>
      SpringDescription.withDampingRatio(mass: 1, stiffness: 700, ratio: 0.8);

  /// About 4 %: the send button and expanding cards.
  SpringDescription get expressive =>
      SpringDescription.withDampingRatio(mass: 1, stiffness: 420, ratio: 0.715);

  bool get animates => mode != MotionMode.off;
  Curve get curve => const Cubic(0.2, 0.8, 0.2, 1);
}
