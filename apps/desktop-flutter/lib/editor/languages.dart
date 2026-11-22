// The language registry (M5.5-LANG-001, ADR-0021 §1): every grammar re_highlight ships, with how
// files are recognised (extension, exact name, name pattern, shebang interpreter, first line), the
// LSP language id where LSP defines one, and a display name. Grammars are built on first use.

import 'package:re_highlight/re_highlight.dart';

import 'languages.g.dart';

/// One language: its id (the LSP id where LSP has one, else the highlight.js name), its display
/// name, and the grammar that highlights it (null: plain text).
class Language {
  const Language(this.id, this.name, this.hljs);
  final String id;
  final String name;
  final String? hljs;

  @override
  String toString() => id;
}

/// highlight.js names whose LSP id differs (VS Code's language identifiers).
const _lspIds = {
  'bash': 'shellscript',
  'objectivec': 'objective-c',
  'dos': 'bat',
  'vbnet': 'vb',
  'plaintext': 'plaintext',
  'xml': 'xml',
  'pgsql': 'sql',
};

/// Languages with no grammar of their own: (id, name, the grammar that colours them).
const _extra = <(String, String, String?)>[
  ('html', 'HTML', 'xml'),
  ('jsonc', 'JSON with Comments', 'json'),
  ('typescriptreact', 'TypeScript JSX', 'typescript'),
  ('javascriptreact', 'JavaScript JSX', 'javascript'),
  ('toml', 'TOML', 'ini'),
  ('svelte', 'Svelte', 'xml'),
  ('astro', 'Astro', 'xml'),
  ('mdx', 'MDX', 'markdown'),
  ('zig', 'Zig', null),
  ('terraform', 'Terraform', null),
  ('hcl', 'HCL', null),
  ('git-commit', 'Git Commit Message', null),
  ('ignore', 'Ignore', 'bash'),
  ('dotenv', 'Environment Variables', 'bash'),
  ('csv', 'CSV', null),
  ('log', 'Log', null),
];

