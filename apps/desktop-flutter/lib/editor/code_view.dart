// The code view: re_editor (MIT) styled from the design tokens, with re_highlight colouring
// (ADR-0020 §3). The documents store owns the text; this widget presents one document's
// controller.

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:re_editor/re_editor.dart';

import '../state/git.dart';
import 'languages.dart';
import '../ui/theme.dart';
import '../ui/tokens.dart';

/// Grammars that embed others (highlight.js `subLanguage`): registered beside them.
const _subLanguages = <String, List<String>>{
  'xml': ['css', 'javascript'],
  'vue': ['xml', 'css', 'javascript', 'typescript'],
  'php': ['php-template', 'xml', 'css', 'javascript'],
  'php-template': ['php', 'xml'],
  'erb': ['ruby', 'xml', 'css', 'javascript'],
  'haml': ['ruby', 'javascript', 'css'],
  'handlebars': ['xml'],
  'twig': ['xml'],
  'django': ['xml'],
  'markdown': ['xml'],
  'mojolicious': ['perl', 'xml'],
  'vbscript-html': ['vbscript', 'xml'],
};

/// The editor's highlight theme for [language]: its grammar and the ones it embeds.
CodeHighlightTheme? highlightTheme(Language language, SyntaxColors y) {
  final reg = Languages.instance;
  final mode = reg.mode(language);
  if (mode == null) return null;
  final h = language.hljs!;
  return CodeHighlightTheme(
    languages: {
      h: CodeHighlightThemeMode(mode: mode),
      for (final sub in _subLanguages[h] ?? const <String>[])
        if (reg.modeByHljs(sub) case final m?) sub: CodeHighlightThemeMode(mode: m),
    },
    theme: syntaxTheme(y),
  );
}

/// highlight.js scopes → the syntax palette (the same roles the Monaco theme used).
Map<String, TextStyle> syntaxTheme(SyntaxColors y) {
  // No italics in code (the owner, 2026-11-22), comments included.
  TextStyle c(Color col) => TextStyle(color: col);
  return {
    'root': TextStyle(color: y.text),
    // Comments and documentation.
    'comment': c(y.comment),
    'quote': c(y.comment),
    'doctag': c(y.keyword),
    // Keywords and markup names.
    'keyword': c(y.keyword),
    'selector-tag': c(y.keyword),
    'section': c(y.keyword),
    'name': c(y.keyword),
    'variable.language': c(y.keyword),
    'deletion': c(y.keyword),
    // Types.
    'type': c(y.type),
    'title.class': c(y.type),
    'title.class.inherited': c(y.type),
    'built_in': c(y.type),
    'class': c(y.type),
    'selector-class': c(y.type),
    // Declared names: functions (`title.function`, called `title.function.invoke`) yellow,
    // classes (`title.class`) and other titles (C#'s namespaces and classes, Kotlin's) as types.
    // re_editor resolves the most specific part of a scope first (patched).
    'title': c(y.type),
    'title.function': c(y.function),
    // A title inside a function's scope is the function's name (C#'s, Kotlin's, Swift's
    // grammars); re_editor joins nested scopes with `-` and tries the whole first.
    'function-title': c(y.function),
    'function-title.function': c(y.function),
    'class-title': c(y.type),
    'title.function.invoke': c(y.function),
    'selector-id': c(y.function),
    // Variables, parameters, properties, keys and attributes.
    'variable': c(y.variable),
    'params': c(y.variable),
    'property': c(y.variable),
    'attr': c(y.variable),
    'attribute': c(y.variable),
    'selector-attr': c(y.variable),
    'template-variable': c(y.variable),
    // Strings.
    'string': c(y.string),
    'template-tag': c(y.string),
    'meta.string': c(y.string),
    'addition': c(y.string),
    'code': c(y.string),
    'subst': c(y.text),
    // Numbers and constants.
    'number': c(y.number),
    'literal': c(y.number),
    'symbol': c(y.number),
    'bullet': c(y.number),
    'variable.constant': c(y.number),
    // Preprocessor, annotations, escapes and patterns.
    'meta': c(y.macro),
    'meta.keyword': c(y.macro),
    'char.escape': c(y.macro),
    'regexp': c(y.macro),
    'selector-pseudo': c(y.macro),
    'link': c(y.macro),
    // Punctuation.
    'operator': c(y.punct),
    'punctuation': c(y.punct),
    'tag': c(y.punct),
    'emphasis': const TextStyle(fontStyle: FontStyle.italic),
    'strong': const TextStyle(fontWeight: FontWeight.w600),
  };
}

/// One document in the editor.
class CodeView extends StatelessWidget {
  const CodeView({
    super.key,
    required this.controller,
    required this.language,
    this.focusNode,
    this.scrollController,
    this.findController,
    this.fontSize = 13,
    this.fontFamily,
    this.wordWrap = false,
    this.readOnly = false,
    this.onChanged,
    this.findBuilder,
    this.onSave,
    this.overrides = const {},
    this.gutterKey,
    this.gitMarks,
  });

  final CodeLineEditingController controller;
  final Language language;
  final FocusNode? focusNode;
  final CodeScrollController? scrollController;
  final CodeFindController? findController;
  final double fontSize;
  final String? fontFamily;
  final bool wordWrap;
  final bool readOnly;
  final ValueChanged<CodeLineEditingValue>? onChanged;
  final CodeFindBuilder? findBuilder;

  /// Mod+S inside the editor (re_editor takes the key for its save intent).
  final VoidCallback? onSave;

