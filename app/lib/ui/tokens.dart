/// Every visual constant in Axiom. Nothing outside this file names a colour, a duration or a spring
/// (test/ui/tokens_test.dart enforces it and checks contrast in both themes).
///
/// The language: graphite structure, navy displays, cobalt only for what is live. Depth comes from
/// one light above: raised parts carry a faint lit top edge, displays sit slightly below the surface.
library;

import 'package:flutter/widgets.dart';

import 'marks.dart';

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
    required this.rowSelectedTop,
    required this.rowSelectedBottom,
    required this.meterTrack,
    required this.meterFill,
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
    required this.kindRead,
    required this.kindEdit,
    required this.kindSearch,
    required this.kindRun,
    required this.kindWeb,
    required this.kindThink,
    required this.codeInline,
    required this.promptRule,
    required this.services,
  });

  final Brightness brightness;

  /// Text, strongest to faintest. All four pass 4.5:1 on every surface they are allowed on.
  final Color ink1, ink2, ink3, ink4;

  /// The person's and other agents' words in a transcript.
  final Color voice;
  final Color onPrimary, onAttention;

  /// States. The plain colour is for marks (3:1), `…Text` for words (4.5:1).
  final Color live, liveText, attention, attentionText, fault, faultText, done, doneText, idle;
  final Color focus,
      selection,
      rule,
      ruleLight,
      bayRule,
      hover,
      rowSelectedTop,
      rowSelectedBottom,
      meterTrack,
      meterFill;
  final SurfaceSpec chassis, plate, plateHover, plateSelected, bay, block, well;
  final SurfaceSpec key, keyHover, keyDown, primary, primaryHover, primaryDown;
  final SurfaceSpec request, answer, overlay;
  final Color diffAdd, diffRemove;
  final Color codePlain, codeKeyword, codeString, codeCall, codeType, codeNumber, codeComment;
  final Color clipWrite, clipRead, clipThink, clipFault, clipAsk;
  final Color scrim;

  /// What kind of work: reading, changing, searching, running, the web, thinking or delegating.
  final Color kindRead, kindEdit, kindSearch, kindRun, kindWeb, kindThink;
  final Color codeInline, promptRule;

  /// A service's hue, for its mark and its levels. Neutral marks have none.
  final Map<ServiceMark, Color> services;

  /// The hover of something that belongs to a service.
  Color tinted(Color? hue) =>
      hue == null ? hover : hue.withValues(alpha: brightness == Brightness.dark ? .11 : .10);

  static const dark = AxColors(
    brightness: Brightness.dark,
    ink1: Color(0xFFE9EDF5),
    ink2: Color(0xFFC4CCD9),
    ink3: Color(0xFFB0B9C9),
    ink4: Color(0xFFA2ACBE),
    voice: Color(0xFFB2BCD0),
    onPrimary: Color(0xFFFFFFFF),
    onAttention: Color(0xFFF6ECD9),
    live: Color(0xFF5B80F2),
    liveText: Color(0xFF9DB3FA),
    attention: Color(0xFFE3A947),
    attentionText: Color(0xFFEBB965),
    fault: Color(0xFFE8635A),
    faultText: Color(0xFFF49A92),
    done: Color(0xFF52BC88),
    doneText: Color(0xFF7ED3A6),
    idle: Color(0xFF7D889C),
    focus: Color(0xFF7697F7),
    selection: Color(0x663F62C8),
    rule: Color(0xFF121317),
    ruleLight: Color(0x0DFFFFFF),
    bayRule: Color(0xFF292C32),
    hover: Color(0x0AFFFFFF),
    rowSelectedTop: Color(0xFF363A43),
    rowSelectedBottom: Color(0xFF32353E),
    meterTrack: Color(0x17FFFFFF),
    meterFill: Color(0xFF8A97B0),
    chassis: SurfaceSpec(
      top: Color(0xFF212429),
      bottom: Color(0xFF1C1E23),
      edge: Color(0x0FFFFFFF),
    ),
    plate: SurfaceSpec(
      top: Color(0xFF2B2E35),
      bottom: Color(0xFF272A30),
      border: Color(0xFF121317),
      edge: Color(0x12FFFFFF),
      shadows: [BoxShadow(color: Color(0x3D000000), blurRadius: 3, offset: Offset(0, 1))],
    ),
    plateHover: SurfaceSpec(
      top: Color(0xFF32353C),
      bottom: Color(0xFF2D3037),
      border: Color(0xFF121317),
      edge: Color(0x16FFFFFF),
      shadows: [BoxShadow(color: Color(0x47000000), blurRadius: 4, offset: Offset(0, 1))],
    ),
    plateSelected: SurfaceSpec(
      top: Color(0xFF2E3848),
      bottom: Color(0xFF293341),
      border: Color(0xFF0F131B),
      edge: Color(0x1CFFFFFF),
      shadows: [BoxShadow(color: Color(0x59000000), blurRadius: 8, offset: Offset(0, 2))],
    ),
    bay: SurfaceSpec(
      top: Color(0xFF17191D),
      bottom: Color(0xFF181A1E),
      border: Color(0xFF0F1013),
      inset: Color(0x42000000),
      lip: Color(0x0CFFFFFF),
    ),
    block: SurfaceSpec(
      top: Color(0xFF1F2226),
      bottom: Color(0xFF1E2125),
      border: Color(0xFF2C2F36),
    ),
    well: SurfaceSpec(
      top: Color(0xFF141619),
      bottom: Color(0xFF17191D),
      border: Color(0xFF0E0F12),
      inset: Color(0x42000000),
      lip: Color(0x0CFFFFFF),
    ),
    key: SurfaceSpec(
      top: Color(0xFF3B3F48),
      bottom: Color(0xFF363A42),
      border: Color(0xFF16181C),
      edge: Color(0x14FFFFFF),
      shadows: [BoxShadow(color: Color(0x2E000000), blurRadius: 1.5, offset: Offset(0, 1))],
    ),
    keyHover: SurfaceSpec(
      top: Color(0xFF444851),
      bottom: Color(0xFF3F434C),
      border: Color(0xFF16181C),
      edge: Color(0x1CFFFFFF),
      shadows: [BoxShadow(color: Color(0x33000000), blurRadius: 2, offset: Offset(0, 1))],
    ),
    keyDown: SurfaceSpec(
      top: Color(0xFF15171A),
      bottom: Color(0xFF191B1F),
      border: Color(0xFF0F1012),
      inset: Color(0x3D000000),
      lip: Color(0x0AFFFFFF),
    ),
    primary: SurfaceSpec(
      top: Color(0xFF3A60DC),
      bottom: Color(0xFF3257D0),
      border: Color(0xFF1B2F7C),
      edge: Color(0x29FFFFFF),
      shadows: [BoxShadow(color: Color(0x38000000), blurRadius: 2, offset: Offset(0, 1))],
    ),
    primaryHover: SurfaceSpec(
      top: Color(0xFF4168E0),
      bottom: Color(0xFF3A60DA),
      border: Color(0xFF1B2F7C),
      edge: Color(0x33FFFFFF),
      shadows: [BoxShadow(color: Color(0x40000000), blurRadius: 3, offset: Offset(0, 1))],
    ),
    primaryDown: SurfaceSpec(
      top: Color(0xFF2A49B4),
      bottom: Color(0xFF2E4FBF),
      border: Color(0xFF17275C),
      inset: Color(0x3D000000),
      lip: Color(0x14FFFFFF),
    ),
    request: SurfaceSpec(
      top: Color(0xFF35302A),
      bottom: Color(0xFF302C26),
      border: Color(0xFF1A1814),
      edge: Color(0x14FFE3B0),
      shadows: [BoxShadow(color: Color(0x40000000), blurRadius: 4, offset: Offset(0, 1))],
    ),
    answer: SurfaceSpec(
      top: Color(0xFF7C5F2E),
      bottom: Color(0xFF70552A),
      border: Color(0xFF33260F),
      edge: Color(0x24FFE9C4),
      shadows: [BoxShadow(color: Color(0x33000000), blurRadius: 2, offset: Offset(0, 1))],
    ),
    overlay: SurfaceSpec(
      top: Color(0xFF2C2F36),
      bottom: Color(0xFF282B31),
      border: Color(0xFF0E0F12),
      edge: Color(0x1AFFFFFF),
      shadows: [
        BoxShadow(color: Color(0x73000000), blurRadius: 28, offset: Offset(0, 12)),
        BoxShadow(color: Color(0x59000000), blurRadius: 5, offset: Offset(0, 2)),
      ],
    ),
    diffAdd: Color(0x2B4DB585),
    diffRemove: Color(0x30E5645B),
    codePlain: Color(0xFFB4BFD2),
    codeKeyword: Color(0xFFA3B2FF),
    codeString: Color(0xFF9FD08E),
    codeCall: Color(0xFFE6EBF5),
    codeType: Color(0xFFD9AEE6),
    codeNumber: Color(0xFFEBBC72),
    codeComment: Color(0xFF99A2B4),
    clipWrite: Color(0xFF2E4590),
    clipRead: Color(0xFF26344E),
    clipThink: Color(0xFF3A3C5A),
    clipFault: Color(0xFF73393A),
    clipAsk: Color(0xFF70552A),
    scrim: Color(0x8A05080E),
    kindRead: Color(0xFF8EA5CE),
    kindEdit: Color(0xFF93A5FF),
    kindSearch: Color(0xFF5FBFA8),
    kindRun: Color(0xFFDDAE62),
    kindWeb: Color(0xFF7FB58A),
    kindThink: Color(0xFFBCA3DE),
    codeInline: Color(0xFFB9C5FF),
    promptRule: Color(0xFF5B80F2),
    services: {
      ServiceMark.anthropic: Color(0xFFE08A66),
      ServiceMark.claude: Color(0xFFE08A66),
      ServiceMark.claudecode: Color(0xFFE08A66),
      ServiceMark.openai: Color(0xFF4FC2A1),
      ServiceMark.codex: Color(0xFF4FC2A1),
      ServiceMark.gemini: Color(0xFF8F9BFF),
      ServiceMark.google: Color(0xFF8F9BFF),
      ServiceMark.antigravity: Color(0xFF8F9BFF),
      ServiceMark.githubcopilot: Color(0xFFB795E8),
      ServiceMark.deepseek: Color(0xFF7590FF),
      ServiceMark.mistral: Color(0xFFF08E45),
      ServiceMark.qwen: Color(0xFFA18CF0),
      ServiceMark.minimax: Color(0xFFEC6C8B),
      ServiceMark.meta: Color(0xFF6D9BFF),
      ServiceMark.nvidia: Color(0xFF92C845),
      ServiceMark.openrouter: Color(0xFF9DB0D6),
      ServiceMark.groq: Color(0xFFF0775A),
      ServiceMark.cerebras: Color(0xFFF0825A),
      ServiceMark.fireworks: Color(0xFFA884F0),
      ServiceMark.together: Color(0xFF6D9BFF),
      ServiceMark.huggingface: Color(0xFFEBC34B),
      ServiceMark.azure: Color(0xFF6F9FF0),
      ServiceMark.bedrock: Color(0xFF4FC2A1),
      ServiceMark.vertexai: Color(0xFF8F9BFF),
      ServiceMark.lmstudio: Color(0xFF9F8CF2),
      ServiceMark.vllm: Color(0xFFEBB94B),
      ServiceMark.cline: Color(0xFFB9A3F0),
      ServiceMark.amp: Color(0xFFF0775A),
    },
  );

  static const light = AxColors(
    brightness: Brightness.light,
    ink1: Color(0xFF171B23),
    ink2: Color(0xFF394150),
    ink3: Color(0xFF424A58),
    ink4: Color(0xFF4A5260),
    voice: Color(0xFF45506A),
    onPrimary: Color(0xFFFFFFFF),
    onAttention: Color(0xFF3A2A08),
    live: Color(0xFF3355C4),
    liveText: Color(0xFF2545AE),
    attention: Color(0xFF966612),
    attentionText: Color(0xFF6E4A07),
    fault: Color(0xFFBE4038),
    faultText: Color(0xFF98261F),
    done: Color(0xFF357A59),
    doneText: Color(0xFF1B5A3D),
    idle: Color(0xFF6B7380),
    focus: Color(0xFF3B61DC),
    selection: Color(0x4D5B7EEB),
    rule: Color(0xFFAAB6C9),
    ruleLight: Color(0x8CFFFFFF),
    bayRule: Color(0xFFD6DEEC),
    hover: Color(0x0D10182C),
    rowSelectedTop: Color(0xFFF4F7FF),
    rowSelectedBottom: Color(0xFFEEF3FD),
    meterTrack: Color(0x1F243149),
    meterFill: Color(0xFF66748E),
    chassis: SurfaceSpec(
      top: Color(0xFFD3DAE6),
      bottom: Color(0xFFC9D1DF),
      edge: Color(0x99FFFFFF),
    ),
    plate: SurfaceSpec(
      top: Color(0xFFE4E9F2),
      bottom: Color(0xFFDDE3EE),
      border: Color(0xFFB2BDD0),
      edge: Color(0xCCFFFFFF),
      shadows: [BoxShadow(color: Color(0x1F1A2233), blurRadius: 3, offset: Offset(0, 1))],
    ),
    plateHover: SurfaceSpec(
      top: Color(0xFFE9EEF6),
      bottom: Color(0xFFE2E8F2),
      border: Color(0xFFABB7CB),
      edge: Color(0xE6FFFFFF),
      shadows: [BoxShadow(color: Color(0x261A2233), blurRadius: 4, offset: Offset(0, 1))],
    ),
    plateSelected: SurfaceSpec(
      top: Color(0xFFF4F7FC),
      bottom: Color(0xFFEFF3F9),
      border: Color(0xFF9DABC2),
      edge: Color(0xFFFFFFFF),
      shadows: [BoxShadow(color: Color(0x331A2233), blurRadius: 8, offset: Offset(0, 2))],
    ),
    bay: SurfaceSpec(
      top: Color(0xFFF9FAFE),
      bottom: Color(0xFFFBFCFF),
      border: Color(0xFFB0BBCE),
      inset: Color(0x14243149),
      lip: Color(0x99FFFFFF),
    ),
    block: SurfaceSpec(
      top: Color(0xFFEFF3FB),
      bottom: Color(0xFFEDF1FA),
      border: Color(0xFFD0D9E8),
    ),
    well: SurfaceSpec(
      top: Color(0xFFC1CAD8),
      bottom: Color(0xFFC5CEDC),
      border: Color(0xFFA7B3C6),
      inset: Color(0x1F243149),
      lip: Color(0x80FFFFFF),
    ),
    key: SurfaceSpec(
      top: Color(0xFFFAFCFF),
      bottom: Color(0xFFEFF3F9),
      border: Color(0xFFA6B3C8),
      edge: Color(0xFFFFFFFF),
      shadows: [BoxShadow(color: Color(0x1F1A2233), blurRadius: 1.5, offset: Offset(0, 1))],
    ),
    keyHover: SurfaceSpec(
      top: Color(0xFFFFFFFF),
      bottom: Color(0xFFF5F8FC),
      border: Color(0xFF96A5BD),
      edge: Color(0xFFFFFFFF),
      shadows: [BoxShadow(color: Color(0x291A2233), blurRadius: 2, offset: Offset(0, 1))],
    ),
    keyDown: SurfaceSpec(
      top: Color(0xFFC6CFDD),
      bottom: Color(0xFFCDD5E2),
      border: Color(0xFF9AA8BE),
      inset: Color(0x1F243149),
      lip: Color(0x80FFFFFF),
    ),
    primary: SurfaceSpec(
      top: Color(0xFF3E62D2),
      bottom: Color(0xFF385BC8),
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
      top: Color(0xFFFBEFD3),
      bottom: Color(0xFFF7E9C9),
      border: Color(0xFFD0B377),
      edge: Color(0xFFFFFBF0),
      shadows: [BoxShadow(color: Color(0x1F3A2A08), blurRadius: 4, offset: Offset(0, 1))],
    ),
    answer: SurfaceSpec(
      top: Color(0xFFF0C875),
      bottom: Color(0xFFE9BF69),
      border: Color(0xFFB28A3A),
      edge: Color(0xA6FFFFFF),
      shadows: [BoxShadow(color: Color(0x263A2A08), blurRadius: 2, offset: Offset(0, 1))],
    ),
    overlay: SurfaceSpec(
      top: Color(0xFFF6F9FE),
      bottom: Color(0xFFF2F5FB),
      border: Color(0xFFA2AFC4),
      edge: Color(0xFFFFFFFF),
      shadows: [
        BoxShadow(color: Color(0x331A2233), blurRadius: 28, offset: Offset(0, 12)),
        BoxShadow(color: Color(0x1F1A2233), blurRadius: 5, offset: Offset(0, 2)),
      ],
    ),
    diffAdd: Color(0x2E2F8A5C),
    diffRemove: Color(0x2ECB453D),
    codePlain: Color(0xFF39414F),
    codeKeyword: Color(0xFF3244B4),
    codeString: Color(0xFF2F6A2A),
    codeCall: Color(0xFF171B23),
    codeType: Color(0xFF7B3A94),
    codeNumber: Color(0xFF7F4F08),
    codeComment: Color(0xFF4F5A6E),
    clipWrite: Color(0xFFA9B9EC),
    clipRead: Color(0xFFD0D6E0),
    clipThink: Color(0xFFC4CAD5),
    clipFault: Color(0xFFE8B3AF),
    clipAsk: Color(0xFFEBCB8E),
    scrim: Color(0x66202A3C),
    kindRead: Color(0xFF45608F),
    kindEdit: Color(0xFF3448B8),
    kindSearch: Color(0xFF14725F),
    kindRun: Color(0xFF85590B),
    kindWeb: Color(0xFF2E6B3B),
    kindThink: Color(0xFF6A4A98),
    codeInline: Color(0xFF3044A8),
    promptRule: Color(0xFF3355C4),
    services: {
      ServiceMark.anthropic: Color(0xFFAD4F2B),
      ServiceMark.claude: Color(0xFFAD4F2B),
      ServiceMark.claudecode: Color(0xFFAD4F2B),
      ServiceMark.openai: Color(0xFF0B7559),
      ServiceMark.codex: Color(0xFF0B7559),
      ServiceMark.gemini: Color(0xFF4552C4),
      ServiceMark.google: Color(0xFF4552C4),
      ServiceMark.antigravity: Color(0xFF4552C4),
      ServiceMark.githubcopilot: Color(0xFF6C3FB0),
      ServiceMark.deepseek: Color(0xFF3349C9),
      ServiceMark.mistral: Color(0xFFA9500C),
      ServiceMark.qwen: Color(0xFF5A3FC0),
      ServiceMark.minimax: Color(0xFFB12C50),
      ServiceMark.meta: Color(0xFF2257C4),
      ServiceMark.nvidia: Color(0xFF47700A),
      ServiceMark.openrouter: Color(0xFF4A5D84),
      ServiceMark.groq: Color(0xFFB03E20),
      ServiceMark.cerebras: Color(0xFFB0461A),
      ServiceMark.fireworks: Color(0xFF6238B8),
      ServiceMark.together: Color(0xFF2257C4),
      ServiceMark.huggingface: Color(0xFF85630A),
      ServiceMark.azure: Color(0xFF1F5AA8),
      ServiceMark.bedrock: Color(0xFF0B7559),
      ServiceMark.vertexai: Color(0xFF4552C4),
      ServiceMark.lmstudio: Color(0xFF5640BE),
      ServiceMark.vllm: Color(0xFF85630A),
      ServiceMark.cline: Color(0xFF6C3FB0),
      ServiceMark.amp: Color(0xFFB03E20),
    },
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