/// Extensions per language, as VS Code recognises them; these win over grammar aliases.
const _extensions = <String, List<String>>{
  'rust': ['rs'],
  'typescript': ['ts', 'mts', 'cts'],
  'typescriptreact': ['tsx'],
  'javascript': ['js', 'mjs', 'cjs', 'es6'],
  'javascriptreact': ['jsx'],
  'json': ['json', 'webmanifest', 'jsonl', 'geojson'],
  'jsonc': ['jsonc', 'code-workspace'],
  'python': ['py', 'pyi', 'pyw', 'pyx', 'rpy', 'gyp'],
  'go': ['go'],
  'c': ['c', 'h'],
  'cpp': ['cpp', 'cc', 'cxx', 'c++', 'hpp', 'hh', 'hxx', 'h++', 'ipp', 'inl', 'tpp', 'cu', 'cuh'],
  'csharp': ['cs', 'csx'],
  'java': ['java', 'jav'],
  'kotlin': ['kt', 'kts'],
  'scala': ['scala', 'sc', 'sbt'],
  'groovy': ['groovy', 'gvy'],
  'gradle': ['gradle'],
  'swift': ['swift'],
  'objective-c': ['m'],
  'dart': ['dart'],
  'ruby': ['rb', 'rbw', 'gemspec', 'rake', 'ru', 'podspec', 'rbi'],
  'php': ['php', 'php4', 'php5', 'phtml', 'ctp'],
  'perl': ['pl', 'pm', 'pod', 't', 'psgi'],
  'lua': ['lua'],
  'r': ['r', 'rhistory', 'rprofile'],
  'julia': ['jl'],
  'haskell': ['hs', 'lhs'],
  'elixir': ['ex', 'exs'],
  'erlang': ['erl', 'hrl'],
  'ocaml': ['ml', 'mli'],
  'fsharp': ['fs', 'fsi', 'fsx', 'fsscript'],
  'clojure': ['clj', 'cljs', 'cljc', 'edn'],
  'elm': ['elm'],
  'nim': ['nim', 'nims', 'nimble'],
  'crystal': ['cr'],
  'd': ['d', 'di'],
  'vb': ['vb'],
  'powershell': ['ps1', 'psm1', 'psd1'],
  'shellscript': ['sh', 'bash', 'zsh', 'ksh', 'ebuild', 'bats', 'command'],
  'bat': ['bat', 'cmd'],
  'html': ['html', 'htm', 'shtml', 'xhtml', 'mdoc', 'jsp', 'asp', 'aspx', 'jshtm'],
  'xml': [
    'xml',
    'xsd',
    'xsl',
    'xslt',
    'svg',
    'plist',
    'xaml',
    'csproj',
    'fsproj',
    'vbproj',
    'props',
    'targets',
    'resx',
    'storyboard',
    'xib',
    'wsdl',
    'rss',
    'atom',
    'iml',
    'nuspec',
  ],
  'css': ['css'],
  'scss': ['scss'],
  'less': ['less'],
  'stylus': ['styl'],
  'markdown': ['md', 'markdown', 'mkd', 'mdwn', 'mdown', 'mkdn', 'ronn', 'workbook'],
  'mdx': ['mdx'],
  'yaml': ['yaml', 'yml', 'eyaml', 'eyml'],
  'toml': ['toml'],
  'ini': ['ini', 'cfg', 'conf', 'editorconfig', 'gitconfig', 'desktop', 'service', 'inf'],
  'properties': ['properties'],
  'sql': ['sql', 'dsql', 'ddl', 'dml'],
  'graphql': ['graphql', 'gql', 'graphqls'],
  'protobuf': ['proto'],
  'thrift': ['thrift'],
  'dockerfile': ['dockerfile', 'containerfile'],
  'makefile': ['mk', 'mak', 'make'],
  'cmake': ['cmake'],
  'diff': ['diff', 'patch', 'rej'],
  'latex': ['tex', 'ltx', 'sty', 'cls', 'bib'],
  'vue': ['vue'],
  'svelte': ['svelte'],
  'astro': ['astro'],
  'handlebars': ['handlebars', 'hbs'],
  'twig': ['twig'],
  'erb': ['erb', 'rhtml'],
  'haml': ['haml'],
  'zig': ['zig', 'zon'],
  'terraform': ['tf', 'tfvars'],
  'hcl': ['hcl', 'nomad'],
  'nix': ['nix'],
  'glsl': ['glsl', 'vert', 'frag', 'geom', 'comp', 'tesc', 'tese'],
  'wasm': ['wat', 'wast'],
  'llvm': ['ll'],
  'x86asm': ['asm', 'nasm'],
  'armasm': ['s'],
  'verilog': ['v', 'sv', 'svh', 'vh'],
  'vhdl': ['vhd', 'vhdl'],
  'qml': ['qml'],
  'vim': ['vim', 'vimrc'],
  'tcl': ['tcl', 'tk'],
  'awk': ['awk'],
  'prolog': ['pro', 'prolog'],
  'scheme': ['scm', 'ss', 'rkt'],
  'lisp': ['lisp', 'lsp', 'cl', 'el'],
  'fortran': ['f', 'for', 'f90', 'f95', 'f03', 'f08'],
  'ada': ['adb', 'ads'],
  'matlab': ['matlab'],
  'mathematica': ['nb', 'wl', 'wls'],
  'coffeescript': ['coffee', 'cson', 'iced'],
  'livescript': ['ls'],
  'reasonml': ['re', 'rei'],
  'sml': ['sml', 'sig'],
  'pony': ['pony'],
  'wren': ['wren'],
  'gherkin': ['feature'],
  'nginx': ['nginxconf'],
  'apache': ['htaccess'],
  'http': ['http', 'rest'],
  'csv': ['csv', 'tsv', 'psv'],
  'log': ['log'],
  'plaintext': ['txt', 'text'],
};

