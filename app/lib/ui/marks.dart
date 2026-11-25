import 'package:flutter/widgets.dart';

import 'icons.dart';
import 'mark_data.dart';
import 'svg_path.dart';
import 'tokens.dart';

/// The services Axiom can name. A mark identifies a service that is really in use; it is never
/// decoration and never suggests that its owner endorses Axiom.
enum ServiceMark {
  axiom('Axiom'),
  local('This computer'),
  anthropic('Anthropic'),
  claude('Claude'),
  claudecode('Claude Code'),
  openai('OpenAI'),
  codex('Codex'),
  gemini('Gemini'),
  google('Google'),
  antigravity('Antigravity'),
  githubcopilot('GitHub Copilot'),
  cursor('Cursor'),
  xai('xAI'),
  grok('Grok'),
  deepseek('DeepSeek'),
  mistral('Mistral'),
  qwen('Qwen'),
  kimi('Kimi'),
  zai('Z.ai'),
  minimax('MiniMax'),
  meta('Meta'),
  nvidia('NVIDIA'),
  openrouter('OpenRouter'),
  groq('Groq'),
  cerebras('Cerebras'),
  fireworks('Fireworks'),
  together('Together AI'),
  huggingface('Hugging Face'),
  azure('Azure'),
  bedrock('Amazon Bedrock'),
  vertexai('Vertex AI'),
  ollama('Ollama'),
  lmstudio('LM Studio'),
  vllm('vLLM'),
  opencode('OpenCode'),
  goose('Goose'),
  cline('Cline'),
  amp('Amp');

  const ServiceMark(this.label);
  final String label;

  Color color(AxColors c) => c.services[this] ?? c.ink2;

  /// The service a model or harness name belongs to, when the name says so plainly.
  static ServiceMark? of(String? name) {
    final n = name?.toLowerCase() ?? '';
    if (n.isEmpty) return null;
    for (final (start, m) in const [
      ('claude code', claudecode),
      ('claude', claude),
      ('opus', claude),
      ('sonnet', claude),
      ('haiku', claude),
      ('fable', claude),
      ('gpt', openai),
      ('o3', openai),
      ('o4', openai),
      ('chatgpt', openai),
      ('codex', codex),
      ('gemini', gemini),
      ('antigravity', antigravity),
      ('copilot', githubcopilot),
      ('grok', grok),
      ('deepseek', deepseek),
      ('mistral', mistral),
      ('codestral', mistral),
      ('devstral', mistral),
      ('qwen', qwen),
      ('kimi', kimi),
      ('glm', zai),
      ('minimax', minimax),
      ('llama', meta),
      ('nemotron', nvidia),
      ('axiom', axiom),
      ('lm studio', lmstudio),
      ('ollama', ollama),
    ]) {
      if (n.startsWith(start)) return m;
    }
    for (final m in values) {
      if (n == m.label.toLowerCase() || n == m.name) return m;
    }
    return null;
  }

  static final _paths = <ServiceMark, Path>{};
  Path? get path {
    final d = markData[name];
    return d == null
        ? null
        : _paths[this] ??= d.fold(
            Path()..fillType = PathFillType.evenOdd,
            (p, x) => p..addPath(svgPath(x), Offset.zero),
          );
  }
}

class AxServiceMark extends StatelessWidget {
  const AxServiceMark(this.mark, {super.key, this.size = 13, this.color, this.labelled = false});
  final ServiceMark mark;
  final double size;
  final Color? color;

  /// Whether the mark stands alone, and so must be named to a screen reader.
  final bool labelled;
  @override
  Widget build(BuildContext context) {
    final c = Ax.of(context).colors;
    final size = MediaQuery.textScalerOf(context).scale(this.size);
    final tint = color ?? mark.color(c);
    final w = switch (mark) {
      ServiceMark.axiom => AxMark(size: size),
      ServiceMark.local => AxGlyph(AxIcon.terminal, size: this.size, color: tint),
      _ => CustomPaint(size: Size.square(size), painter: _MarkPainter(mark, tint)),
    };
    return labelled
        ? Semantics(label: mark.label, image: true, excludeSemantics: true, child: w)
        : ExcludeSemantics(child: w);
  }
}

class _MarkPainter extends CustomPainter {
  const _MarkPainter(this.mark, this.color);
  final ServiceMark mark;
  final Color color;
  @override
  void paint(Canvas c, Size s) {
    final k = s.shortestSide;
    c.translate((s.width - k) / 2, (s.height - k) / 2);
    c.scale(k / 24);
    c.drawPath(mark.path!, Paint()..color = color);
  }

  @override
  bool shouldRepaint(_MarkPainter o) => o.mark != mark || o.color != color;
}
