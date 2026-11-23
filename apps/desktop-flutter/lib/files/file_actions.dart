// Making, renaming and trashing files and folders from the shell (audit AUD-022, protocol 0.6):
// the explorer's menu and keys, and File → New File… / New Folder… / Save As…. Every operation
// is the daemon's (`fs/createFile`, `fs/createDirectory`, `fs/rename`, `fs/trash`), inside the
// workspace; nothing is deleted outright (Move to Trash).

import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../app/app_state.dart';
import '../state/documents.dart';
import '../state/workspace.dart';
import '../ui/controls.dart';
import '../ui/overlays.dart';
import '../ui/text_input.dart';
import '../ui/theme.dart';

/// Asks for a name (or a workspace-relative path); null when cancelled or left empty.
Future<String?> askName(
  BuildContext context, {
  required String title,
  required String label,
  String initial = '',
  String? detail,
}) async {
  final v = await showAxDialog<String>(
    context: context,
    label: title,
    builder: (_) => _NameBox(title: title, label: label, initial: initial, detail: detail),
  );
  final name = v?.trim();
  return name == null || name.isEmpty ? null : name;
}

class _NameBox extends StatefulWidget {
  const _NameBox({required this.title, required this.label, required this.initial, this.detail});
  final String title, label, initial;
  final String? detail;

  @override
  State<_NameBox> createState() => _NameBoxState();
}

class _NameBoxState extends State<_NameBox> {
  late final _text = TextEditingController(text: widget.initial);