/// Exact file names.
const _fileNames = <String, String>{
  'Dockerfile': 'dockerfile',
  'Containerfile': 'dockerfile',
  'Makefile': 'makefile',
  'makefile': 'makefile',
  'GNUmakefile': 'makefile',
  'CMakeLists.txt': 'cmake',
  'Cargo.lock': 'toml',
  'Pipfile': 'toml',
  'poetry.lock': 'toml',
  'uv.lock': 'toml',
  'go.mod': 'go',
  'go.sum': 'plaintext',
  'Gemfile': 'ruby',
  'Rakefile': 'ruby',
  'Podfile': 'ruby',
  'Fastfile': 'ruby',
  'Vagrantfile': 'ruby',
  'Brewfile': 'ruby',
  'Jenkinsfile': 'groovy',
  'BUILD': 'python',
  'WORKSPACE': 'python',
  'BUILD.bazel': 'python',
  'Justfile': 'makefile',
  'justfile': 'makefile',
  '.bashrc': 'shellscript',
  '.bash_profile': 'shellscript',
  '.bash_aliases': 'shellscript',
  '.profile': 'shellscript',
  '.zshrc': 'shellscript',
  '.zprofile': 'shellscript',
  '.zshenv': 'shellscript',
  'PKGBUILD': 'shellscript',
  '.gitignore': 'ignore',
  '.dockerignore': 'ignore',
  '.npmignore': 'ignore',
  '.prettierignore': 'ignore',
  '.eslintignore': 'ignore',
  '.gitattributes': 'ignore',
  '.gitconfig': 'ini',
  '.editorconfig': 'ini',
  '.npmrc': 'ini',
  '.env': 'dotenv',
  'COMMIT_EDITMSG': 'git-commit',
  'MERGE_MSG': 'git-commit',
  'tsconfig.json': 'jsonc',
  'jsconfig.json': 'jsonc',
  '.eslintrc.json': 'jsonc',
  'devcontainer.json': 'jsonc',
  'settings.json': 'jsonc',
  'nginx.conf': 'nginx',
  '.vimrc': 'vim',
  'LICENSE': 'plaintext',
};

/// Name patterns: (suffix or prefix test, language), tried before extensions.
final _patterns = <(bool Function(String), String)>[
  ((n) => n.endsWith('.d.ts'), 'typescript'),
  ((n) => n.startsWith('.env.'), 'dotenv'),
  ((n) => n.startsWith('Dockerfile.') || n.endsWith('.Dockerfile'), 'dockerfile'),
  ((n) => n.startsWith('tsconfig.') && n.endsWith('.json'), 'jsonc'),
  ((n) => n.endsWith('.blade.php'), 'php'),
  ((n) => n.endsWith('.html.erb'), 'erb'),
  ((n) => n.startsWith('requirements') && n.endsWith('.txt'), 'plaintext'),
];

/// Shebang interpreters (`#!/usr/bin/env python3`, `#!/bin/bash`).
const _interpreters = <String, String>{
  'python': 'python',
  'python2': 'python',
  'python3': 'python',
  'node': 'javascript',
  'nodejs': 'javascript',
  'deno': 'typescript',
  'bun': 'javascript',
  'tsx': 'typescript',
  'ts-node': 'typescript',
  'sh': 'shellscript',
  'bash': 'shellscript',
  'zsh': 'shellscript',
  'ksh': 'shellscript',
  'dash': 'shellscript',
  'fish': 'shellscript',
  'ruby': 'ruby',
  'perl': 'perl',
  'php': 'php',
  'lua': 'lua',
  'Rscript': 'r',
  'julia': 'julia',
  'swift': 'swift',
  'elixir': 'elixir',
  'escript': 'erlang',
  'pwsh': 'powershell',
  'awk': 'awk',
  'tclsh': 'tcl',
  'make': 'makefile',
  'dart': 'dart',
  'groovy': 'groovy',
  'kotlin': 'kotlin',
  'scala': 'scala',
};

