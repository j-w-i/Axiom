/// What people call a model. [said]: what its provider calls it, if it says. A name that is not known
/// to be made in some way is left as it is: nothing is guessed.
String modelName(String id, [String? said]) {
  final s = said?.trim() ?? '';
  if (s.isNotEmpty && s != id) {
    // Its mark says whose it is.
    return s.replaceFirst(RegExp(r'^(Anthropic: )?Claude (?=(Opus|Sonnet|Haiku|Fable)\b)'), '');
  }
  final long = RegExp(r'\[(\d+)m\]$', caseSensitive: false).firstMatch(id);
  var n = (long == null ? id : id.substring(0, long.start)).toLowerCase();
  // Whose it is or where it is run, before it; which day's it is, after.
  n = n.substring(n.lastIndexOf('/') + 1);
  n = n.replaceFirst(RegExp(r'^([a-z]+\.)?anthropic\.'), '');
  n = n.replaceFirst(RegExp(r'-v\d+(:\d+)?$'), '');
  n = n.replaceFirst(RegExp(r'[-@](\d{8}|\d{4}-\d{2}-\d{2}|latest)$'), '');
  final made = _claude(n) ?? _gpt(n) ?? _other(n);
  if (made == null) return id;
  return long == null ? made : '$made (${long[1]}M)';
}

final _claudeNew = RegExp(r'^(?:claude-)?(opus|sonnet|haiku|fable)(?:-(\d+)(?:[-.](\d{1,2}))?)?$');
final _claudeOld = RegExp(r'^claude-(\d)(?:[-.](\d))?-(opus|sonnet|haiku)$');

String? _claude(String n) {
  if (_claudeNew.firstMatch(n) case final m?) return _version(m[1]!, m[2], m[3]);
  if (_claudeOld.firstMatch(n) case final m?) return _version(m[3]!, m[1], m[2]);
  return null;
}

String _version(String name, String? major, String? minor) => [
  _upper(name),
  if (major != null) minor == null || minor == '0' ? major : '$major.$minor',
].join(' ');

String? _gpt(String n) {
  final m = RegExp(r'^(gpt|chatgpt)-(\d+(?:\.\d+)?o?|oss)((?:[-:][a-z0-9.]+)*)$').firstMatch(n);
  if (m == null) return null;
  return [
    '${m[1] == 'gpt' ? 'GPT' : 'ChatGPT'}-${m[2] == 'oss' ? 'OSS' : m[2]}',
    for (final w in _words(m[3]!))
      switch (w) {
        'mini' || 'nano' || 'chat' => w,
        _ => _word(w),
      },
  ].join(' ');
}

const _families = {
  'gemini': 'Gemini',
  'gemma': 'Gemma',
  'grok': 'Grok',
  'qwen': 'Qwen',
  'qwq': 'QwQ',
  'llama': 'Llama',
  'mistral': 'Mistral',
  'ministral': 'Ministral',
  'mixtral': 'Mixtral',
  'codestral': 'Codestral',
  'devstral': 'Devstral',
  'magistral': 'Magistral',
  'deepseek': 'DeepSeek',
  'kimi': 'Kimi',
  'glm': 'GLM',
  'phi': 'Phi',
  'minimax': 'MiniMax',
  'nemotron': 'Nemotron',
};

String? _other(String n) {
  final m = RegExp(r'^([a-z]+)([\d.]*)((?:[-_:][a-z0-9.]+)*)$').firstMatch(n);
  final name = m == null ? null : _families[m[1]];
  if (name == null) return null;
  return ['$name${m![2]}', for (final w in _words(m[3]!)) _word(w)].join(' ');
}

Iterable<String> _words(String s) => s.split(RegExp('[-_:]')).where((w) => w.isNotEmpty);

String _word(String w) {
  // How large it is, how it is kept, and short words that are said letter by letter.
  if (RegExp(r'^(\d+(\.\d+)?[bkm]|[a-z]\d+(\.\d+)?|it|vl|moe|oss|gguf|mlx|fp\d+|bf\d+)$')
      .hasMatch(w)) {
    return w.toUpperCase();
  }
  return _upper(w);
}

String _upper(String s) => s.isEmpty ? s : s[0].toUpperCase() + s.substring(1);

/// Of models that are offered together: what each is called, and its own name where two are called
/// the same.
Map<String, (String, String?)> modelNames(Iterable<(String id, String? said)> all) {
  final names = {for (final (id, said) in all) id: modelName(id, said)};
  final times = <String, int>{};
  for (final n in names.values) {
    times[n] = (times[n] ?? 0) + 1;
  }
  return {
    for (final MapEntry(:key, :value) in names.entries)
      key: (value, times[value]! > 1 && value != key ? key : null),
  };
}
