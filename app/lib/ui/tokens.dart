/// Every visual constant in Axiom. Nothing outside this file names a colour, a duration or a spring
/// (test/ui/tokens_test.dart enforces it and checks contrast in both themes).
///
/// The language: graphite structure, navy displays, cobalt only for what is live. Depth comes from
/// one light above: raised parts carry a faint lit top edge, displays sit slightly below the surface.
library;

import 'package:flutter/widgets.dart';

/// How one kind of surface is built. A surface is a role, not a colour (docs/design/system.md).
@immutable
class SurfaceSpec {
  const SurfaceSpec({
    required this.top,
    required this.bottom,
    this.border = const Color(0x00000000),
    this.edge = const Color(0x00000000),
    this.inset = const Color(0x00000000),
    this.lip = const Color(0x00000000),
    this.shadows = const [],
  });

  /// Vertical gradient stops. They differ by a few percent at most.
  final Color top, bottom;

  /// The hairline that cuts the part out of what is under it.
  final Color border;

  /// The lit top edge of a raised part.
  final Color edge;

  /// The shade cast into a recessed part by its upper rim.
  final Color inset;

  /// The light caught by the lower rim of a recessed part.
  final Color lip;
  final List<BoxShadow> shadows;

  SurfaceSpec lerp(SurfaceSpec o, double t) => SurfaceSpec(
    top: Color.lerp(top, o.top, t)!,
    bottom: Color.lerp(bottom, o.bottom, t)!,
    border: Color.lerp(border, o.border, t)!,
    edge: Color.lerp(edge, o.edge, t)!,
    inset: Color.lerp(inset, o.inset, t)!,
    lip: Color.lerp(lip, o.lip, t)!,
    shadows: BoxShadow.lerpList(shadows, o.shadows, t)!,
  );
}

@immutable
class AxColors {
  const AxColors({
    required this.brightness,
    required this.ink1,
    required this.ink2,
    required this.ink3,
    required this.ink4,
    required this.voice,
    required this.onPrimary,
    required this.onAttention,
    required this.live,
    required this.liveText,
    required this.attention,
    required this.attentionText,
    required this.fault,
    required this.faultText,
    required this.done,
    required this.doneText,
    required this.idle,
    required this.focus,
    required this.selection,
    required this.rule,
    required this.ruleLight,
    required this.bayRule,
    required this.hover,
    required this.meterTrack,
    required this.meterLow,
    required this.chassis,
    required this.plate,
    required this.plateHover,
    required this.plateSelected,
    required this.bay,
    required this.block,
    required this.well,
    required this.key,
    required this.keyHover,
    required this.keyDown,
    required this.primary,
    required this.primaryHover,
    required this.primaryDown,
    required this.request,
    required this.answer,
    required this.overlay,
    required this.diffAdd,
    required this.diffRemove,
    required this.codePlain,
    required this.codeKeyword,
    required this.codeString,
    required this.codeCall,
    required this.codeType,
    required this.codeNumber,
    required this.codeComment,
    required this.clipWrite,
    required this.clipRead,
    required this.clipThink,
    required this.clipFault,
    required this.clipAsk,
    required this.scrim,
  });

  final Brightness brightness;

  /// Text, strongest to faintest. All four pass 4.5:1 on every surface they are allowed on.
  final Color ink1, ink2, ink3, ink4;

  /// The person's and other agents' words in a transcript.
  final Color voice;
  final Color onPrimary, onAttention;

  /// States. The plain colour is for marks (3:1), `…Text` for words (4.5:1).
  final Color live, liveText, attention, attentionText, fault, faultText, done, doneText, idle;
  final Color focus, selection, rule, ruleLight, bayRule, hover, meterTrack, meterLow;
  final SurfaceSpec chassis, plate, plateHover, plateSelected, bay, block, well;
  final SurfaceSpec key, keyHover, keyDown, primary, primaryHover, primaryDown;
  final SurfaceSpec request, answer, overlay;
  final Color diffAdd, diffRemove;
  final Color codePlain, codeKeyword, codeString, codeCall, codeType, codeNumber, codeComment;
  final Color clipWrite, clipRead, clipThink, clipFault, clipAsk;
  final Color scrim;