/// The registry: every language, and how a file finds its own.
class Languages {
  Languages._() {
    final seen = <String>{};
    for (final g in grammars) {
      final id = _lspIds[g.hljs] ?? g.hljs;
      if (!seen.add(id)) continue;
      _byHljs[g.hljs] = g;
      _list.add(Language(id, g.name, g.hljs));
    }
    for (final g in grammars) {
      _byHljs[g.hljs] = g;
    }
    for (final (id, name, hljs) in _extra) {
      if (seen.add(id)) _list.add(Language(id, name, hljs));
    }
    _list.sort((a, b) => a.name.toLowerCase().compareTo(b.name.toLowerCase()));
    for (final l in _list) {
      _byId[l.id] = l;
    }
    // Grammar aliases first, then the explicit table over them.
    for (final g in grammars) {
      final id = _lspIds[g.hljs] ?? g.hljs;
      for (final a in g.aliases) {
        _byExtension.putIfAbsent(a.toLowerCase(), () => id);
      }
      _byExtension.putIfAbsent(g.hljs, () => id);
    }
    for (final e in _extensions.entries) {
      for (final x in e.value) {
        _byExtension[x] = e.key;
      }
    }
  }

  static final Languages instance = Languages._();

  final _list = <Language>[];
  final _byId = <String, Language>{};
  final _byHljs = <String, Grammar>{};
  final _byExtension = <String, String>{};
  final _modes = <String, Mode>{};

  /// Plain text: no grammar.
  static const plain = Language('plaintext', 'Plain Text', null);

  /// Every language, by name.
  List<Language> get all => List.unmodifiable(_list);

  /// The language with this id (plain text when unknown).
  Language byId(String? id) => (id == null ? null : _byId[id]) ?? plain;

  /// The language of a file [path] (its last segment decides), or of its first line (a shebang,
  /// an XML declaration) when the name says nothing.
  Language detect(String path, {String? firstLine}) {
    final name = path.split('/').last;
    final exact = _fileNames[name];
    if (exact != null) return byId(exact);
    for (final (test, id) in _patterns) {
      if (test(name)) return byId(id);
    }
    final lower = name.toLowerCase();
    // The longest extension wins (`.tar.gz` style names): try each dot from the left.
    for (var i = lower.indexOf('.', 1); i >= 0; i = lower.indexOf('.', i + 1)) {
      final id = _byExtension[lower.substring(i + 1)];
      if (id != null) return byId(id);
    }
    final first = firstLine?.trimRight() ?? '';
    if (first.startsWith('#!')) {
      final words = first.substring(2).trim().split(RegExp(r'\s+'));
      var prog = words.isEmpty ? '' : words.first.split('/').last;
      if (prog == 'env') {
        prog = words.skip(1).firstWhere((w) => !w.startsWith('-'), orElse: () => '');
      }
      final id = _interpreters[prog] ?? _interpreters[prog.replaceAll(RegExp(r'[0-9.]+$'), '')];
      if (id != null) return byId(id);
    }
    if (first.startsWith('<?xml')) return byId('xml');
    if (first.startsWith('<!DOCTYPE html') || first.startsWith('<html')) return byId('html');
    return plain;
  }

  /// The language a Markdown code fence names (`py`, `rust`, `c++`), or null.
  Language? forFence(String fence) {
    final f = fence.trim().toLowerCase();
    if (f.isEmpty) return null;
    final l = _byId[f] ?? _byId[_byExtension[f] ?? ''];
    if (l != null) return l;
    return switch (f) {
      'shell' || 'console' || 'sh' || 'zsh' => _byId['shellscript'],
      'c++' => _byId['cpp'],
      'c#' => _byId['csharp'],
      'f#' => _byId['fsharp'],
      _ => null,
    };
  }

  /// The grammar that highlights [language] (built on first use), or null for plain text.
  Mode? mode(Language language) {
    final h = language.hljs;
    if (h == null) return null;
    return _modes[h] ??= _byHljs[h]!.mode();
  }

  /// How many grammars have been built (tests: opening one document builds one).
  int get builtGrammars => _modes.length;

  /// highlight.js name → grammar, for a [Highlight] that registers languages as used.
  Mode? modeByHljs(String hljs) {
    final g = _byHljs[hljs];
    return g == null ? null : (_modes[hljs] ??= g.mode());
  }
}
