import 'dart:async';
import 'dart:isolate';
// The editor area (FL-EDIT-001): tabs, breadcrumbs, the banner, the code view for the active
// document, the right group, and the find bar. Each open document keeps its own editor state;
// switching tabs shows the one already laid out (no model swap, unlike Monaco's 17 ms).

import 'dart:ui' show SemanticsRole;

import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:re_editor/re_editor.dart';

import '../app/app_state.dart';
import '../state/documents.dart';
import '../state/git.dart';
import '../ui/controls.dart';
import '../ui/icon_data.dart';
import '../ui/glyph.dart';
import '../ui/icons.dart';
import '../ui/overlays.dart';
import '../ui/pressable.dart';
import '../ui/splitter.dart';
import '../ui/text_input.dart';
import '../ui/theme.dart';
import '../ui/tokens.dart';
import 'code_view.dart';
import 'lsp/lsp_editor.dart';
import 'lsp/lsp_view.dart';

class EditorArea extends StatefulWidget {
  const EditorArea({super.key});

  @override
  State<EditorArea> createState() => EditorAreaState();
}

class EditorAreaState extends State<EditorArea> {
  late final AppState app = AppScope.of(context);
  double? _leftWidth;

  /// One focus node per document per group, so focus can be moved to "the editor".
  final _focus = <String, FocusNode>{};
  FocusNode focusFor(String key) =>
      _focus.putIfAbsent(key, () => FocusNode(debugLabel: 'editor $key'));

  /// The group (1 or 2) whose editor has keyboard focus, or null. By node, not by its debug
  /// label, which release builds do not keep (AUD-085).
  int? focusedGroup() {
    for (final e in _focus.entries) {
      if (e.value.hasFocus) return int.tryParse(e.key.substring(0, 1));
    }
    return null;
  }

  /// Focuses the left group's editor (Mod+1) or the right one's (Mod+2).
  void focusGroup(int group) {
    final d = app.documents;
    final path = group == 2 ? d.split?.active : d.activePath;
    if (path == null || path.isEmpty) return;
    focusFor('$group:$path').requestFocus();
  }

  @override
  void dispose() {
    for (final n in _focus.values) {
      n.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final docs = app.documents;
    return ListenableBuilder(
      listenable: docs,
      builder: (context, _) {
        _focus.removeWhere((k, n) {
          final path = k.substring(2);
          if (docs.doc(path) != null) return false;
          n.dispose();
          return true;
        });
        final split = docs.split;
        final left = _Group(group: 1, area: this);
        if (split == null) return left;
        return LayoutBuilder(
          builder: (context, box) {
            // Each group keeps 240 px when there is room; in a narrow editor column (a small or
            // tiled window, the side bar and the AI dock open) the bounds shrink with it instead of
            // crossing (AUD-004: clamp(240, width - 240) threw under 480 px).
            final (lo, hi) = splitBounds(box.maxWidth);
            final w = (_leftWidth ?? box.maxWidth / 2).clamp(lo, hi);
            return Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                SizedBox(width: w, child: left),
                AxSplitter(
                  label: 'Resize the editor groups',
                  value: w,
                  min: lo,
                  max: hi,
                  defaultValue: box.maxWidth / 2,
                  onChanged: (v) => setState(() => _leftWidth = v),
                ),
                Expanded(child: _Group(group: 2, area: this)),
              ],
            );
          },
        );
      },
    );
  }
}

/// The left group's width bounds in an editor column [width] wide: 240 px for each group when
/// both fit, else an even share; never crossing (min ≤ max).
(double, double) splitBounds(double width) {
  final w = width.isFinite && width > 0 ? width : 0.0;
  final pane = w >= 480 ? 240.0 : w / 2;
  return (pane, w - pane);
}

/// One editor group: its tabs and the code view of its active document.
class _Group extends StatelessWidget {
  const _Group({required this.group, required this.area});
  final int group;
  final EditorAreaState area;

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final docs = app.documents;
    final t = context.axiom;
    final c = t.colors;
    final paths = group == 1
        ? [for (final d in docs.docs) d.path]
        : (docs.split?.paths ?? const <String>[]);
    final active = group == 1 ? docs.activePath : (docs.split?.active ?? '');
    final doc = docs.doc(active);
    return ColoredBox(
      color: c.ground,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _TabStrip(group: group, paths: paths, active: active),
          if (doc != null) _Breadcrumbs(path: doc.path),
          if (group == 1 && docs.banner.isNotEmpty)
            _Banner(text: docs.banner, onDismiss: docs.dismissBanner),
          if (doc != null) _TrustLine(doc: doc),
          if (doc != null) _ServerOfferLine(doc: doc),
          Expanded(
            child: doc == null
                ? const SizedBox.shrink()
                : _DocView(
                    key: ValueKey('$group:${doc.path}'),
                    doc: doc,
                    group: group,
                    focusNode: area.focusFor('$group:${doc.path}'),
                  ),
          ),
        ],
      ),
    );
  }
}

