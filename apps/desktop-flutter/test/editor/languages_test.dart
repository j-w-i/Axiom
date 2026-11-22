// The language registry (M5.5-LANG-001, ADR-0021 §1): recognition by extension, name, pattern,
// shebang and first line; every grammar builds; grammars are built only when used.

import 'package:axiom_desktop/editor/languages.dart';
import 'package:axiom_desktop/editor/languages.g.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:re_highlight/re_highlight.dart';

void main() {
  final reg = Languages.instance;

  test('opening one file builds one grammar, not all of them', () {
    final before = reg.builtGrammars;
    final rust = reg.detect('src/main.rs');
    reg.mode(rust);
    expect(reg.builtGrammars, lessThanOrEqualTo(before + 1));
    expect(reg.builtGrammars, lessThan(grammarCount));
  });

  test('files are recognised as VS Code recognises them', () {
    const cases = {
      'src/main.rs': 'rust',
      'a/b/App.tsx': 'typescriptreact',
      'index.ts': 'typescript',
      'types.d.ts': 'typescript',
      'vite.config.mts': 'typescript',
      'app.js': 'javascript',
      'Button.jsx': 'javascriptreact',
      'server.cjs': 'javascript',
      'package.json': 'json',
      'tsconfig.json': 'jsonc',
      'tsconfig.base.json': 'jsonc',
      'settings.jsonc': 'jsonc',
      'main.py': 'python',
      'stubs.pyi': 'python',
      'main.go': 'go',
      'go.mod': 'go',
      'lib.c': 'c',
      'lib.h': 'c',
      'lib.cpp': 'cpp',
      'lib.hpp': 'cpp',
      'kernel.cu': 'cpp',
      'Program.cs': 'csharp',
      'Main.java': 'java',
      'Main.kt': 'kotlin',
      'build.gradle.kts': 'kotlin',
      'build.gradle': 'gradle',
      'App.swift': 'swift',
      'main.dart': 'dart',
      'app.rb': 'ruby',
      'Gemfile': 'ruby',
      'index.php': 'php',
      'view.blade.php': 'php',
      'init.lua': 'lua',
      'script.sh': 'shellscript',
      '.bashrc': 'shellscript',
      '.zshrc': 'shellscript',
      'deploy.ps1': 'powershell',
      'build.bat': 'bat',
      'index.html': 'html',
      'page.htm': 'html',
      'icon.svg': 'xml',
      'pom.xml': 'xml',
      'App.csproj': 'xml',
      'style.css': 'css',
      'style.scss': 'scss',
      'theme.less': 'less',
      'README.md': 'markdown',
      'post.mdx': 'mdx',
      'config.yaml': 'yaml',
      'compose.yml': 'yaml',
      'Cargo.toml': 'toml',
      'Cargo.lock': 'toml',
      'pyproject.toml': 'toml',
      'setup.cfg': 'ini',
      'schema.sql': 'sql',
      'schema.graphql': 'graphql',
      'api.proto': 'protobuf',
      'Dockerfile': 'dockerfile',
      'Dockerfile.dev': 'dockerfile',
      'app.dockerfile': 'dockerfile',
      'Makefile': 'makefile',
      'CMakeLists.txt': 'cmake',
      'tools.cmake': 'cmake',
      'fix.patch': 'diff',
      'paper.tex': 'latex',
      'App.vue': 'vue',
      'App.svelte': 'svelte',
      'main.zig': 'zig',
      'main.tf': 'terraform',
      'default.nix': 'nix',
      'shader.frag': 'glsl',
      'Main.hs': 'haskell',
      'mix.exs': 'elixir',
      'main.ml': 'ocaml',
      'main.scala': 'scala',
      'core.clj': 'clojure',
      'main.jl': 'julia',
      'analysis.r': 'r',
      '.gitignore': 'ignore',
      '.env': 'dotenv',
      '.env.local': 'dotenv',
      'notes.txt': 'plaintext',
      'unknownfile.zzz': 'plaintext',
      'LICENSE': 'plaintext',
      'Jenkinsfile': 'groovy',
      'Main.qml': 'qml',
    };
    final wrong = <String>[];
    cases.forEach((path, id) {
      final got = reg.detect(path).id;
      if (got != id) wrong.add('$path: $got (want $id)');
    });
    expect(wrong, isEmpty);
    expect(cases.length, greaterThanOrEqualTo(60));
  });

  test('a shebang or the first line decides when the name says nothing', () {
    expect(reg.detect('bin/tool', firstLine: '#!/usr/bin/env python3').id, 'python');
    expect(
      reg.detect('bin/tool', firstLine: '#!/usr/bin/env -S node --experimental').id,
      'javascript',
    );
    expect(reg.detect('bin/run', firstLine: '#!/bin/bash').id, 'shellscript');
    expect(reg.detect('bin/run', firstLine: '#!/usr/bin/ruby -w').id, 'ruby');
    expect(reg.detect('data', firstLine: '<?xml version="1.0"?>').id, 'xml');
    expect(reg.detect('page', firstLine: '<!DOCTYPE html>').id, 'html');
    // The name wins over the first line.
    expect(reg.detect('a.rs', firstLine: '#!/usr/bin/env python3').id, 'rust');
  });

  test('every registered grammar builds and highlights a line', () {
    final hl = Highlight();
    final failures = <String>[];
    for (final g in grammars) {
      try {
        hl.registerLanguage(g.hljs, g.mode());
      } on Object catch (e) {
        failures.add('${g.hljs}: $e');
      }
    }
    for (final g in grammars) {
      try {
        hl.highlight(code: 'x = 1 // comment\n', language: g.hljs);
      } on Object catch (e) {
        failures.add('${g.hljs}: $e');
      }
    }
    expect(failures, isEmpty);
    expect(grammars.length, grammarCount);
  });

  test('every language has a unique id and a name; fences resolve', () {
    final ids = reg.all.map((l) => l.id).toList();
    expect(ids.toSet().length, ids.length);
    expect(reg.all.length, greaterThan(190));
    expect(reg.forFence('py')?.id, 'python');
    expect(reg.forFence('rust')?.id, 'rust');
    expect(reg.forFence('c++')?.id, 'cpp');
    expect(reg.forFence('sh')?.id, 'shellscript');
    expect(reg.forFence('tsx')?.id, 'typescriptreact');
    expect(reg.forFence('nosuchlanguage'), isNull);
  });
}