  /// Extra key overrides (the completion popup takes the arrows, Enter, Tab and Escape).
  final Map<Type, Action<Intent>> overrides;

  /// Measures the gutter (popups are placed relative to the text).
  final GlobalKey? gutterKey;

  /// Lines added, modified, or after a deletion, against HEAD (a strip beside the numbers).
  final ValueListenable<Map<int, GutterMark>>? gitMarks;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final y = c.syntax;
    return CodeEditor(
      controller: controller,
      focusNode: focusNode,
      scrollController: scrollController,
      findController: findController,
      findBuilder: findBuilder,
      readOnly: readOnly,
      cursorBlink: t.motion.caretBlinks,
      cursorBlinkTimeout: t.motion.caretBlinkTimeout,
      wordWrap: wordWrap,
      onChanged: onChanged,
      padding: const EdgeInsets.only(left: 6, right: 12, top: 6, bottom: 64),
      shortcutsActivatorsBuilder: const _AxiomShortcuts(),
      shortcutOverrideActions: {
        ...overrides,
        if (onSave != null)
          CodeShortcutSaveIntent: CallbackAction<CodeShortcutSaveIntent>(
            onInvoke: (_) {
              onSave!();
              return null;
            },
          ),
      },
      style: CodeEditorStyle(
        fontSize: fontSize,
        fontFamily: (fontFamily == null || fontFamily!.isEmpty) ? AxiomType.mono : fontFamily,
        fontHeight: 1.5,
        textColor: y.text,
        backgroundColor: c.ground,
        selectionColor: c.selection,
        highlightColor: c.accentSoft,
        cursorColor: c.accent,
        cursorWidth: 2,
        cursorLineColor: y.currentLine,
        chunkIndicatorColor: y.lineNumber,
        codeTheme: highlightTheme(language, y),
      ),
      indicatorBuilder: (context, editingController, chunkController, notifier) => Row(
        key: gutterKey,
        children: [
          DefaultCodeLineNumber(
            controller: editingController,
            notifier: notifier,
            textStyle: TextStyle(
              fontFamily: AxiomType.mono,
              fontSize: fontSize * 0.92,
              color: y.lineNumber,
            ),
            focusedTextStyle: TextStyle(
              fontFamily: AxiomType.mono,
              fontSize: fontSize * 0.92,
              color: y.lineNumberActive,
            ),
          ),
          const SizedBox(width: 4),
          if (gitMarks != null)
            GitMarkStrip(notifier: notifier, marks: gitMarks!)
          else
            const SizedBox(width: 3),
          const SizedBox(width: 6),
        ],
      ),
    );
  }
}

/// re_editor's keys without the ones the shell owns: Mod+L is the AI dock (as in the Tauri
/// shell), not "select line".
class _AxiomShortcuts extends CodeShortcutsActivatorsBuilder {
  const _AxiomShortcuts();

  @override
  List<ShortcutActivator>? build(CodeShortcutType type) => switch (type) {
    CodeShortcutType.lineSelect => null,
    // Ctrl+T is "workspace symbols" off macOS (as in VS Code, M5.5-EDIT-002); macOS keeps
    // Control+T for transposing (its symbols key is Cmd+T).
    CodeShortcutType.transposeCharacters when defaultTargetPlatform != TargetPlatform.macOS => null,
    _ => const DefaultCodeShortcutsActivatorsBuilder().build(type),
  };
}

/// The gutter's Git marks: a bar per added (green) or modified (brass) line, a wedge where lines
/// were deleted. The file's state is also on its tab as text.
class GitMarkStrip extends StatelessWidget {
  const GitMarkStrip({super.key, required this.notifier, required this.marks});
  final CodeIndicatorValueNotifier notifier;
  final ValueListenable<Map<int, GutterMark>> marks;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return ExcludeSemantics(
      child: SizedBox(
        width: 3,
        // Repainted, not rebuilt: the editor updates [notifier] during its layout.
        child: CustomPaint(
          painter: _MarksPainter(
            notifier,
            marks,
            added: c.ok,
            modified: c.accent,
            deleted: c.danger,
          ),
        ),
      ),
    );
  }
}

class _MarksPainter extends CustomPainter {
  _MarksPainter(
    this.notifier,
    this.marksOf, {
    required this.added,
    required this.modified,
    required this.deleted,
  }) : super(repaint: Listenable.merge([notifier, marksOf]));
  final CodeIndicatorValueNotifier notifier;
  final ValueListenable<Map<int, GutterMark>> marksOf;
  final Color added, modified, deleted;

  @override
  void paint(Canvas canvas, Size size) {
    final marks = marksOf.value;
    if (marks.isEmpty) return;
    for (final p in notifier.value?.paragraphs ?? const <CodeLineRenderParagraph>[]) {
      final m = marks[p.index];
      if (m == null) continue;
      final paint = Paint()
        ..color = switch (m) {
          GutterMark.added => added,
          GutterMark.modified => modified,
          GutterMark.deleted => deleted,
        };
      if (m == GutterMark.deleted) {
        final y = p.offset.dy;
        canvas.drawPath(
          Path()
            ..moveTo(0, y - 3)
            ..lineTo(size.width + 2, y)
            ..lineTo(0, y + 3)
            ..close(),
          paint,
        );
      } else {
        canvas.drawRect(Rect.fromLTWH(0, p.offset.dy, size.width, p.height), paint);
      }
    }
  }

  @override
  bool shouldRepaint(_MarksPainter old) =>
      old.notifier != notifier || old.marksOf != marksOf || old.added != added;
}
