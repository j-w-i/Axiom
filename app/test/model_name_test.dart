import 'package:axiom/patterns/model_name.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('a model is called what people call it', () {
    for (final (id, name) in [
      ('claude-opus-5-5', 'Opus 5.5'),
      ('claude-sonnet-5', 'Sonnet 5'),
      ('claude-sonnet-4-0', 'Sonnet 4'),
      ('claude-sonnet-4-20250514', 'Sonnet 4'),
      ('claude-haiku-4-5-20251001', 'Haiku 4.5'),
      ('claude-3-5-sonnet-20241022', 'Sonnet 3.5'),
      ('claude-3-opus-latest', 'Opus 3'),
      ('claude-opus-4-1@20250805', 'Opus 4.1'),
      ('us.anthropic.claude-sonnet-4-5-20250929-v1:0', 'Sonnet 4.5'),
      ('anthropic/claude-opus-4.5', 'Opus 4.5'),
      ('claude-sonnet-5-5[1m]', 'Sonnet 5.5 (1M)'),
      ('opus', 'Opus'),
      ('gpt-5.6', 'GPT-5.6'),
      ('gpt-5.6-mini', 'GPT-5.6 mini'),
      ('gpt-5.1-codex-max', 'GPT-5.1 Codex Max'),
      ('gpt-4o-mini-2024-07-18', 'GPT-4o mini'),
      ('gpt-oss:20b', 'GPT-OSS 20B'),
      ('openai/gpt-5', 'GPT-5'),
      ('gemini-3.5-pro-preview', 'Gemini 3.5 Pro Preview'),
      ('qwen3:4b', 'Qwen3 4B'),
      ('qwen2.5-coder-7b-instruct', 'Qwen2.5 Coder 7B Instruct'),
      ('llama-3.3-70b', 'Llama 3.3 70B'),
      ('deepseek-r1', 'DeepSeek R1'),
      ('grok-4', 'Grok 4'),
    ]) {
      expect(modelName(id), name, reason: id);
    }
  });

  test('what is not known is left as it is', () {
    for (final id in [
      'o4-mini',
      'my-finetune_v2',
      'ft:gpt-5:me::abc',
      'claude-next',
      '',
      'x/y z',
    ]) {
      expect(modelName(id), id);
    }
  });

  test('what its provider says comes first', () {
    expect(modelName('claude-opus-5-5', 'Claude Opus 5.5'), 'Opus 5.5');
    expect(modelName('m1', 'The Model'), 'The Model');
    // Some say the same again.
    expect(modelName('claude-opus-5-5', 'claude-opus-5-5'), 'Opus 5.5');
    expect(modelName('claude-opus-5-5', ' '), 'Opus 5.5');
  });

  test('two that are called the same can be told apart', () {
    expect(
      modelNames([
        ('claude-sonnet-4-20250514', null),
        ('claude-sonnet-4-0', null),
        ('gpt-5.6', null),
      ]),
      {
        'claude-sonnet-4-20250514': ('Sonnet 4', 'claude-sonnet-4-20250514'),
        'claude-sonnet-4-0': ('Sonnet 4', 'claude-sonnet-4-0'),
        'gpt-5.6': ('GPT-5.6', null),
      },
    );
  });
}
