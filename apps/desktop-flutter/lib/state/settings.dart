import 'package:flutter/foundation.dart';

import '../daemon/client.dart';
import '../daemon/protocol.g.dart';
import '../ui/tokens.dart';

enum ThemeSetting { system, light, dark }

/// The daemon's settings the shell presents (ipc.md §settings), plus `appearance.motion`, a
/// shell key the daemon stores unchanged.
class SettingsStore extends ChangeNotifier {
  SettingsStore(this.client);
  final DaemonClient client;

  bool loaded = false;
  ThemeSetting theme = ThemeSetting.system;
  MotionMode motion = MotionMode.standard;
  int editorFontSize = 13;
  String editorFontFamily = '';
  bool editorWordWrap = false;

  /// M5.5 (ADR-0021 §5, §8).
  bool lspAutoInstall = false;
  bool editorInlayHints = true;
  bool editorFormatOnSave = false;
  bool reopenLast = true;
  String? lastRoot;
  String error = '';

  static const _keys = [
    'appearance.theme',
    'appearance.motion',
    'editor.fontSize',
    'editor.fontFamily',
    'editor.wordWrap',
    'lsp.autoInstall',
    'editor.inlayHints',
    'editor.formatOnSave',
    'workspace.reopenLast',
    'workspace.lastRoot',
  ];

  Future<void> load() async {
    try {
      final r = await client.call(Methods.settingsGet, const GetParams(keys: _keys));
      _apply(r.values_);
      loaded = true;
      error = '';
    } on DaemonException catch (e) {
      error = 'Settings could not be read: ${e.message}';
    }
    notifyListeners();
  }

  void _apply(Map<String, Object?> v) {
    theme = switch (v['appearance.theme']) {
      'light' => ThemeSetting.light,
      'dark' => ThemeSetting.dark,
      _ => ThemeSetting.system,
    };
    motion = switch (v['appearance.motion']) {
      'reduced' => MotionMode.reduced,
      'off' => MotionMode.off,
      _ => MotionMode.standard,
    };
    final size = v['editor.fontSize'];
    if (size is num) editorFontSize = size.toInt().clamp(8, 40);
    if (v['editor.fontFamily'] is String) editorFontFamily = v['editor.fontFamily']! as String;
    if (v['editor.wordWrap'] is bool) editorWordWrap = v['editor.wordWrap']! as bool;
    if (v['lsp.autoInstall'] is bool) lspAutoInstall = v['lsp.autoInstall']! as bool;
    if (v['editor.inlayHints'] is bool) editorInlayHints = v['editor.inlayHints']! as bool;
    if (v['editor.formatOnSave'] is bool) editorFormatOnSave = v['editor.formatOnSave']! as bool;
    if (v['workspace.reopenLast'] is bool) reopenLast = v['workspace.reopenLast']! as bool;
    lastRoot = v['workspace.lastRoot'] as String?;
  }

  /// Sets one key on the daemon; the store follows once it is accepted.
  Future<void> setValue(String key, Object? value) async {
    try {
      await client.call(Methods.settingsSet, SetParams(values_: {key: value}));
      _apply({..._snapshot(), key: value});
      error = '';
    } on DaemonException catch (e) {
      error = e.message;
    }
    notifyListeners();
  }

  Map<String, Object?> _snapshot() => {
    'appearance.theme': theme.name,
    'appearance.motion': motion.name,
    'editor.fontSize': editorFontSize,
    'editor.fontFamily': editorFontFamily,
    'editor.wordWrap': editorWordWrap,
    'lsp.autoInstall': lspAutoInstall,
    'editor.inlayHints': editorInlayHints,
    'editor.formatOnSave': editorFormatOnSave,
    'workspace.reopenLast': reopenLast,
    'workspace.lastRoot': lastRoot,
  };
}