  static const dark = AxColors(
    brightness: Brightness.dark,
    ink1: Color(0xFFE8EAEF),
    ink2: Color(0xFFBDC2CB),
    ink3: Color(0xFF9DA3AE),
    ink4: Color(0xFF959BA7),
    voice: Color(0xFFAFB6C3),
    onPrimary: Color(0xFFFFFFFF),
    onAttention: Color(0xFFF6ECD9),
    live: Color(0xFF4A6FE3),
    liveText: Color(0xFF92A9F2),
    attention: Color(0xFFD2A052),
    attentionText: Color(0xFFDDB06A),
    fault: Color(0xFFD9655E),
    faultText: Color(0xFFE88B84),
    done: Color(0xFF5FA383),
    doneText: Color(0xFF7DB99B),
    idle: Color(0xFF6E7683),
    focus: Color(0xFF6D8CF0),
    selection: Color(0x663F62C8),
    rule: Color(0xFF131518),
    ruleLight: Color(0x0DFFFFFF),
    bayRule: Color(0xFF1C212B),
    hover: Color(0x0AFFFFFF),
    meterTrack: Color(0x17FFFFFF),
    meterLow: Color(0xFF59647F),
    chassis: SurfaceSpec(
      top: Color(0xFF1F2126),
      bottom: Color(0xFF191B1F),
      edge: Color(0x0FFFFFFF),
    ),
    plate: SurfaceSpec(
      top: Color(0xFF2A2D33),
      bottom: Color(0xFF26292E),
      border: Color(0xFF141519),
      edge: Color(0x12FFFFFF),
      shadows: [BoxShadow(color: Color(0x3D000000), blurRadius: 3, offset: Offset(0, 1))],
    ),
    plateHover: SurfaceSpec(
      top: Color(0xFF2D3037),
      bottom: Color(0xFF292C32),
      border: Color(0xFF141519),
      edge: Color(0x16FFFFFF),
      shadows: [BoxShadow(color: Color(0x47000000), blurRadius: 4, offset: Offset(0, 1))],
    ),
    plateSelected: SurfaceSpec(
      top: Color(0xFF34373F),
      bottom: Color(0xFF2F3239),
      border: Color(0xFF121317),
      edge: Color(0x1CFFFFFF),
      shadows: [BoxShadow(color: Color(0x59000000), blurRadius: 8, offset: Offset(0, 2))],
    ),
    bay: SurfaceSpec(
      top: Color(0xFF0E121A),
      bottom: Color(0xFF10151E),
      border: Color(0xFF0B0D12),
      inset: Color(0x42000000),
      lip: Color(0x0CFFFFFF),
    ),
    block: SurfaceSpec(
      top: Color(0xFF151A23),
      bottom: Color(0xFF141922),
      border: Color(0xFF222836),
    ),
    well: SurfaceSpec(
      top: Color(0xFF141619),
      bottom: Color(0xFF16181C),
      border: Color(0xFF0F1013),
      inset: Color(0x42000000),
      lip: Color(0x0CFFFFFF),
    ),
    key: SurfaceSpec(
      top: Color(0xFF3A3E46),
      bottom: Color(0xFF353941),
      border: Color(0xFF17191D),
      edge: Color(0x14FFFFFF),
      shadows: [BoxShadow(color: Color(0x2E000000), blurRadius: 1.5, offset: Offset(0, 1))],
    ),
    keyHover: SurfaceSpec(
      top: Color(0xFF42464F),
      bottom: Color(0xFF3D414A),
      border: Color(0xFF17191D),
      edge: Color(0x1CFFFFFF),
      shadows: [BoxShadow(color: Color(0x33000000), blurRadius: 2, offset: Offset(0, 1))],
    ),
    keyDown: SurfaceSpec(
      top: Color(0xFF16181C),
      bottom: Color(0xFF1A1C21),
      border: Color(0xFF101115),
      inset: Color(0x3D000000),
      lip: Color(0x0AFFFFFF),
    ),
    primary: SurfaceSpec(
      top: Color(0xFF3B5FCE),
      bottom: Color(0xFF3456C0),
      border: Color(0xFF1B2D6B),
      edge: Color(0x29FFFFFF),
      shadows: [BoxShadow(color: Color(0x38000000), blurRadius: 2, offset: Offset(0, 1))],
    ),
    primaryHover: SurfaceSpec(
      top: Color(0xFF4468DA),
      bottom: Color(0xFF3C5FCC),
      border: Color(0xFF1B2D6B),
      edge: Color(0x33FFFFFF),
      shadows: [BoxShadow(color: Color(0x40000000), blurRadius: 3, offset: Offset(0, 1))],
    ),
    primaryDown: SurfaceSpec(
      top: Color(0xFF2B489F),
      bottom: Color(0xFF2F4DAB),
      border: Color(0xFF17275C),
      inset: Color(0x3D000000),
      lip: Color(0x14FFFFFF),
    ),
    request: SurfaceSpec(
      top: Color(0xFF2E2B26),
      bottom: Color(0xFF2B2924),
      border: Color(0xFF191713),
      edge: Color(0x14FFE3B0),
      shadows: [BoxShadow(color: Color(0x40000000), blurRadius: 4, offset: Offset(0, 1))],
    ),
    answer: SurfaceSpec(
      top: Color(0xFF6A5431),
      bottom: Color(0xFF614D2E),
      border: Color(0xFF2E2412),
      edge: Color(0x24FFE9C4),
      shadows: [BoxShadow(color: Color(0x33000000), blurRadius: 2, offset: Offset(0, 1))],
    ),
    overlay: SurfaceSpec(
      top: Color(0xFF2D3037),
      bottom: Color(0xFF2A2D33),
      border: Color(0xFF0F1013),
      edge: Color(0x1AFFFFFF),
      shadows: [
        BoxShadow(color: Color(0x73000000), blurRadius: 28, offset: Offset(0, 12)),
        BoxShadow(color: Color(0x59000000), blurRadius: 5, offset: Offset(0, 2)),
      ],
    ),
    diffAdd: Color(0x1F5FA383),
    diffRemove: Color(0x22D9655E),
    codePlain: Color(0xFFA9B1BE),
    codeKeyword: Color(0xFF9FA9D0),
    codeString: Color(0xFFA6B99E),
    codeCall: Color(0xFFDCE0E7),
    codeType: Color(0xFFB9ABCA),
    codeNumber: Color(0xFFCBB48F),
    codeComment: Color(0xFF858D9A),
    clipWrite: Color(0xFF2E4280),
    clipRead: Color(0xFF2B3140),
    clipThink: Color(0xFF383D49),
    clipFault: Color(0xFF6B3A39),
    clipAsk: Color(0xFF6A5431),
    scrim: Color(0x8A08090B),
  );