  @override
  void initState() {
    super.initState();
    // The name without its extension is selected, as file managers do.
    final dot = widget.initial.lastIndexOf('.');
    _text.selection = TextSelection(
      baseOffset: 0,
      extentOffset: dot > 0 ? dot : widget.initial.length,
    );
  }

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    void go() => Navigator.of(context).pop(_text.text);
    return SizedBox(
      width: 420,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(widget.title, style: t.type.label),
            if (widget.detail != null) ...[
              const SizedBox(height: 4),
              Text(widget.detail!, style: t.type.small),
            ],
            const SizedBox(height: 8),
            CallbackShortcuts(
              bindings: {
                const SingleActivator(LogicalKeyboardKey.enter): go,
                const SingleActivator(LogicalKeyboardKey.escape): () => Navigator.of(context).pop(),
              },
              child: AxTextInput(
                label: widget.label,
                controller: _text,
                autofocus: true,
                onSubmitted: (_) => go(),
              ),
            ),
            const SizedBox(height: 12),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                AxButton(
                  label: 'Cancel',
                  kind: ButtonKind.quiet,
                  onPressed: () => Navigator.of(context).pop(),
                ),
                const SizedBox(width: 8),
                AxButton(label: 'OK', kind: ButtonKind.primary, onPressed: go),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

String _join(String dir, String name) => dir.isEmpty ? name : '$dir/$name';

void _report(BuildContext context, String? error) {
  if (error != null && context.mounted) ToastHost.of(context).show(error, kind: ToastKind.error);
}

/// New File… in [dir] (the explorer's selected folder by default); opens it.
Future<void> newFile(BuildContext context, AppState app, {String? dir}) async {
  final ws = app.workspace;
  if (!ws.isOpen) return;
  final where = dir ?? ws.selectedFolder;
  final name = await askName(
    context,
    title: 'New File',
    label: 'File name',
    detail: where.isEmpty ? 'In ${ws.name}' : 'In $where',
  );
  if (name == null || !context.mounted) return;
  final path = _join(where, name);
  final err = await ws.createFile(path);
  if (!context.mounted) return;
  if (err != null) return _report(context, err);
  if (where.isNotEmpty) ws.setExpanded(where, true);
  ws.selection = path;
  unawaited(app.documents.open(path));
}

/// New Folder… in [dir] (the explorer's selected folder by default).
Future<void> newFolder(BuildContext context, AppState app, {String? dir}) async {
  final ws = app.workspace;
  if (!ws.isOpen) return;
  final where = dir ?? ws.selectedFolder;
  final name = await askName(
    context,
    title: 'New Folder',
    label: 'Folder name',
    detail: where.isEmpty ? 'In ${ws.name}' : 'In $where',
  );
  if (name == null || !context.mounted) return;
  final path = _join(where, name);
  final err = await ws.createFolder(path);
  if (!context.mounted) return;
  if (err != null) return _report(context, err);
  if (where.isNotEmpty) ws.setExpanded(where, true);
  ws.selection = path;
}

/// Rename… [path]: a new name in the same folder, or a path to move it within the workspace.
Future<void> renamePath(BuildContext context, AppState app, String path) async {
  final ws = app.workspace;
  if (!ws.isOpen || path.isEmpty) return;
  final parent = parentPath(path);
  final old = path.split('/').last;
  final name = await askName(
    context,
    title: 'Rename $old',
    label: 'New name',
    initial: old,
    detail: 'A name, or a path from the folder\'s root to move it (with a “/”).',
  );
  if (name == null || name == old || !context.mounted) return;
  final to = name.contains('/') ? name : _join(parent, name);
  final err = await ws.rename(path, to);
  if (!context.mounted) return;
  if (err != null) return _report(context, err);
  app.documents.renamed(path, to);
  ws.selection = to;
}

/// Move to Trash [path] (asks first; the system's trash, never a permanent delete).
Future<void> trashPath(BuildContext context, AppState app, String path) async {
  final ws = app.workspace;
  if (!ws.isOpen || path.isEmpty) return;
  final name = path.split('/').last;
  final open = app.documents.docs.where((d) => d.path == path || d.path.startsWith('$path/'));
  final unsaved = open.where((d) => d.dirty).length;
  final a = await showAxDialog<String>(
    context: context,
    label: 'Move to Trash',
    builder: (_) => AxQuestion(
      title: 'Move $name to the trash?',
      detail: unsaved == 0
          ? 'You can restore it from the system\'s trash.'
          : 'It has unsaved changes in $unsaved open ${unsaved == 1 ? 'file' : 'files'}; they are '
                'kept open, marked deleted. You can restore it from the system\'s trash.',
      actions: const [('trash', 'Move to Trash'), ('cancel', 'Cancel')],
      defaultAction: 'cancel',
    ),
  );
  if (a != 'trash' || !context.mounted) return;
  final err = await ws.trash(path);
  if (context.mounted) _report(context, err);
}

/// Save As… for [d]: a new path; the editor shows the new file, the old one is left on disk.
Future<void> saveAs(BuildContext context, AppState app, Doc d) async {
  if (d.readOnly) return;
  final name = await askName(
    context,
    title: 'Save As',
    label: 'Path in the folder',
    initial: d.path,
    detail: 'A new file; the current one is left as it is on disk.',
  );
  if (name == null || name == d.path || !context.mounted) return;
  final err = await app.documents.saveAs(d, name);
  if (context.mounted) _report(context, err);
}

/// The explorer's menu for [path] ('' is the folder's root).
List<MenuEntry> fileMenu(BuildContext context, AppState app, String path) {
  final ws = app.workspace;
  final n = ws.node(path);
  final isRoot = path.isEmpty;
  final dir = n == null || n.isDir ? path : parentPath(path);
  final abs = isRoot ? ws.root : '${ws.root}/$path';
  return [
    if (!isRoot) MenuEntry.heading(path.split('/').last),
    MenuEntry.item('New File…', () => unawaited(newFile(context, app, dir: dir))),
    MenuEntry.item('New Folder…', () => unawaited(newFolder(context, app, dir: dir))),
    if (!isRoot) ...[
      MenuEntry.item('Rename…', () => unawaited(renamePath(context, app, path)), shortcut: 'F2'),
      MenuEntry.item(
        'Move to Trash',
        () => unawaited(trashPath(context, app, path)),
        shortcut: 'Delete',
      ),
    ],
    MenuEntry.item('Copy Path', () => unawaited(Clipboard.setData(ClipboardData(text: abs)))),
    if (!isRoot)
      MenuEntry.item(
        'Copy Relative Path',
        () => unawaited(Clipboard.setData(ClipboardData(text: path))),
      ),
  ];
}