class _TabStrip extends StatefulWidget {
  const _TabStrip({required this.group, required this.paths, required this.active});
  final int group;
  final List<String> paths;
  final String active;

  @override
  State<_TabStrip> createState() => _TabStripState();
}

/// The active tab is scrolled into view when it changes; a vertical wheel scrolls the strip; when
/// the tabs do not fit, a menu at the end lists every open tab (AUD-017).
class _TabStripState extends State<_TabStrip> {
  final _scroll = ScrollController();
  final _activeKey = GlobalKey();
  bool _overflows = false;
  (Size, double)? _view;

  @override
  void initState() {
    super.initState();
    _scroll.addListener(_measure);
    WidgetsBinding.instance.addPostFrameCallback((_) => _reveal());
  }

  @override
  void didUpdateWidget(_TabStrip old) {
    super.didUpdateWidget(old);
    if (old.active != widget.active || old.paths.length != widget.paths.length) {
      WidgetsBinding.instance.addPostFrameCallback((_) => _reveal());
    }
  }

  @override
  void dispose() {
    _scroll.dispose();
    super.dispose();
  }

  void _measure() {
    if (!mounted || !_scroll.hasClients) return;
    final over = _scroll.position.maxScrollExtent > 0;
    if (over != _overflows) setState(() => _overflows = over);
  }

  void _reveal() {
    if (!mounted) return;
    _measure();
    final ctx = _activeKey.currentContext;
    if (ctx == null) return;
    Scrollable.ensureVisible(
      ctx,
      duration: context.axiom.motion.glides ? const Duration(milliseconds: 120) : Duration.zero,
      alignmentPolicy: ScrollPositionAlignmentPolicy.keepVisibleAtEnd,
    );
    Scrollable.ensureVisible(
      ctx,
      alignmentPolicy: ScrollPositionAlignmentPolicy.keepVisibleAtStart,
    );
  }

  void _wheel(PointerSignalEvent e) {
    if (e is! PointerScrollEvent || !_scroll.hasClients) return;
    final d = e.scrollDelta.dy.abs() > e.scrollDelta.dx.abs() ? e.scrollDelta.dy : e.scrollDelta.dx;
    final p = _scroll.position;
    _scroll.jumpTo((p.pixels + d).clamp(p.minScrollExtent, p.maxScrollExtent));
  }

