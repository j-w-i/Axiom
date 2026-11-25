/// Every visual constant in Axiom. Nothing outside this file names a colour, a duration or a spring
/// (test/ui/tokens_test.dart enforces it and checks contrast in both themes).
///
/// The language: flat, near-neutral charcoal; hairlines where a part ends; aqua only for what runs or
/// has the keyboard. Only what floats casts a shadow.
library;

import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import 'marks.dart';

/// How one kind of surface is built. A surface is a role, not a colour (docs/design/system.md).
@immutable
class SurfaceSpec {
  const SurfaceSpec(this.fill, {this.border = const Color(0x00000000), this.shadows = const []});
  final Color fill;

  /// The hairline that says where the part ends.
  final Color border;

  /// Only what floats has any.
  final List<BoxShadow> shadows;
}

@immutable
class AxColors {
  const AxColors({
    required this.brightness,
    required this.ink1,
    required this.ink2,
    required this.ink3,
    required this.ink4,
    required this.onPrimary,
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
    required this.hairline,
    required this.selected,
    required this.hover,
    required this.meterTrack,
    required this.meterFill,
    required this.chassis,
    required this.plate,
    required this.bay,
    required this.block,
    required this.key,
    required this.keyHover,
    required this.keyDown,
    required this.primary,
    required this.primaryHover,
    required this.primaryDown,
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
    required this.codeInline,
    required this.codeTile,
    required this.services,
  });

  final Brightness brightness;

  /// Text, strongest to faintest. All pass 4.5:1 on every surface they are allowed on: [ink4] on the
  /// chassis and the bay only.
  final Color ink1, ink2, ink3, ink4;
  final Color onPrimary;

  /// States. The plain colour is for marks (3:1), `…Text` for words (4.5:1).
  final Color live, liveText, attention, attentionText, fault, faultText, done, doneText, idle;

  /// What is selected is filled with [selected], whoever it belongs to.
  final Color focus, selection, hairline, selected, hover, meterTrack, meterFill;
  final SurfaceSpec chassis, plate, bay, block;
  final SurfaceSpec key, keyHover, keyDown, primary, primaryHover, primaryDown;
  final SurfaceSpec overlay;
  final Color diffAdd, diffRemove;
  final Color codePlain, codeKeyword, codeString, codeCall, codeType, codeNumber, codeComment;

  /// Code among words: its ink, and the tile it stands on.
  final Color codeInline, codeTile;

  /// A service's hue, for its mark and its levels. Neutral marks have none.
  final Map<ServiceMark, Color> services;

  /// The hover of something that belongs to a service.
  Color tinted(Color? hue) =>
      hue == null ? hover : hue.withValues(alpha: brightness == Brightness.dark ? .11 : .10);