  static const light = AxColors(
    brightness: Brightness.light,
    ink1: Color(0xFF171B23),
    ink2: Color(0xFF394150),
    ink3: Color(0xFF474F5D),
    ink4: Color(0xFF4F5765),
    voice: Color(0xFF45506A),
    onPrimary: Color(0xFFFFFFFF),
    onAttention: Color(0xFF3A2A08),
    live: Color(0xFF3355C4),
    liveText: Color(0xFF2545AE),
    attention: Color(0xFFA06F16),
    attentionText: Color(0xFF6E4A07),
    fault: Color(0xFFBE4038),
    faultText: Color(0xFF9C2A23),
    done: Color(0xFF357A59),
    doneText: Color(0xFF1F5F41),
    idle: Color(0xFF747C89),
    focus: Color(0xFF3B61DC),
    selection: Color(0x4D5B7EEB),
    rule: Color(0xFFB4B9C2),
    ruleLight: Color(0x8CFFFFFF),
    bayRule: Color(0xFFDDE1E8),
    hover: Color(0x0D10182C),
    meterTrack: Color(0x1F243149),
    meterLow: Color(0xFF8B96AF),
    chassis: SurfaceSpec(
      top: Color(0xFFD9DCE1),
      bottom: Color(0xFFD0D4DA),
      edge: Color(0x99FFFFFF),
    ),
    plate: SurfaceSpec(
      top: Color(0xFFE7E9ED),
      bottom: Color(0xFFE1E4E9),
      border: Color(0xFFB9BEC7),
      edge: Color(0xCCFFFFFF),
      shadows: [BoxShadow(color: Color(0x1F1A2233), blurRadius: 3, offset: Offset(0, 1))],
    ),
    plateHover: SurfaceSpec(
      top: Color(0xFFEBEDF1),
      bottom: Color(0xFFE5E8ED),
      border: Color(0xFFB3B9C3),
      edge: Color(0xE6FFFFFF),
      shadows: [BoxShadow(color: Color(0x261A2233), blurRadius: 4, offset: Offset(0, 1))],
    ),
    plateSelected: SurfaceSpec(
      top: Color(0xFFF6F7F9),
      bottom: Color(0xFFF1F3F6),
      border: Color(0xFFA5ACB8),
      edge: Color(0xFFFFFFFF),
      shadows: [BoxShadow(color: Color(0x331A2233), blurRadius: 8, offset: Offset(0, 2))],
    ),
    bay: SurfaceSpec(
      top: Color(0xFFF8F9FB),
      bottom: Color(0xFFFAFBFC),
      border: Color(0xFFB7BDC7),
      inset: Color(0x14243149),
      lip: Color(0x99FFFFFF),
    ),
    block: SurfaceSpec(
      top: Color(0xFFF0F2F6),
      bottom: Color(0xFFEFF1F5),
      border: Color(0xFFD5DAE3),
    ),
    well: SurfaceSpec(
      top: Color(0xFFC6CBD2),
      bottom: Color(0xFFCACED5),
      border: Color(0xFFAEB4BE),
      inset: Color(0x1F243149),
      lip: Color(0x80FFFFFF),
    ),
    key: SurfaceSpec(
      top: Color(0xFFFAFBFC),
      bottom: Color(0xFFF0F2F5),
      border: Color(0xFFAEB5C0),
      edge: Color(0xFFFFFFFF),
      shadows: [BoxShadow(color: Color(0x1F1A2233), blurRadius: 1.5, offset: Offset(0, 1))],
    ),
    keyHover: SurfaceSpec(
      top: Color(0xFFFFFFFF),
      bottom: Color(0xFFF6F7F9),
      border: Color(0xFF9FA7B4),
      edge: Color(0xFFFFFFFF),
      shadows: [BoxShadow(color: Color(0x291A2233), blurRadius: 2, offset: Offset(0, 1))],
    ),
    keyDown: SurfaceSpec(
      top: Color(0xFFCBD0D8),
      bottom: Color(0xFFD2D6DD),
      border: Color(0xFFA3ABB7),
      inset: Color(0x1F243149),
      lip: Color(0x80FFFFFF),
    ),
    primary: SurfaceSpec(
      top: Color(0xFF3E62D2),
      bottom: Color(0xFF3558C4),
      border: Color(0xFF23409C),
      edge: Color(0x47FFFFFF),
      shadows: [BoxShadow(color: Color(0x3324408F), blurRadius: 2, offset: Offset(0, 1))],
    ),
    primaryHover: SurfaceSpec(
      top: Color(0xFF476BDC),
      bottom: Color(0xFF3D60CE),
      border: Color(0xFF23409C),
      edge: Color(0x52FFFFFF),
      shadows: [BoxShadow(color: Color(0x4024408F), blurRadius: 3, offset: Offset(0, 1))],
    ),
    primaryDown: SurfaceSpec(
      top: Color(0xFF2B489F),
      bottom: Color(0xFF2F4DAB),
      border: Color(0xFF1F3886),
      inset: Color(0x3D000000),
      lip: Color(0x66FFFFFF),
    ),
    request: SurfaceSpec(
      top: Color(0xFFF6EEDD),
      bottom: Color(0xFFF2E9D6),
      border: Color(0xFFC9B68E),
      edge: Color(0xFFFFFBF0),
      shadows: [BoxShadow(color: Color(0x1F3A2A08), blurRadius: 4, offset: Offset(0, 1))],
    ),
    answer: SurfaceSpec(
      top: Color(0xFFEBCB8E),
      bottom: Color(0xFFE4C282),
      border: Color(0xFFAE8A48),
      edge: Color(0xA6FFFFFF),
      shadows: [BoxShadow(color: Color(0x263A2A08), blurRadius: 2, offset: Offset(0, 1))],
    ),
    overlay: SurfaceSpec(
      top: Color(0xFFF7F8FA),
      bottom: Color(0xFFF3F5F7),
      border: Color(0xFFABB2BD),
      edge: Color(0xFFFFFFFF),
      shadows: [
        BoxShadow(color: Color(0x331A2233), blurRadius: 28, offset: Offset(0, 12)),
        BoxShadow(color: Color(0x1F1A2233), blurRadius: 5, offset: Offset(0, 2)),
      ],
    ),
    diffAdd: Color(0x24357A59),
    diffRemove: Color(0x24BE4038),
    codePlain: Color(0xFF39414F),
    codeKeyword: Color(0xFF3D4B95),
    codeString: Color(0xFF3F6234),
    codeCall: Color(0xFF171B23),
    codeType: Color(0xFF65468A),
    codeNumber: Color(0xFF7A5314),
    codeComment: Color(0xFF5C6472),
    clipWrite: Color(0xFFA9B9EC),
    clipRead: Color(0xFFD0D6E0),
    clipThink: Color(0xFFC4CAD5),
    clipFault: Color(0xFFE8B3AF),
    clipAsk: Color(0xFFEBCB8E),
    scrim: Color(0x66262C36),
  );
}