  @override
  Widget build(BuildContext context) {
    final group = widget.group;
    final paths = widget.paths;
    final active = widget.active;
    // A window resize or a text-scale change moves the tabs: the active one is shown again.
    final view = (MediaQuery.sizeOf(context), MediaQuery.textScalerOf(context).scale(100));
    if (view != _view) {
      _view = view;
      WidgetsBinding.instance.addPostFrameCallback((_) => _reveal());
    }
    final app = AppScope.of(context);
    final docs = app.documents;
    final t = context.axiom;
    final c = t.colors;
    WidgetsBinding.instance.addPostFrameCallback((_) => _measure());
    return SizedBox(
      height: context.scaled(Metrics.tabHeight, fontSize: 12, lineHeight: 16),
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: c.bar,
          border: Border(bottom: BorderSide(color: c.groove)),
        ),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Expanded(
              child: Listener(
                onPointerSignal: _wheel,
                // The tab bar's role sits inside the scroll view, so its children are the tabs.
                child: SingleChildScrollView(
                  controller: _scroll,
                  scrollDirection: Axis.horizontal,
                  child: Semantics(
                    role: SemanticsRole.tabBar,
                    label: group == 1 ? 'Open documents' : 'Right editor group',
                    container: true,
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        for (final (i, p) in paths.indexed)
                          // A tab is dragged onto another to take its place (the left group's order;
                          // Ctrl+Shift+PgUp/PgDn by keyboard).
                          DragTarget<String>(
                            onWillAcceptWithDetails: (d) => group == 1 && d.data != p,
                            onAcceptWithDetails: (d) => docs.moveTab(d.data, i),
                            builder: (context, over, _) {
                              final tab = _Tab(
                                key: p == active ? _activeKey : null,
                                doc: docs.doc(p)!,
                                selected: p == active,
                                dropTarget: over.isNotEmpty,
                                onSelect: () =>
                                    group == 1 ? docs.activate(p) : docs.activateSplit(p),
                                onClose: () => group == 1 ? docs.close(p) : docs.closeInSplit(p),
                                closeLabel: group == 1
                                    ? 'Close ${titleOf(p)}'
                                    : 'Close ${titleOf(p)} in the right group',
                              );
                              if (group != 1) return tab;
                              return Draggable<String>(
                                data: p,
                                axis: Axis.horizontal,
                                feedback: Opacity(
                                  opacity: 0.85,
                                  child: SizedBox(height: 32, child: tab),
                                ),
                                childWhenDragging: Opacity(opacity: 0.4, child: tab),
                                child: tab,
                              );
                            },
                          ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
            if (_overflows)
              AxMenu(
                label: group == 1 ? 'All open documents' : 'All documents in the right group',
                hint: 'Every open tab',
                entries: () => [
                  for (final p in paths)
                    MenuEntry.item(
                      '${titleOf(p)}${docs.doc(p)?.dirty ?? false ? ' •' : ''}',
                      () => group == 1 ? docs.activate(p) : docs.activateSplit(p),
                    ),
                ],
                trigger: (context, st, open) => FocusRing(
                  visible: st.focused,
                  child: SizedBox(
                    width: 30,
                    child: Center(
                      child: AxiomIcon(
                        AxiomIcons.more,
                        size: 14,
                        color: open || st.hovered ? c.ink : c.ink3,
                      ),
                    ),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _Tab extends StatelessWidget {
  const _Tab({
    super.key,
    required this.doc,
    required this.selected,
    this.dropTarget = false,
    required this.onSelect,
    required this.onClose,
    required this.closeLabel,
  });
  final Doc doc;
  final bool selected;

  /// A dragged tab is over this one: it would go here.
  final bool dropTarget;
  final VoidCallback onSelect;
  final VoidCallback onClose;
  final String closeLabel;

  @override
  Widget build(BuildContext context) {
    final git = AppScope.of(context).git;
    return ListenableBuilder(
      listenable: git,
      builder: (context, _) => _tab(context, git.marks[doc.path]),
    );
  }

  Widget _tab(BuildContext context, GitMark? mark) {
    final t = context.axiom;
    final c = t.colors;
    final name = [
      doc.title,
      if (mark != null) mark.word,
      if (doc.dirty) 'unsaved',
      if (doc.changedOnDisk) 'changed on disk',
      if (doc.deletedOnDisk) 'deleted on disk',
      if (doc.detached) 'waiting for the backend',
    ].join(', ');
    final tab = Listener(
      onPointerDown: (e) {
        if (e.buttons == kMiddleMouseButton) onClose();
      },
      child: Pressable(
        label: name,
        hint: '${doc.path} — Ctrl+Tab',
        role: PressRole.tab,
        selected: selected,
        onPressed: onSelect,
        excludeChildSemantics: false,
        builder: (context, s) {
          // Tauri's tab: the file's chip, its name, then the close slot (shown on the active tab,
          // under the pointer and with focus; a dirty tab shows its dot there instead).
          final showClose = selected || s.hovered || s.focused;
          final content = Padding(
            padding: const EdgeInsets.only(left: Metrics.s5, right: 3),
            child: Row(
              children: [
                FileGlyph(doc.title),
                const SizedBox(width: 7),
                // A long name ellipsises instead of widening the tab without bound (AUD-017); a
                // file deleted on disk is struck through (AUD-019).
                ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 220),
                  child: Text(
                    doc.title,
                    maxLines: 1,
                    softWrap: false,
                    overflow: TextOverflow.ellipsis,
                    style: t.type.body.copyWith(
                      color: doc.deletedOnDisk
                          ? c.danger
                          : gitInk(c, mark?.kind) ?? (selected || s.hovered ? c.ink : c.ink2),
                      decoration: doc.deletedOnDisk ? TextDecoration.lineThrough : null,
                    ),
                  ),
                ),
                if (mark != null) ...[
                  const SizedBox(width: 5),
                  ExcludeSemantics(
                    child: Text(
                      mark.letter,
                      style: t.type.codeSmall.copyWith(color: gitInk(c, mark.kind)),
                    ),
                  ),
                ],
                const SizedBox(width: 4),
                SizedBox.square(
                  dimension: 24,
                  child: doc.dirty && !s.hovered
                      ? Center(
                          child: ExcludeSemantics(
                            child: Container(
                              width: 7,
                              height: 7,
                              decoration: BoxDecoration(
                                color: doc.changedOnDisk || doc.deletedOnDisk ? c.warn : c.ink2,
                                shape: BoxShape.circle,
                              ),
                            ),
                          ),
                        )
                      // A hidden close button is not a Tab stop (AUD-021).
                      : ExcludeFocus(
                          excluding: !showClose,
                          child: Opacity(
                            opacity: showClose ? 1 : 0,
                            child: AxIconButton(
                              icon: AxiomIcons.close,
                              label: closeLabel,
                              size: 24,
                              color: c.ink3,
                              onPressed: onClose,
                            ),
                          ),
                        ),
                ),
              ],
            ),
          );
          // The active tab is a raised key in the editor's tone with side seams and no seam
          // below; the others sit in the strip with a seam on their right. The brass lit edge
          // runs along the active tab's top.
          Widget tab = DecoratedBox(
            decoration: BoxDecoration(
              color: selected ? c.ground : (s.hovered ? c.hover : null),
              border: selected
                  ? Border(
                      left: BorderSide(color: c.groove),
                      right: BorderSide(color: c.groove),
                    )
                  : Border(
                      right: BorderSide(color: c.hairline),
                      bottom: BorderSide(color: c.groove),
                    ),
            ),
            child: content,
          );
          if (selected) {
            tab = Stack(
              fit: StackFit.passthrough,
              children: [
                tab,
                Positioned(
                  left: 0,
                  right: 0,
                  top: 0,
                  height: 9,
                  child: IgnorePointer(
                    child: DecoratedBox(
                      decoration: BoxDecoration(
                        gradient: LinearGradient(
                          begin: Alignment.topCenter,
                          end: Alignment.bottomCenter,
                          stops: const [0, 1.5 / 9, 1.5 / 9, 1],
                          colors: [
                            c.accent,
                            c.accent,
                            c.accentSoft,
                            c.accentSoft.withValues(alpha: 0),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
              ],
            );
          }
          return FocusRing(visible: s.focused, child: tab);
        },
      ),
    );
    if (!dropTarget) return tab;
    return DecoratedBox(
      position: DecorationPosition.foreground,
      decoration: BoxDecoration(
        border: Border(left: BorderSide(color: c.accent, width: 2)),
      ),
      child: tab,
    );
  }
}

class _Breadcrumbs extends StatelessWidget {
  const _Breadcrumbs({required this.path});
  final String path;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final parts = isExternal(path)
        ? Uri.parse(path).pathSegments.where((s) => s.isNotEmpty).toList()
        : path.split('/');
    return SizedBox(
      height: 24,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12),
        child: Semantics(
          label: 'Path: ${parts.join(' › ')}',
          excludeSemantics: true,
          // A path longer than the bar (an SDK's file, a deep folder) keeps its end in view, the
          // file's name, and its start is cut, as VS Code does: never an overflow.
          child: LayoutBuilder(
            builder: (context, box) {
              final row = Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  for (var i = 0; i < parts.length; i++) ...[
                    if (i > 0)
                      Padding(
                        padding: const EdgeInsets.symmetric(horizontal: 4),
                        child: AxiomIcon(AxiomIcons.chevron, size: 12, color: t.colors.ink3),
                      ),
                    Text(
                      parts[i],
                      maxLines: 1,
                      softWrap: false,
                      style: t.type.small.copyWith(
                        color: i == parts.length - 1 ? t.colors.ink : t.colors.ink2,
                      ),
                    ),
                  ],
                ],
              );
              var width = (parts.length - 1) * 20.0;
              for (final part in parts) {
                width += (TextPainter(
                  text: TextSpan(text: part, style: t.type.small),
                  textDirection: TextDirection.ltr,
                  maxLines: 1,
                  textScaler: MediaQuery.textScalerOf(context),
                )..layout()).width;
              }
              if (width <= box.maxWidth) return Align(alignment: Alignment.centerLeft, child: row);
              return ClipRect(
                child: OverflowBox(
                  maxWidth: double.infinity,
                  alignment: Alignment.centerRight,
                  child: row,
                ),
              );
            },
          ),
        ),
      ),
    );
  }
}

class _Banner extends StatelessWidget {
  const _Banner({required this.text, required this.onDismiss});
  final String text;
  final VoidCallback onDismiss;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    return Semantics(
      liveRegion: true,
      child: ColoredBox(
        color: t.colors.dangerSoft,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
          child: Row(
            children: [
              AxiomIcon(AxiomIcons.warning, size: 14, color: t.colors.dangerText),
              const SizedBox(width: 8),
              Expanded(
                child: Text(text, style: t.type.small.copyWith(color: t.colors.dangerText)),
              ),
              AxButton(label: 'Dismiss', kind: ButtonKind.quiet, onPressed: onDismiss),
            ],
          ),
        ),
      ),
    );
  }
}

/// The folder is not trusted, so the document's language server does not run (or runs without
/// the parts that run the folder's code): what the daemon says, and Trust Folder (AUD-008).
class _TrustLine extends StatelessWidget {
  const _TrustLine({required this.doc});
  final Doc doc;

  static final _dismissed = <String>{};

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    return ListenableBuilder(
      listenable: app.lsp,
      builder: (context, _) {
        final server = app.lsp.serverFor(lspLanguageId(doc));
        final detail = server?.detail ?? '';
        final waits = detail.startsWith('Trust this folder to enable');
        final restricted = detail.startsWith('Restricted until you trust');
        if (server == null || (!waits && !restricted)) return const SizedBox.shrink();
        if (_dismissed.contains('${app.workspace.root}\u0000${server.serverId}')) {
          return const SizedBox.shrink();
        }
        final t = context.axiom;
        final c = t.colors;
        return Semantics(
          liveRegion: true,
          container: true,
          child: DecoratedBox(
            decoration: BoxDecoration(
              color: c.panel,
              border: Border(bottom: BorderSide(color: c.hairline)),
            ),
            child: Padding(
              padding: const EdgeInsets.fromLTRB(12, 4, 8, 4),
              child: Row(
                children: [
                  AxiomIcon(AxiomIcons.warning, size: 14, color: c.accentText),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(detail, style: t.type.small.copyWith(color: c.ink)),
                  ),
                  AxButton(
                    label: 'Trust Folder',
                    hint: 'Trust ${app.workspace.name}: its language servers may run its code',
                    onPressed: () async {
                      await app.environments.trustWorkspace(true);
                      await app.lsp.load();
                    },
                  ),
                  const SizedBox(width: Metrics.s2),
                  AxButton(
                    label: 'Not now',
                    kind: ButtonKind.quiet,
                    onPressed: () {
                      _dismissed.add('${app.workspace.root}\u0000${server.serverId}');
                      (context as Element).markNeedsBuild();
                    },
                  ),
                ],
              ),
            ),
          ),
        );
      },
    );
  }
}

/// "Python has a language server: basedpyright. Install… Not now" (ADR-0021 §5), then its install's
/// progress or failure; the prerequisite first when the plan needs one.
class _ServerOfferLine extends StatelessWidget {
  const _ServerOfferLine({required this.doc});
  final Doc doc;

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final si = app.serverInstalls;
    return ListenableBuilder(
      listenable: si,
      builder: (context, _) {
        final offer = si.offerFor(doc);
        if (offer == null) return const SizedBox.shrink();
        final t = context.axiom;
        final c = t.colors;
        final env = app.environments;
        final pid = offer.providerId;
        final pre = si.prerequisiteOf(pid);
        final running = env.installs[pid] ?? (pre == null ? null : env.installs[pre]);
        final String text;
        final List<Widget> actions;
        if (running != null) {
          final who = env.installs.containsKey(pid) ? offer.providerName : si.nameOf(pre!);
          text = 'Installing $who… ${running.progress?.label ?? ''}'.trim();
          actions = const [];
        } else if (pre != null) {
          text = '${offer.providerName} needs ${si.nameOf(pre)} first.';
          actions = [
            AxButton(
              label: 'Install ${si.nameOf(pre)}…',
              onPressed: () => unawaited(si.install(pre)),
            ),
          ];
        } else if (env.failures[pid] case final why?) {
          text = 'Could not install ${offer.providerName}: $why';
          actions = [AxButton(label: 'Try again', onPressed: () => unawaited(si.install(pid)))];
        } else if (env.planning == pid) {
          text = 'Planning the install of ${offer.providerName}…';
          actions = const [];
        } else {
          final what = pid == offer.server.providerId
              ? '${offer.languageName} has a language server: ${offer.providerName}.'
              : '${offer.server.serverId} for ${offer.languageName} needs ${offer.providerName}.';
          text = what;
          actions = [
            AxButton(
              label: 'Install…',
              hint: 'Install ${offer.providerName}: review what it downloads first',
              onPressed: () => unawaited(si.install(pid)),
            ),
            const SizedBox(width: Metrics.s2),
            AxButton(
              label: 'Not now',
              kind: ButtonKind.quiet,
              hint: 'Hide this for ${offer.languageName} until Axiom restarts',
              onPressed: () => si.dismiss(offer.language),
            ),
          ];
        }
        return Semantics(
          liveRegion: true,
          container: true,
          child: DecoratedBox(
            decoration: BoxDecoration(
              color: c.panel,
              border: Border(bottom: BorderSide(color: c.hairline)),
            ),
            child: Padding(
              padding: const EdgeInsets.fromLTRB(12, 4, 8, 4),
              child: Row(
                children: [
                  AxiomIcon(AxiomIcons.download, size: 14, color: c.accentText),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(text, style: t.type.small.copyWith(color: c.ink)),
                  ),
                  ...actions,
                ],
              ),
            ),
          ),
        );
      },
    );
  }
}

/// One document's code view, with its find bar; reports the cursor and honours reveals.
class _DocView extends StatefulWidget {
  const _DocView({super.key, required this.doc, required this.group, required this.focusNode});
  final Doc doc;
  final int group;
  final FocusNode focusNode;

  @override
  State<_DocView> createState() => _DocViewState();
}

class _DocViewState extends State<_DocView> {
  late final AppState app = AppScope.of(context);

  /// The left group edits the document's controller; the right group its own view of it.
  late final CodeLineEditingController _controller = widget.group == 1
      ? widget.doc.controller
      : widget.doc.secondary;
  late final CodeFindController _find = CodeFindController(_controller);
  late final CodeScrollController _scroll = CodeScrollController();

  /// The gutter's marks against HEAD (recomputed 150 ms after an edit or a repository change).
  final gitMarks = ValueNotifier<Map<int, GutterMark>>(const {});
  Timer? _marksTimer;
  int _marksSeq = 0;

  /// The lines the gutter marks were scheduled for: a caret or selection move changes nothing.
  Object? _marksLines;

  void _gitChanged() => _scheduleMarks(force: true);

  void _scheduleMarks({bool force = false}) {
    if (!force && identical(_controller.codeLines, _marksLines)) return;
    _marksLines = _controller.codeLines;
    _marksTimer?.cancel();
    _marksTimer = Timer(const Duration(milliseconds: 150), () => unawaited(_computeMarks()));
  }

  Future<void> _computeMarks() async {
    if (widget.doc.readOnly) return; // outside the workspace: not in its repository
    final seq = ++_marksSeq;
    final head = await app.git.headText(widget.doc.path);
    if (!mounted || seq != _marksSeq) return;
    if (head == null) {
      gitMarks.value = const {};
      return;
    }
    final now = _controller.text;
    // A large file's diff runs on another isolate (the strings are shared, not copied), so the
    // UI thread does not align thousands of lines after an edit (audit finding 8).
    final marks = _controller.codeLines.length > 2000
        ? await Isolate.run(() => gutterMarks(head, now))
        : gutterMarks(head, now);
    if (!mounted || seq != _marksSeq) return;
    gitMarks.value = marks;
  }

  @override
  void initState() {
    super.initState();
    _controller.addListener(_onChange);
    _controller.addListener(_scheduleMarks);
    app.git.addListener(_gitChanged);
    _scheduleMarks(force: true);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      _reveal();
      _takeFocus();
      _report();
    });
    app.documents.addListener(_reveal);
  }

  @override
  void dispose() {
    _marksTimer?.cancel();
    _controller.removeListener(_scheduleMarks);
    app.git.removeListener(_gitChanged);
    gitMarks.dispose();
    _controller.removeListener(_onChange);
    app.documents.removeListener(_reveal);
    _find.dispose();
    super.dispose();
  }

  /// Focuses this editor when the store asked for it (opened by keyboard).
  void _takeFocus() {
    final d = app.documents;
    if (widget.group != 1 || d.focusRequest != widget.doc.path) return;
    d.focusRequest = null;
    widget.focusNode.requestFocus();
  }

  void _reveal() {
    _takeFocus();
    final r = app.documents.pendingReveal;
    if (r == null || r.path != widget.doc.path || widget.group != 1) return;
    app.documents.pendingReveal = null;
    final c = _controller;
    final line = (r.line - 1).clamp(0, c.lineCount - 1);
    final col = (r.column - 1).clamp(0, c.codeLines[line].length);
    c.selection = CodeLineSelection.collapsed(index: line, offset: col);
    c.makeCursorCenterIfInvisible();
    widget.focusNode.requestFocus();
    _report();
  }

  void _onChange() => _report();

  void _report() {
    if (!widget.focusNode.hasFocus && widget.group != 1) {
      return;
    }
    final c = _controller;
    final s = c.selection;
    app.documents.setCursor(
      s.extentIndex + 1,
      s.extentOffset + 1,
      s.isCollapsed ? 0 : _selectedLength(c),
    );
  }

  final _gutter = GlobalKey();

  @override
  Widget build(BuildContext context) {
    final s = app.settings;
    final c = context.colors;
    widget.doc.highlightColor = c.accentSoft;
    widget.doc.syntax = c.syntax;
    widget.doc.underlineColor = (severity) => switch (severity) {
      1 => c.danger,
      2 => c.warn,
      _ => c.accent,
    };
    return ListenableBuilder(
      listenable: s,
      builder: (context, _) => Focus(
        // Follows focus inside the editor; not a stop of its own.
        canRequestFocus: false,
        skipTraversal: true,
        includeSemantics: false,
        onFocusChange: (has) {
          if (has) _report();
        },
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            ValueListenableBuilder(
              valueListenable: _find,
              builder: (context, v, _) =>
                  v == null ? const SizedBox.shrink() : FindBar(controller: _find),
            ),
            Expanded(
              child: LspEditorFeatures(
                doc: widget.doc,
                controller: _controller,
                geometry: CodeGeometry(
                  fontSize: s.editorFontSize.toDouble(),
                  fontFamily: s.editorFontFamily,
                  gutterKey: _gutter,
                  scroll: _scroll,
                ),
                // re_editor's own node acts (tap, scroll) without a name: merged into one named node.
                builder: (context, overrides) => MergeSemantics(
                  child: Semantics(
                    label: '${widget.doc.path}, editor${widget.group == 2 ? ' (right group)' : ''}',
                    child: ValueListenableBuilder(
                      valueListenable: widget.doc.languageNotifier,
                      builder: (context, language, _) => CodeView(
                        controller: _controller,
                        readOnly: widget.doc.readOnly,
                        language: language,
                        focusNode: widget.focusNode,
                        scrollController: _scroll,
                        findController: _find,
                        fontSize: s.editorFontSize.toDouble(),
                        fontFamily: s.editorFontFamily,
                        wordWrap: s.editorWordWrap,
                        // The bar is drawn above the editor (outside its merged node, so its fields
                        // keep their own names); re_editor gets an empty one.
                        findBuilder: (context, controller, readOnly) => const _NoFindBar(),
                        onSave: () => app.documents.save(widget.doc.path),
                        overrides: overrides,
                        gutterKey: _gutter,
                        gitMarks: gitMarks,
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// The find (and replace) bar: Axiom's controls over re_editor's find controller.
class _NoFindBar extends StatelessWidget implements PreferredSizeWidget {
  const _NoFindBar();

  @override
  Size get preferredSize => Size.zero;

  @override
  Widget build(BuildContext context) => const SizedBox.shrink();
}

class FindBar extends StatelessWidget implements PreferredSizeWidget {
  const FindBar({super.key, required this.controller});
  final CodeFindController controller;

  @override
  Size get preferredSize =>
      Size.fromHeight(controller.value == null ? 0 : (controller.value!.replaceMode ? 72 : 40));

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    return ValueListenableBuilder(
      valueListenable: controller,
      builder: (context, v, _) {
        if (v == null) return const SizedBox.shrink();
        final r = v.result;
        final count = r == null || r.matches.isEmpty
            ? 'No results'
            : '${r.index + 1} of ${r.matches.length}';
        // In a narrow group (a split, a small window) the bar shrinks instead of overflowing:
        // the field takes what is left, the toggles show short names (AUD-053).
        Widget toggle(String label, String short, bool on, VoidCallback f, bool narrow) =>
            Semantics(
              toggled: on,
              label: narrow ? label : null,
              child: narrow
                  ? ExcludeSemantics(
                      child: AxButton(
                        label: short,
                        kind: on ? ButtonKind.standard : ButtonKind.quiet,
                        onPressed: f,
                      ),
                    )
                  : AxButton(
                      label: label,
                      kind: on ? ButtonKind.standard : ButtonKind.quiet,
                      onPressed: f,
                    ),
            );
        return Shortcuts(
          shortcuts: const {
            SingleActivator(LogicalKeyboardKey.escape): DismissIntent(),
            SingleActivator(LogicalKeyboardKey.enter): _NextIntent(),
            SingleActivator(LogicalKeyboardKey.enter, shift: true): _PrevIntent(),
          },
          child: Actions(
            actions: {
              DismissIntent: CallbackAction<DismissIntent>(onInvoke: (_) => controller.close()),
              _NextIntent: CallbackAction<_NextIntent>(onInvoke: (_) => controller.nextMatch()),
              _PrevIntent: CallbackAction<_PrevIntent>(onInvoke: (_) => controller.previousMatch()),
            },
            child: ColoredBox(
              color: c.bar,
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
                child: LayoutBuilder(
                  builder: (context, box) {
                    final narrow = box.maxWidth < 640;
                    return Column(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Row(
                          children: [
                            Flexible(
                              child: ConstrainedBox(
                                constraints: const BoxConstraints(maxWidth: 280),
                                child: AxTextInput(
                                  label: 'Find',
                                  controller: controller.findInputController,
                                  focusNode: controller.findInputFocusNode,
                                  placeholder: 'Find',
                                  mono: true,
                                ),
                              ),
                            ),
                            const SizedBox(width: 6),
                            toggle(
                              'Match case',
                              'Aa',
                              v.option.caseSensitive,
                              controller.toggleCaseSensitive,
                              narrow,
                            ),
                            toggle(
                              'Regular expression',
                              '.*',
                              v.option.regex,
                              controller.toggleRegex,
                              narrow,
                            ),
                            const SizedBox(width: 6),
                            Semantics(liveRegion: true, child: Text(count, style: t.type.numeric)),
                            const SizedBox(width: 6),
                            AxIconButton(
                              icon: AxiomIcons.arrowDown,
                              label: 'Next match',
                              shortcut: 'Enter',
                              onPressed: controller.nextMatch,
                            ),
                            if (!narrow)
                              AxButton(
                                label: v.replaceMode ? 'Hide replace' : 'Replace…',
                                kind: ButtonKind.quiet,
                                onPressed: controller.toggleMode,
                              )
                            else
                              AxIconButton(
                                icon: AxiomIcons.more,
                                label: v.replaceMode ? 'Hide replace' : 'Replace…',
                                onPressed: controller.toggleMode,
                              ),
                            if (!narrow) const Spacer(),
                            AxIconButton(
                              icon: AxiomIcons.close,
                              label: 'Close find',
                              shortcut: 'Esc',
                              onPressed: controller.close,
                            ),
                          ],
                        ),
                        if (v.replaceMode)
                          Padding(
                            padding: const EdgeInsets.only(top: 4),
                            child: Row(
                              children: [
                                Flexible(
                                  child: ConstrainedBox(
                                    constraints: const BoxConstraints(maxWidth: 280),
                                    child: AxTextInput(
                                      label: 'Replace with',
                                      controller: controller.replaceInputController,
                                      focusNode: controller.replaceInputFocusNode,
                                      placeholder: 'Replace with',
                                      mono: true,
                                    ),
                                  ),
                                ),
                                const SizedBox(width: 6),
                                AxButton(
                                  label: 'Replace',
                                  kind: ButtonKind.quiet,
                                  onPressed: controller.replaceMatch,
                                ),
                                AxButton(
                                  label: 'Replace all',
                                  kind: ButtonKind.quiet,
                                  onPressed: controller.replaceAllMatches,
                                ),
                              ],
                            ),
                          ),
                      ],
                    );
                  },
                ),
              ),
            ),
          ),
        );
      },
    );
  }
}

class _NextIntent extends Intent {
  const _NextIntent();
}

class _PrevIntent extends Intent {
  const _PrevIntent();
}

/// The selection's length in characters (newlines counted), from the lines' lengths: no string
/// is built, so a long selection costs one pass over its lines, not a copy of its text.
int _selectedLength(CodeLineEditingController c) {
  final s = c.selection;
  final lines = c.codeLines;
  final (a, ao, b, bo) = (s.startIndex, s.startOffset, s.endIndex, s.endOffset);
  if (a == b) return (bo - ao).abs();
  var n = lines[a].length - ao + 1;
  for (var i = a + 1; i < b; i++) {
    n += lines[i].length + 1;
  }
  return n + bo;
}