  static const dark = AxColors(
    brightness: Brightness.dark,
    ink1: Color(0xFFECEDED),
    ink2: Color(0xFFBCBFC5),
    ink3: Color(0xFFA0A4AC),
    ink4: Color(0xFF8F939B),
    onPrimary: Color(0xFF151619),
    live: Color(0xFF35C3F3),
    liveText: Color(0xFF86DCFB),
    attention: Color(0xFFE2AA52),
    attentionText: Color(0xFFEBB965),
    fault: Color(0xFFEE766E),
    faultText: Color(0xFFF49A92),
    done: Color(0xFF5FBD8B),
    doneText: Color(0xFF7ED3A6),
    idle: Color(0xFF8C9098),
    focus: Color(0xFF55CDF6),
    selection: Color(0x6620A9DB),
    hairline: Color(0x14FFFFFF),
    selected: Color(0x13FFFFFF),
    hover: Color(0x0AFFFFFF),
    meterTrack: Color(0x1FFFFFFF),
    meterFill: Color(0xFFA8ACB4),
    chassis: SurfaceSpec(Color(0xFF1C1D20)),
    plate: SurfaceSpec(Color(0xFF232529), border: Color(0x1FFFFFFF)),
    bay: SurfaceSpec(Color(0xFF151619)),
    block: SurfaceSpec(Color(0xFF232529)),
    key: SurfaceSpec(Color(0xFF2A2C31), border: Color(0x1FFFFFFF)),
    keyHover: SurfaceSpec(Color(0xFF33353B), border: Color(0x29FFFFFF)),
    keyDown: SurfaceSpec(Color(0xFF1F2125), border: Color(0x1FFFFFFF)),
    primary: SurfaceSpec(Color(0xFFE9EAEC)),
    primaryHover: SurfaceSpec(Color(0xFFFFFFFF)),
    primaryDown: SurfaceSpec(Color(0xFFC9CBCF)),
    overlay: SurfaceSpec(
      Color(0xFF26282C),
      border: Color(0x1FFFFFFF),
      shadows: [
        BoxShadow(color: Color(0x66000000), blurRadius: 32, offset: Offset(0, 12)),
        BoxShadow(color: Color(0x40000000), blurRadius: 4, offset: Offset(0, 1)),
      ],
    ),
    diffAdd: Color(0x264DB585),
    diffRemove: Color(0x2AE5645B),
    codePlain: Color(0xFFC9CCD2),
    codeKeyword: Color(0xFF8FB7E8),
    codeString: Color(0xFF9FD08E),
    codeCall: Color(0xFFECEDED),
    codeType: Color(0xFFD9AEE6),
    codeNumber: Color(0xFFEBBC72),
    codeComment: Color(0xFFA0A4AC),
    codeInline: Color(0xFFDADCE0),
    codeTile: Color(0xFF232529),
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
    ink1: Color(0xFF191A1C),
    ink2: Color(0xFF484B51),
    ink3: Color(0xFF565A61),
    ink4: Color(0xFF646870),
    onPrimary: Color(0xFFFFFFFF),
    live: Color(0xFF08709C),
    liveText: Color(0xFF06587B),
    attention: Color(0xFF966612),
    attentionText: Color(0xFF6E4A07),
    fault: Color(0xFFBB3D35),
    faultText: Color(0xFF98261F),
    done: Color(0xFF2D7A55),
    doneText: Color(0xFF1B5A3D),
    idle: Color(0xFF6B7078),
    focus: Color(0xFF0874A2),
    selection: Color(0x4D35B9E6),
    hairline: Color(0x17000000),
    selected: Color(0x11000000),
    hover: Color(0x09000000),
    meterTrack: Color(0x1A000000),
    meterFill: Color(0xFF60646C),
    chassis: SurfaceSpec(Color(0xFFF2F2F3)),
    plate: SurfaceSpec(Color(0xFFF6F6F7), border: Color(0x24000000)),
    bay: SurfaceSpec(Color(0xFFFFFFFF)),
    block: SurfaceSpec(Color(0xFFF4F4F5)),
    key: SurfaceSpec(Color(0xFFFFFFFF), border: Color(0x29000000)),
    keyHover: SurfaceSpec(Color(0xFFF6F6F7), border: Color(0x38000000)),
    keyDown: SurfaceSpec(Color(0xFFEAEAEC), border: Color(0x29000000)),
    primary: SurfaceSpec(Color(0xFF202226)),
    primaryHover: SurfaceSpec(Color(0xFF36393F)),
    primaryDown: SurfaceSpec(Color(0xFF0E0F11)),
    overlay: SurfaceSpec(
      Color(0xFFFFFFFF),
      border: Color(0x24000000),
      shadows: [
        BoxShadow(color: Color(0x24000000), blurRadius: 32, offset: Offset(0, 12)),
        BoxShadow(color: Color(0x14000000), blurRadius: 4, offset: Offset(0, 1)),
      ],
    ),
    diffAdd: Color(0x222F8A5C),
    diffRemove: Color(0x1FCB453D),
    codePlain: Color(0xFF33363B),
    codeKeyword: Color(0xFF1F5FA8),
    codeString: Color(0xFF2F6A2A),
    codeCall: Color(0xFF191A1C),
    codeType: Color(0xFF7B3A94),
    codeNumber: Color(0xFF7F4F08),
    codeComment: Color(0xFF5A5E66),
    codeInline: Color(0xFF2A2C30),
    codeTile: Color(0xFFF0F0F1),
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

/// Text styles. Inter carries words; JetBrains Mono carries code, paths and commands.
class AxType {
  const AxType(this._ink);
  final Color _ink;

  TextStyle _w(double size, double weight, double height, {double tracking = 0}) => TextStyle(
    fontFamily: 'Inter',
    fontSize: size,
    height: height,
    color: _ink,
    letterSpacing: tracking,
    leadingDistribution: TextLeadingDistribution.even,
    fontVariations: [FontVariation('wght', weight), const FontVariation('opsz', 14)],
    decoration: TextDecoration.none,
  );

  /// No ligatures: what is shown of a literal is its characters.
  TextStyle _m(double size, double height) => TextStyle(
    fontFamily: 'JetBrains Mono',
    fontSize: size,
    height: height,
    color: _ink,
    leadingDistribution: TextLeadingDistribution.even,
    fontVariations: const [FontVariation('wght', 400)],
    fontFeatures: const [FontFeature.disable('liga'), FontFeature.disable('calt')],
    decoration: TextDecoration.none,
  );

  /// Long reading. Measure is capped by the transcript.
  TextStyle get prose => _w(14, 400, 1.6);
  TextStyle get title => _w(15, 600, 1.35);
  TextStyle get heading => _w(19, 600, 1.3, tracking: -.2);
  TextStyle get ui => _w(13, 400, 1.4);
  TextStyle get uiMedium => _w(13, 500, 1.4);
  TextStyle get uiStrong => _w(13, 600, 1.4);
  TextStyle get label => _w(12.5, 400, 1.4);
  TextStyle get labelStrong => _w(12.5, 500, 1.4);
  TextStyle get small => _w(11.5, 400, 1.4);
  TextStyle get smallStrong => _w(11.5, 500, 1.4);
  TextStyle get caption => _w(10.5, 500, 1.2);
  TextStyle get code => _m(12, 1.6);
  TextStyle get codeSmall => _m(11.5, 1.55);
}

abstract final class AxSpace {
  /// The conversation, what is asked in it and where it is written: one column.
  static const double column = 720, gutter = 24;

  /// What stands beside that column, in [width].
  static double beside(double width) => math.max(gutter, (width - column) / 2);
}

/// A control, a tile of code, a card or what floats. What is inside is never rounder than its holder.
abstract final class AxRadius {
  static const double tick = 4, key = 6, block = 8, card = 10;
}

/// Control sizes. Density changes height and padding, never type size.
enum AxDensity {
  compact(24, 24, 7),
  regular(28, 28, 9),
  roomy(32, 32, 11);

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

  /// How long something new under a resting pointer cannot be pressed: a double click.
  static const Duration settle = Duration(milliseconds: 500);
  static const Duration turn = Duration(milliseconds: 1400);
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