/// Text styles. Archivo carries words; Spline Sans Mono carries code, paths and commands.
class AxType {
  const AxType(this._ink);
  final Color _ink;

  TextStyle _a(
    double size,
    double weight,
    double height, {
    double tracking = .15,
    double width = 100,
  }) => TextStyle(
    fontFamily: 'Archivo',
    fontSize: size,
    height: height,
    color: _ink,
    letterSpacing: tracking,
    leadingDistribution: TextLeadingDistribution.even,
    fontVariations: [FontVariation('wght', weight), FontVariation('wdth', width)],
    fontFeatures: const [FontFeature.tabularFigures()],
    decoration: TextDecoration.none,
  );
  TextStyle _m(double size, double height) => TextStyle(
    fontFamily: 'Spline Sans Mono',
    fontSize: size,
    height: height,
    color: _ink,
    leadingDistribution: TextLeadingDistribution.even,
    fontVariations: const [FontVariation('wght', 400)],
    decoration: TextDecoration.none,
  );

  /// Long reading. Measure is capped at about 76 characters by the transcript.
  TextStyle get prose => _a(14, 400, 1.6, tracking: .2);
  TextStyle get title => _a(15, 600, 1.35, tracking: .1);
  TextStyle get heading => _a(19, 600, 1.3, tracking: 0);
  TextStyle get ui => _a(13, 440, 1.35);
  TextStyle get uiStrong => _a(13, 590, 1.35, tracking: .1);
  TextStyle get label => _a(12, 440, 1.35);
  TextStyle get labelStrong => _a(12, 560, 1.35);
  TextStyle get small => _a(11, 440, 1.35, tracking: .2);

  /// Captions on keys and scales: narrower, so five selectors fit on one row.
  TextStyle get caption => _a(10, 500, 1.2, tracking: .25, width: 94);
  TextStyle get code => _m(12, 1.6);
  TextStyle get codeSmall => _m(11, 1.5);
  TextStyle get readout => _m(15, 1.2);
}

abstract final class AxSpace {
  static const double x1 = 2, x2 = 4, x3 = 6, x4 = 8, x5 = 12, x6 = 16, x7 = 24;
}

abstract final class AxRadius {
  static const double tick = 2, key = 4, plate = 5, bay = 6, overlay = 7;
}

/// Control sizes. Density changes height and padding, never type size.
enum AxDensity {
  compact(24, 24, 6),
  regular(28, 27, 8),
  roomy(32, 32, 10);

  const AxDensity(this.control, this.row, this.pad);
  final double control, row, pad;
}

/// Springs. A control answers in under 120 ms; nothing waits for an animation to end.
abstract final class AxMotion {
  /// Presses, hovers, switches: critically damped.
  static final SpringDescription snap = SpringDescription.withDampingRatio(
    mass: 1,
    stiffness: 1400,
    ratio: 1,
  );

  /// Panels, selections, strips: a trace of overshoot.
  static final SpringDescription glide = SpringDescription.withDampingRatio(
    mass: 1,
    stiffness: 520,
    ratio: .86,
  );
  static const Duration hoverDelay = Duration(milliseconds: 450);
  static const Duration breath = Duration(milliseconds: 1800);
  static const Duration blink = Duration(milliseconds: 1060);
}

@immutable
class AxTheme {
  const AxTheme({this.colors = AxColors.dark, this.density = AxDensity.regular});
  final AxColors colors;
  final AxDensity density;
  AxType get type => AxType(colors.ink1);
  @override
  bool operator ==(Object other) =>
      other is AxTheme && other.colors == colors && other.density == density;
  @override
  int get hashCode => Object.hash(colors, density);
}

class Ax extends InheritedWidget {
  const Ax({required this.theme, required super.child, super.key});
  final AxTheme theme;
  static AxTheme of(BuildContext c) =>
      c.dependOnInheritedWidgetOfExactType<Ax>()?.theme ?? const AxTheme();
  @override
  bool updateShouldNotify(Ax old) => old.theme != theme;
}

extension AxText on TextStyle {
  TextStyle ink(Color c) => copyWith(color: c);
}
