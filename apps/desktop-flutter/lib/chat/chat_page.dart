// Chat (FL-CHAT-001; ADR-0017, ADR-0020 §6): Axiom's own conversation surface, under Files on
// the rail. One conversation list and one composer for Axiom's agent (any configured model) and
// the external agent harnesses (Claude Code, Codex, Gemini CLI), with or without a folder;
// a missing harness can be installed from here.

import 'dart:async';
import 'dart:convert';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/semantics.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../app/app_state.dart';
import '../app/open_link.dart';
import '../daemon/protocol.g.dart';
import '../state/agent.dart';
import '../ui/controls.dart';
import '../ui/icon_data.dart';
import '../ui/icons.dart';
import '../ui/overlays.dart';
import '../ui/pressable.dart';
import '../ui/surfaces.dart';
import '../ui/tabs.dart';
import '../ui/text_input.dart';
import '../ui/theme.dart';
import '../ui/tokens.dart';
import 'markdown_view.dart';
import 'models.dart';

String _ago(int ms) {
  final d = DateTime.now().difference(DateTime.fromMillisecondsSinceEpoch(ms));
  if (d.inMinutes < 1) return 'now';
  if (d.inHours < 1) return '${d.inMinutes} min';
  if (d.inDays < 1) return '${d.inHours} h';
  if (d.inDays < 7) return '${d.inDays} d';
  return '${(d.inDays / 7).floor()} w';
}

/// The side bar of the Chat destination: new chat, scope, search, the conversations.
class ChatSide extends StatefulWidget {
  const ChatSide({super.key});

  @override
  State<ChatSide> createState() => _ChatSideState();
}

class _ChatSideState extends State<ChatSide> {
  late final AppState app = AppScope.of(context);
  final _search = TextEditingController();

  @override
  void initState() {
    super.initState();
    _search.addListener(() => app.agent.setSearch(_search.text));
    _loadSoon(app);
  }

  @override
  void dispose() {
    _search.dispose();
    super.dispose();
  }

  Future<void> _rename(SessionInfo s) async {
    final title = await showAxDialog<String>(
      context: context,
      label: 'Rename the conversation',
      builder: (_) => _OneField(title: 'Rename', label: 'Title', initial: s.title),
    );
    if (title != null && title.trim().isNotEmpty) await app.agent.rename(s.sessionId, title.trim());
  }

  Future<void> _delete(SessionInfo s) async {
    final answer = await showAxDialog<String>(
      context: context,
      label: 'Delete the conversation',
      builder: (_) => AxQuestion(
        title: 'Delete “${s.title}”?',
        detail:
            'The conversation and its checkpoints are removed. Files the agent changed stay '
            'as they are.',
        actions: const [('cancel', 'Cancel'), ('delete', 'Delete')],
      ),
    );
    if (answer == 'delete') await app.agent.remove(s.sessionId);
  }

  /// F2 renames the focused conversation, Delete deletes it (after asking).
  KeyEventResult _rowKey(SessionInfo s, KeyEvent e) {
    if (e is! KeyDownEvent) return KeyEventResult.ignored;
    if (e.logicalKey == LogicalKeyboardKey.f2) {
      unawaited(_rename(s));
      return KeyEventResult.handled;
    }
    if (e.logicalKey == LogicalKeyboardKey.delete) {
      unawaited(_delete(s));
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final a = app.agent;
    return ListenableBuilder(
      listenable: Listenable.merge([a, app.workspace]),
      builder: (context, _) => Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(Metrics.s4, Metrics.s2, Metrics.s4, Metrics.s3),
            child: AxButton(
              label: 'New chat',
              icon: AxiomIcons.plus,
              hint: 'New chat — Mod+N',
              onPressed: () => a.newChat(),
            ),
          ),
          if (app.workspace.isOpen)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: Metrics.s3),
              child: AxTabs(
                label: 'Conversations of',
                height: 28,
                tabs: [
                  const TabItem(id: 'general', label: 'General'),
                  TabItem(id: 'workspace', label: app.workspace.name),
                ],
                selected: a.scope.name,
                onSelect: (id) => a.setScope(id == 'workspace' ? Scope.workspace : Scope.general),
              ),
            ),
          Padding(
            padding: const EdgeInsets.all(Metrics.s3),
            child: AxTextInput(
              label: 'Search conversations',
              controller: _search,
              placeholder: 'Search conversations',
              leading: AxiomIcon(AxiomIcons.search, size: 14, color: c.ink3),
            ),
          ),
          Expanded(
            child: Builder(
              builder: (context) {
                final list = a.filtered();
                if (list.isEmpty) {
                  return Padding(
                    padding: const EdgeInsets.all(Metrics.s5),
                    child: Text(
                      a.search.isEmpty ? 'No conversations yet.' : 'No conversation matches.',
                      style: t.type.small,
                    ),
                  );
                }
                return ListView.builder(
                  itemExtent: 30,
                  padding: const EdgeInsets.symmetric(horizontal: Metrics.s2),
                  itemCount: list.length,
                  itemBuilder: (context, i) {
                    final s = list[i];
                    final on = s.sessionId == a.activeId;
                    final approvalsWaiting = a.approvals
                        .where((x) => x.sessionId == s.sessionId)
                        .length;
                    return Focus(
                      canRequestFocus: false,
                      skipTraversal: true,
                      onKeyEvent: (_, e) => _rowKey(s, e),
                      child: AxListRow(
                        hint: 'Open — F2 renames, Delete deletes',
                        label: [
                          s.title,
                          if (s.running) 'writing',
                          if (approvalsWaiting > 0) '$approvalsWaiting waiting for you',
                          _ago(s.updatedMs),
                        ].join(', '),
                        selected: on,
                        height: 30,
                        onPressed: () => unawaited(a.open(s.sessionId)),
                        child: Padding(
                          padding: const EdgeInsets.only(left: 8),
                          child: Row(
                            children: [
                              if (s.running || approvalsWaiting > 0) ...[
                                StatusDot(
                                  approvalsWaiting > 0 ? DotState.warn : DotState.running,
                                  ring: on ? c.raised : c.bar,
                                ),
                                const SizedBox(width: 6),
                              ],
                              Expanded(
                                child: Text(
                                  s.title,
                                  style: t.type.body,
                                  overflow: TextOverflow.ellipsis,
                                ),
                              ),
                              Text(_ago(s.updatedMs), style: t.type.numeric.copyWith(fontSize: 11)),
                              AxMenu(
                                label: 'More for ${s.title}',
                                entries: () => [
                                  MenuEntry.item('Rename…', () => unawaited(_rename(s))),
                                  MenuEntry.item('Delete…', () => unawaited(_delete(s))),
                                ],
                                trigger: (context, st, open) => Padding(
                                  padding: const EdgeInsets.symmetric(horizontal: 6),
                                  child: AxiomIcon(
                                    AxiomIcons.more,
                                    size: 14,
                                    color: st.hovered || open ? c.ink : c.ink3,
                                  ),
                                ),
                              ),
                            ],
                          ),
                        ),
                      ),
                    );
                  },
                );
              },
            ),
          ),
        ],
      ),
    );
  }
}

/// The open conversation: answer line, transcript, approvals, composer (or setup).
class ChatView extends StatefulWidget {
  const ChatView({super.key, this.dock = false});

  /// Beside the editor (workspace-spec.md §5.2): the folder's conversations, switched from the
  /// header.
  final bool dock;

  @override
  State<ChatView> createState() => ChatViewState();
}

class ChatViewState extends State<ChatView> {
  late final AppState app = AppScope.of(context);
  final composer = TextEditingController();
  final composerFocus = FocusNode(debugLabel: 'chat composer');
  final _scroll = ScrollController();
  final _selection = FocusNode(skipTraversal: true, canRequestFocus: false);
  int? _editing;
  bool _stick = true;

  /// Images attached to the next message.
  final attached = <ImageData>[];
  String _attachNote = '';

  static const maxImageBytes = 5 * 1024 * 1024;
  static const imageTypes = {
    'png': 'image/png',
    'jpg': 'image/jpeg',
    'jpeg': 'image/jpeg',
    'gif': 'image/gif',
    'webp': 'image/webp',
  };

  /// Attaches an image (refused over 5 MiB or of another type, with a note).
  void addImage(Uint8List bytes, String name) {
    final ext = name.contains('.') ? name.split('.').last.toLowerCase() : '';
    final type = imageTypes[ext];
    setState(() {
      if (type == null) {
        _attachNote = '$name is not a PNG, JPEG, GIF or WebP image.';
      } else if (bytes.length > maxImageBytes) {
        _attachNote = '$name is larger than 5 MiB.';
      } else {
        _attachNote = '';
        attached.add(ImageData(mediaType: type, data: base64Encode(bytes)));
      }
    });
  }

  void removeAttached(int i) => setState(() => attached.removeAt(i));

  Future<void> _pickImages() async {
    final files = await openFiles(
      acceptedTypeGroups: [XTypeGroup(label: 'Images', extensions: imageTypes.keys.toList())],
    );
    for (final f in files) {
      final bytes = await f.readAsBytes();
      if (mounted) addImage(bytes, f.name);
    }
    composerFocus.requestFocus();
  }

  @override
  void initState() {
    super.initState();
    _loadSoon(app);
    _scroll.addListener(() {
      if (!_scroll.hasClients) return;
      _stick = _scroll.position.pixels >= _scroll.position.maxScrollExtent - 24;
    });
    app.agent.addListener(_follow);
    if (widget.dock) {
      app.focusDock = _focusSoon;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted && app.agent.scope != Scope.workspace) app.agent.setScope(Scope.workspace);
      });
    } else {
      app.focusComposer = _focusSoon;
    }
    _focusSoon();
  }

  /// After the frame that shows Chat (focus cannot enter it while it is still excluded).
  void _focusSoon() => WidgetsBinding.instance.addPostFrameCallback((_) {
    if (mounted) composerFocus.requestFocus();
  });

  @override
  void dispose() {
    app.agent.removeListener(_follow);
    if (app.focusComposer == _focusSoon) app.focusComposer = null;
    if (app.focusDock == _focusSoon) app.focusDock = null;
    composer.dispose();
    composerFocus.dispose();
    _scroll.dispose();
    _selection.dispose();
    super.dispose();
  }

  bool _wasRunning = false;

  /// Keep the newest text in view while the person has not scrolled up; announce the end of an
  /// answer to a screen reader.
  void _follow() {
    final running = app.agent.active.running;
    if (_wasRunning && !running && mounted && TickerMode.valuesOf(context).enabled) {
      unawaited(
        SemanticsService.sendAnnouncement(View.of(context), 'Answer complete', TextDirection.ltr),
      );
    }
    _wasRunning = running;
    if (!_stick) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scroll.hasClients) _scroll.jumpTo(_scroll.position.maxScrollExtent);
    });
  }

  Future<void> send() async {
    final text = composer.text;
    final images = [...attached];
    if (text.trim().isEmpty && images.isEmpty) return;
    final editing = _editing;
    composer.clear();
    setState(() {
      _editing = null;
      attached.clear();
      _attachNote = '';
    });
    _stick = true;
    final ok = editing != null
        ? await app.agent.edit(editing, text)
        : await app.agent.send(text, images: images);
    if (!ok && mounted && composer.text.isEmpty) {
      composer.text = text;
      setState(() => attached.addAll(images));
    }
  }

  /// The composer's keys: Enter sends (Shift+Enter is a new line), ↑ in an empty composer
  /// edits the last message, Esc stops the reply or cancels the edit.
  KeyEventResult _key(FocusNode _, KeyEvent e) {
    if (e is! KeyDownEvent) return KeyEventResult.ignored;
    final k = HardwareKeyboard.instance;
    final plain = !k.isShiftPressed && !k.isControlPressed && !k.isAltPressed && !k.isMetaPressed;
    switch (e.logicalKey) {
      case LogicalKeyboardKey.enter || LogicalKeyboardKey.numpadEnter when plain:
        unawaited(send());
        return KeyEventResult.handled;
      case LogicalKeyboardKey.arrowUp when plain && composer.text.isEmpty:
        _editLast();
        return KeyEventResult.handled;
      case LogicalKeyboardKey.escape when app.agent.active.running || _editing != null:
        if (app.agent.active.running) app.agent.stop();
        if (_editing != null) {
          composer.clear();
          setState(() => _editing = null);
        }
        return KeyEventResult.handled;
      default:
        return KeyEventResult.ignored;
    }
  }

  /// Each item's widget, reused while the item and its buttons are the same: a streamed delta
  /// rebuilds only the streaming text, not the transcript above it.
  var _views = <Item, (bool, bool, Widget)>{};

  List<Widget> _itemViews(List<Item> items, bool running) {
    final next = <Item, (bool, bool, Widget)>{};
    final last = lastUser(items);
    final out = <Widget>[];
    for (final (i, it) in items.indexed) {
      final edit = it is UserItem && identical(it, last) && !running;
      final regen = it is AssistantItem && i == items.length - 1 && !running;
      final old = _views[it];
      final w = old != null && old.$1 == edit && old.$2 == regen
          ? old.$3
          : _ItemView(
              item: it,
              onEdit: edit ? _editLast : null,
              onRegenerate: regen
                  ? () {
                      // The button goes away with the answer: focus stays in the conversation.
                      composerFocus.requestFocus();
                      unawaited(app.agent.regenerate());
                    }
                  : null,
            );
      next[it] = (edit, regen, w);
      out.add(w);
    }
    _views = next;
    return out;
  }

  /// ↑ in an empty composer edits the last message.
  void _editLast() {
    final u = lastUser(app.agent.items());
    if (u == null || app.agent.active.running) return;
    setState(() => _editing = u.turn);
    composer.text = u.text;
    composer.selection = TextSelection.collapsed(offset: u.text.length);
  }

  /// Who answers: the open conversation's provider, or the one chosen for a new chat.
  ProviderOption? _provider() {
    final a = app.agent;
    final s = a.sessions.where((x) => x.sessionId == a.activeId).firstOrNull;
    final id = s?.providerId ?? a.model?.providerId;
    return a.providers.where((x) => x.providerId == id).firstOrNull;
  }

  String? _external() {
    final p = _provider();
    return p != null && p.isAgent ? p.name : null;
  }

  String _who() => _provider()?.name ?? 'Axiom';

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final a = app.agent;
    final g = widget.dock ? 14.0 : 28.0;
    return ListenableBuilder(
      listenable: a,
      builder: (context, _) {
        final items = a.items();
        final running = a.active.running;
        final title = a.activeId == null
            ? null
            : a.sessions.where((s) => s.sessionId == a.activeId).firstOrNull?.title;
        final hasModel = a.providers.any((p) => p.usable);
        final answer = running
            ? '${_who()} is writing…'
            : (title ??
                  (hasModel || a.status != 'ready' ? 'Ask ${_who()}' : 'Choose who to talk to'));
        final mine = [
          for (final r in a.approvals)
            if (r.sessionId != null && r.sessionId == a.activeId) r,
        ];
        return ColoredBox(
          color: widget.dock ? c.bar : c.ground,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: EdgeInsets.fromLTRB(g, widget.dock ? 8 : 20, g, 8),
                child: widget.dock
                    ? _DockHeader(title: title ?? 'New conversation')
                    : Semantics(liveRegion: running, child: AnswerLine(answer)),
              ),
              if (a.error.isNotEmpty) _Banner(text: a.error, gutter: g, onDismiss: a.dismissError),
              Expanded(
                child: a.status == 'ready' && !hasModel && a.activeId == null
                    ? SingleChildScrollView(padding: EdgeInsets.all(g), child: const ModelSetup())
                    : SelectableRegion(
                        selectionControls: emptyTextSelectionControls,
                        focusNode: _selection,
                        child: ListView(
                          controller: _scroll,
                          padding: EdgeInsets.fromLTRB(g, 8, g, 16),
                          children: [
                            ..._itemViews(items, running),
                            // The text being streamed, where its item will be: the same
                            // element then shows the finished answer.
                            if (a.active.streaming.isNotEmpty)
                              _ItemView(
                                item: AssistantItem('streaming', 0, a.active.streaming, null),
                              ),
                            if (running && a.active.streaming.isEmpty)
                              const Padding(
                                padding: EdgeInsets.only(bottom: 14),
                                child: Align(
                                  alignment: Alignment.centerLeft,
                                  child: Spinner(label: 'Writing'),
                                ),
                              ),
                          ],
                        ),
                      ),
              ),
              for (final r in mine)
                Padding(
                  padding: EdgeInsets.fromLTRB(g, 0, g, 10),
                  child: ApprovalCard(
                    key: ValueKey(r.requestId),
                    request: r,
                    external: _external(),
                    takeFocus: true,
                  ),
                ),
              if (a.restoreConflict != null) _RestoreConflict(gutter: g),
              if (a.sendError.isNotEmpty)
                _Banner(text: a.sendError, gutter: g, onDismiss: a.dismissError),
              Padding(
                padding: EdgeInsets.fromLTRB(g, 6, g, widget.dock ? 12 : 18),
                child: _Composer(state: this, running: running),
              ),
            ],
          ),
        );
      },
    );
  }
}

/// The dock's header: the folder's conversations to switch between, and New chat.
class _DockHeader extends StatelessWidget {
  const _DockHeader({required this.title});
  final String title;

  @override
  Widget build(BuildContext context) {
    final a = AppScope.of(context).agent;
    final t = context.axiom;
    final c = t.colors;
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: 'AI dock',
      child: Row(
        children: [
          Expanded(
            child: Align(
              alignment: Alignment.centerLeft,
              child: AxMenu(
                label: 'Conversation: $title',
                hint: 'Switch conversations in this folder',
                entries: () => [
                  MenuEntry.item('New conversation', () => a.newChat(Scope.workspace)),
                  for (final s in a.sessions.take(20))
                    MenuEntry.item(s.title, () => unawaited(a.open(s.sessionId))),
                ],
                trigger: (context, s, open) => FocusRing(
                  visible: s.focused,
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      color: open || s.hovered ? c.hover : null,
                      borderRadius: BorderRadius.circular(Metrics.radiusControl),
                    ),
                    child: Padding(
                      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 3),
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Flexible(
                            child: Text(
                              title,
                              style: t.type.label,
                              overflow: TextOverflow.ellipsis,
                            ),
                          ),
                          const SizedBox(width: 4),
                          AxiomIcon(AxiomIcons.arrowDown, size: 12, color: c.ink3),
                        ],
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
          AxIconButton(
            icon: AxiomIcons.plus,
            label: 'New chat',
            shortcut: 'Mod+N',
            size: 26,
            onPressed: () => a.newChat(Scope.workspace),
          ),
        ],
      ),
    );
  }
}

class _Composer extends StatelessWidget {
  const _Composer({required this.state, required this.running});
  final ChatViewState state;
  final bool running;

  @override
  Widget build(BuildContext context) {
    final app = state.app;
    final t = context.axiom;
    final c = t.colors;
    final p = state._provider();
    final m = p?.models.where((x) => x.id == (app.agent.model?.model)).firstOrNull;
    final note = state._attachNote.isNotEmpty
        ? state._attachNote
        : (state.attached.isNotEmpty && p != null && !p.isAgent && m != null && !m.images
              ? '${m.name} cannot see images; it will be told one was attached.'
              : '');
    final input = Focus(
      onKeyEvent: state._key,
      canRequestFocus: false,
      skipTraversal: true,
      child: AxTextInput(
        label: state._editing != null ? 'Edit your message' : 'Message',
        controller: state.composer,
        focusNode: state.composerFocus,
        bezel: true,
        minLines: 2,
        maxLines: 10,
        placeholder: state._editing != null
            ? 'Edit your message (Esc cancels)'
            : 'Ask Axiom or an agent… (Shift+Enter for a new line)',
        trailing: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            const ModelPicker(),
            const SizedBox(height: 6),
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                AxIconButton(
                  icon: AxiomIcons.image,
                  label: 'Attach an image',
                  size: 28,
                  onPressed: running ? null : () => unawaited(state._pickImages()),
                ),
                const SizedBox(width: 4),
                running
                    ? AxIconButton(
                        icon: AxiomIcons.stop,
                        label: 'Stop the reply',
                        shortcut: 'Esc',
                        color: c.dangerText,
                        onPressed: app.agent.stop,
                      )
                    : InkCircleButton(
                        icon: AxiomIcons.send,
                        label: 'Send',
                        shortcut: 'Enter',
                        onPressed: () => unawaited(state.send()),
                      ),
              ],
            ),
          ],
        ),
      ),
    );
    if (state.attached.isEmpty && note.isEmpty) return input;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (state.attached.isNotEmpty)
          Padding(
            padding: const EdgeInsets.only(bottom: 6),
            child: Wrap(
              spacing: 6,
              runSpacing: 6,
              children: [
                for (final (i, im) in state.attached.indexed)
                  Semantics(
                    container: true,
                    explicitChildNodes: true,
                    label: 'Attached image ${i + 1}',
                    child: Stack(
                      children: [
                        Thumbnail(data: im, size: 56),
                        Positioned(
                          right: 0,
                          top: 0,
                          child: AxIconButton(
                            icon: AxiomIcons.close,
                            label: 'Remove image ${i + 1}',
                            size: 20,
                            onPressed: () => state.removeAttached(i),
                          ),
                        ),
                      ],
                    ),
                  ),
              ],
            ),
          ),
        if (note.isNotEmpty)
          Padding(
            padding: const EdgeInsets.only(bottom: 6),
            child: Semantics(liveRegion: true, child: Text(note, style: t.type.small)),
          ),
        input,
      ],
    );
  }
}

/// An image from base64, decoded once per image.
class Thumbnail extends StatelessWidget {
  const Thumbnail({super.key, required this.data, this.size = 120});
  final ImageData data;
  final double size;
  static final _decoded = <String, Uint8List>{};

  @override
  Widget build(BuildContext context) {
    final key = '${data.data.length}:${data.data.hashCode}';
    final bytes = _decoded.putIfAbsent(key, () => base64Decode(data.data));
    return ClipRRect(
      borderRadius: BorderRadius.circular(Metrics.radiusControl),
      child: Image.memory(
        bytes,
        width: size,
        height: size,
        fit: BoxFit.cover,
        gaplessPlayback: true,
        semanticLabel: 'Image',
      ),
    );
  }
}

/// Who answers: Axiom's models and the external agents, two-tone ("Claude Code" bright, the
/// model dim); a missing agent is offered for installation.
class ModelPicker extends StatelessWidget {
  const ModelPicker({super.key});

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final a = app.agent;
    final t = context.axiom;
    final c = t.colors;
    final locked = a.activeId != null;
    // An open conversation keeps who answers it.
    final session = a.sessions.where((x) => x.sessionId == a.activeId).firstOrNull;
    final ModelChoice? m = locked && session != null
        ? (providerId: session.providerId, model: session.model)
        : a.model;
    final p = m == null ? null : a.providers.where((x) => x.providerId == m.providerId).firstOrNull;
    final modelName = p?.models.where((x) => x.id == m!.model).firstOrNull?.name ?? m?.model ?? '';
    final label = p == null
        ? 'Choose a model'
        : '${p.name}${p.isAgent ? ' (agent)' : ''}, $modelName';
    if (locked) {
      return Semantics(
        label: '$label. A conversation keeps who answers; start a new chat to change it.',
        excludeSemantics: true,
        child: Text.rich(
          TextSpan(
            children: [
              TextSpan(
                text: p?.name ?? '',
                style: t.type.label.copyWith(color: c.ink2),
              ),
              if (!(p?.isAgent ?? false))
                TextSpan(
                  text: '  $modelName',
                  style: t.type.tiny.copyWith(color: c.ink3),
                ),
            ],
          ),
        ),
      );
    }
    return AxMenu(
      label: label,
      hint: 'Choose who answers a new chat',
      entries: () {
        final out = <MenuEntry>[];
        final models = [
          for (final x in a.providers)
            if (!x.isAgent) x,
        ];
        final agents = [
          for (final x in a.providers)
            if (x.isAgent) x,
        ];
        for (final x in models) {
          out.add(MenuEntry.heading(x.name));
          if (!x.usable) {
            out.add(MenuEntry.item(x.error ?? 'No models', null, enabled: false));
            continue;
          }
          for (final mo in x.models) {
            out.add(
              MenuEntry.item(mo.name, () => a.setModel((providerId: x.providerId, model: mo.id))),
            );
          }
        }
        if (agents.isNotEmpty) out.add(const MenuEntry.heading('Agents'));
        for (final x in agents) {
          if (x.usable) {
            out.add(
              MenuEntry.item(
                x.name,
                () => a.setModel((providerId: x.providerId, model: x.models.first.id)),
              ),
            );
          } else if (x.builtin) {
            out.add(
              MenuEntry.item(
                a.installing.contains(x.providerId)
                    ? 'Installing ${x.name}…'
                    : 'Install ${x.name}…',
                a.installing.contains(x.providerId)
                    ? null
                    : () => unawaited(installAgent(context, x)),
                enabled: !a.installing.contains(x.providerId),
              ),
            );
          }
        }
        return out;
      },
      trigger: (context, s, open) => FocusRing(
        visible: s.focused,
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: open || s.hovered ? c.hover : null,
            borderRadius: BorderRadius.circular(Metrics.radiusControl),
          ),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
            child: Text.rich(
              TextSpan(
                children: [
                  TextSpan(
                    text: p?.name ?? 'Choose a model',
                    style: t.type.label.copyWith(color: c.ink),
                  ),
                  if (p != null && !p.isAgent)
                    TextSpan(
                      text: '  $modelName',
                      style: t.type.tiny.copyWith(color: c.ink3),
                    ),
                  if (p != null && p.isAgent)
                    TextSpan(
                      text: '  agent',
                      style: t.type.tiny.copyWith(color: c.ink3),
                    ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _ItemView extends StatefulWidget {
  const _ItemView({required this.item, this.onEdit, this.onRegenerate});
  final Item item;
  final VoidCallback? onEdit;
  final VoidCallback? onRegenerate;

  @override
  State<_ItemView> createState() => _ItemViewState();
}

class _ItemViewState extends State<_ItemView> {
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final app = AppScope.of(context);
    final it = widget.item;
    Widget pad(Widget w) => Padding(padding: const EdgeInsets.only(bottom: 14), child: w);
    switch (it) {
      case UserItem(:final text, :final images):
        for (final im in images) {
          unawaited(app.agent.loadImage(im));
        }
        return pad(
          Align(
            alignment: Alignment.centerRight,
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 640),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.end,
                children: [
                  Raised(
                    child: Padding(
                      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
                      child: Text(text, style: t.type.body.copyWith(fontSize: 14, height: 1.5)),
                    ),
                  ),
                  if (images.isNotEmpty)
                    Padding(
                      padding: const EdgeInsets.only(top: 6),
                      // The images arrive after the transcript: only this part follows them.
                      child: ListenableBuilder(
                        listenable: app.agent,
                        builder: (context, _) => Wrap(
                          spacing: 6,
                          children: [
                            for (final im in images)
                              app.agent.images[im.sha256] != null
                                  ? Thumbnail(data: app.agent.images[im.sha256]!, size: 96)
                                  : SizedBox(
                                      width: 96,
                                      height: 96,
                                      child: Well(
                                        radius: Metrics.radiusControl,
                                        child: Center(
                                          child: AxiomIcon(
                                            AxiomIcons.image,
                                            size: 16,
                                            color: c.ink3,
                                          ),
                                        ),
                                      ),
                                    ),
                          ],
                        ),
                      ),
                    ),
                  if (images.isNotEmpty)
                    Padding(
                      padding: const EdgeInsets.only(top: 4),
                      child: Text(
                        '${images.length} image${images.length == 1 ? '' : 's'} attached',
                        style: t.type.tiny,
                      ),
                    ),
                  if (widget.onEdit != null)
                    AxButton(
                      label: 'Edit',
                      kind: ButtonKind.quiet,
                      hint: 'Edit your last message — ↑',
                      onPressed: widget.onEdit,
                    ),
                ],
              ),
            ),
          ),
        );
      case AssistantItem(:final text, :final textKind):
        final dim = textKind != null;
        return pad(
          Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              if (dim)
                Padding(
                  padding: const EdgeInsets.only(bottom: 4),
                  child: Text(textKind == TextKind.plan ? 'Plan' : 'Thinking', style: t.type.tiny),
                ),
              MarkdownView(text, dim: dim, onOpenLink: _openLink),
              if (widget.onRegenerate != null)
                AxButton(
                  label: 'Regenerate',
                  kind: ButtonKind.quiet,
                  icon: AxiomIcons.refresh,
                  onPressed: widget.onRegenerate,
                ),
            ],
          ),
        );
      case ToolItem(:final tool, :final targets, :final state, :final summary, :final output):
        final dot = switch (state) {
          ToolState.running => DotState.running,
          ToolState.ok => DotState.ok,
          ToolState.error => DotState.error,
        };
        final word = switch (state) {
          ToolState.running => 'running',
          ToolState.ok => 'done',
          ToolState.error => 'failed',
        };
        return pad(
          Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              AxListRow(
                label: '$tool ${targets.join(', ')}, $word${summary.isEmpty ? '' : ': $summary'}',
                hint: output == null ? null : (_open ? 'Hide the output' : 'Show the output'),
                onPressed: output == null ? null : () => setState(() => _open = !_open),
                height: 28,
                child: Row(
                  children: [
                    const SizedBox(width: 8),
                    StatusDot(dot, ring: c.ground),
                    const SizedBox(width: 8),
                    Text(tool, style: t.type.codeSmall.copyWith(color: c.ink)),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        targets.join(', '),
                        style: t.type.codeSmall,
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                    Flexible(
                      child: Text(summary, style: t.type.tiny, overflow: TextOverflow.ellipsis),
                    ),
                    const SizedBox(width: 8),
                  ],
                ),
              ),
              if (_open && output != null)
                Padding(
                  padding: const EdgeInsets.only(top: 4),
                  child: CodeBlock(
                    code: output is String
                        ? output
                        : const JsonEncoder.withIndent('  ').convert(output),
                    language: output is String ? '' : 'json',
                  ),
                ),
            ],
          ),
        );
      case CheckpointItem(:final number, :final label):
        return pad(
          Row(
            children: [
              const Expanded(child: Hairline()),
              const SizedBox(width: 8),
              Text('Checkpoint $number · $label', style: t.type.tiny),
              const SizedBox(width: 6),
              AxButton(
                label: 'Restore',
                kind: ButtonKind.quiet,
                hint: 'Restore the files the agent changed since checkpoint $number',
                onPressed: () {
                  context.findAncestorStateOfType<ChatViewState>()?.composerFocus.requestFocus();
                  unawaited(app.agent.restore(number));
                },
              ),
              const SizedBox(width: 8),
              const Expanded(child: Hairline()),
            ],
          ),
        );
      case NoticeItem(:final message, :final notice):
        return pad(
          Semantics(
            liveRegion: true,
            child: Text(
              message,
              style: t.type.small.copyWith(color: notice ? c.ink2 : c.dangerText),
            ),
          ),
        );
    }
  }
}

/// An approval the agent waits for (workspace-spec.md §5.2): what it wants to do and why it asks,
/// the diff of a write, then Allow once (Enter), Allow for session, or Deny (Esc).
class ApprovalCard extends StatefulWidget {
  const ApprovalCard({
    super.key,
    required this.request,
    this.external,
    this.origin,
    this.takeFocus = false,
  });
  final ApprovalRequest request;

  /// The external agent's name, when the conversation is with one.
  final String? external;

  /// Where it comes from, for an approval outside the open conversation.
  final String? origin;

  /// Focus Allow once when the card appears (the open conversation's card: Enter allows, Esc
  /// denies).
  final bool takeFocus;

  static const effects = {
    Effect2.read: 'reads files',
    Effect2.writeWorkspace: 'changes files in the folder',
    Effect2.execute: 'runs a program',
    Effect2.install: 'installs a toolchain',
    Effect2.network: 'uses the network',
    Effect2.writeOutside: 'changes files outside the folder',
    Effect2.delete: 'deletes files',
    Effect2.credentials: 'uses credentials',
    Effect2.deploy: 'deploys',
  };

  @override
  State<ApprovalCard> createState() => _ApprovalCardState();
}

class _ApprovalCardState extends State<ApprovalCard> {
  final _allow = FocusNode(debugLabel: 'allow once');
  final _card = FocusNode(debugLabel: 'approval card', canRequestFocus: false, skipTraversal: true);

  @override
  void initState() {
    super.initState();
    if (widget.takeFocus) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _allow.requestFocus();
      });
    }
  }

  @override
  void dispose() {
    _allow.dispose();
    _card.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final t = context.axiom;
    final c = t.colors;
    final r = widget.request;
    final external = widget.external;
    const effects = ApprovalCard.effects;
    void answer(ApprovalDecision d) {
      // Focus goes back to the composer the card took it from.
      if (_card.hasFocus) {
        context.findAncestorStateOfType<ChatViewState>()?.composerFocus.requestFocus();
      }
      unawaited(app.agent.approve(r.requestId, d));
    }

    final targets = r.targets.join(', ');
    final meta = [
      'It ${effects[r.effect] ?? r.effect.wire}',
      r.rule != null ? 'rule: ${r.rule}' : 'the policy asks by default',
      if (r.checkpointPlanned) 'a checkpoint is taken first',
      ?widget.origin,
    ].join(' · ');
    final preview = r.preview;
    return Focus(
      focusNode: _card,
      onKeyEvent: (_, e) {
        if (e is KeyDownEvent && e.logicalKey == LogicalKeyboardKey.escape) {
          answer(ApprovalDecision.deny);
          return KeyEventResult.handled;
        }
        return KeyEventResult.ignored;
      },
      child: Semantics(
        container: true,
        explicitChildNodes: true,
        liveRegion: true,
        label: 'Allow ${r.tool}${targets.isEmpty ? '' : ' on $targets'}?',
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: c.accentSoft,
            borderRadius: BorderRadius.circular(Metrics.radiusRaised),
            border: Border.all(color: c.accent.withValues(alpha: 0.5)),
          ),
          child: Padding(
            padding: const EdgeInsets.all(14),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Row(
                  children: [
                    AxiomIcon(AxiomIcons.shield, size: 16, color: c.accentText),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text.rich(
                        TextSpan(
                          children: [
                            const TextSpan(text: 'Allow '),
                            TextSpan(
                              text: r.tool,
                              style: t.type.code.copyWith(color: c.ink),
                            ),
                            if (targets.isNotEmpty) ...[
                              const TextSpan(text: ' on '),
                              TextSpan(
                                text: targets,
                                style: t.type.code.copyWith(color: c.ink),
                              ),
                            ],
                            const TextSpan(text: '?'),
                          ],
                        ),
                        style: t.type.label,
                        maxLines: 3,
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 4),
                Text(meta, style: t.type.small),
                if (external != null)
                  Padding(
                    padding: const EdgeInsets.only(top: 2),
                    child: Text(
                      '$external is an external agent: what it does without asking Axiom is '
                      'outside this check.',
                      style: t.type.small,
                    ),
                  ),
                if (preview != null && preview.startsWith('---'))
                  Padding(
                    padding: const EdgeInsets.only(top: 8),
                    child: ConstrainedBox(
                      constraints: const BoxConstraints(maxHeight: 260),
                      child: SingleChildScrollView(child: DiffBlock(diff: preview)),
                    ),
                  )
                else if (r.argv != null && r.argv!.isNotEmpty)
                  Padding(
                    padding: const EdgeInsets.only(top: 8),
                    child: CodeBlock(code: r.argv!.join(' '), language: 'sh'),
                  ),
                const SizedBox(height: 10),
                Wrap(
                  spacing: 6,
                  runSpacing: 6,
                  children: [
                    AxButton(
                      label: 'Allow once',
                      hint: 'Allow this call — Enter',
                      focusNode: _allow,
                      onPressed: () => answer(ApprovalDecision.once),
                    ),
                    if (r.canRemember)
                      AxButton(
                        label: 'Allow for session',
                        kind: ButtonKind.quiet,
                        hint: 'Allow this tool on these targets for the rest of the session',
                        onPressed: () => answer(ApprovalDecision.session),
                      ),
                    AxButton(
                      label: 'Deny',
                      kind: ButtonKind.quiet,
                      hint: 'Refuse — Esc',
                      onPressed: () => answer(ApprovalDecision.deny),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// One row of a unified diff.
typedef DiffRow = ({String kind, String text, int? old, int? now});

/// A unified diff as rows with line numbers.
({String file, int added, int removed, List<DiffRow> rows}) parseDiff(String diff) {
  final rows = <DiffRow>[];
  var file = '';
  var added = 0, removed = 0, o = 0, n = 0;
  final hunk = RegExp(r'^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@');
  for (final line in diff.split('\n')) {
    if (line.startsWith('+++ ')) {
      file = line.substring(4).replaceFirst(RegExp('^b/'), '');
      continue;
    }
    if (line.startsWith('--- ')) {
      if (file.isEmpty) file = line.substring(4).replaceFirst(RegExp('^a/'), '');
      continue;
    }
    final h = hunk.firstMatch(line);
    if (h != null) {
      o = int.parse(h.group(1)!);
      n = int.parse(h.group(2)!);
      rows.add((kind: 'hunk', text: line, old: null, now: null));
    } else if (line.startsWith('+')) {
      rows.add((kind: 'add', text: line.substring(1), old: null, now: n++));
      added++;
    } else if (line.startsWith('-')) {
      rows.add((kind: 'del', text: line.substring(1), old: o++, now: null));
      removed++;
    } else if (line.startsWith(' ')) {
      rows.add((kind: 'ctx', text: line.substring(1), old: o++, now: n++));
    }
  }
  return (file: file, added: added, removed: removed, rows: rows);
}

class DiffBlock extends StatelessWidget {
  const DiffBlock({super.key, required this.diff});
  final String diff;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final d = parseDiff(diff);
    final code = t.type.code.copyWith(fontSize: 12, color: c.syntax.text, height: 1.45);
    final num = t.type.numeric.copyWith(fontSize: 11, color: c.ink3, height: 1.6);
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: 'Diff of ${d.file}: ${d.added} added, ${d.removed} removed',
      child: Well(
        radius: Metrics.radiusControl,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(10, 6, 10, 4),
              child: Row(
                children: [
                  Expanded(
                    child: Text(d.file, style: t.type.codeSmall.copyWith(color: c.ink)),
                  ),
                  Text('+${d.added}', style: t.type.numeric.copyWith(color: c.okText)),
                  const SizedBox(width: 6),
                  Text('−${d.removed}', style: t.type.numeric.copyWith(color: c.dangerText)),
                ],
              ),
            ),
            for (final r in d.rows)
              if (r.kind == 'hunk')
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 2),
                  child: Text(r.text, style: code.copyWith(color: c.ink3)),
                )
              else
                ColoredBox(
                  color: switch (r.kind) {
                    'add' => c.ok.withValues(alpha: 0.12),
                    'del' => c.danger.withValues(alpha: 0.12),
                    _ => c.ok.withValues(alpha: 0),
                  },
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      SizedBox(
                        width: 34,
                        child: Text('${r.old ?? ''}', style: num, textAlign: TextAlign.right),
                      ),
                      SizedBox(
                        width: 34,
                        child: Text('${r.now ?? ''}', style: num, textAlign: TextAlign.right),
                      ),
                      SizedBox(
                        width: 18,
                        child: Semantics(
                          label: switch (r.kind) {
                            'add' => 'added',
                            'del' => 'removed',
                            _ => 'unchanged',
                          },
                          child: Text(switch (r.kind) {
                            'add' => ' +',
                            'del' => ' −',
                            _ => '',
                          }, style: code.copyWith(color: c.ink2)),
                        ),
                      ),
                      Expanded(child: Text(r.text, style: code)),
                    ],
                  ),
                ),
            const SizedBox(height: 4),
          ],
        ),
      ),
    );
  }
}

class _RestoreConflict extends StatelessWidget {
  const _RestoreConflict({required this.gutter});
  final double gutter;

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final t = context.axiom;
    final rc = app.agent.restoreConflict!;
    return Padding(
      padding: EdgeInsets.fromLTRB(gutter, 0, gutter, 10),
      child: Sheet(
        radius: Metrics.radiusRaised,
        padding: const EdgeInsets.all(14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              '${rc.conflicts.length} file${rc.conflicts.length == 1 ? '' : 's'} changed since the agent wrote them',
              style: t.type.label,
            ),
            const SizedBox(height: 4),
            Text(
              'Restoring checkpoint ${rc.checkpoint} would overwrite them, so nothing was restored. '
              'You can restore the other files and leave these as they are:',
              style: t.type.small,
            ),
            const SizedBox(height: 4),
            for (final x in rc.conflicts)
              Text(
                '${x.path} — ${switch (x.reason) {
                  ConflictReason.modifiedSinceAgent => 'edited',
                  ConflictReason.deletedSinceAgent => 'deleted',
                  ConflictReason.createdSinceAgent => 'created',
                }} after the agent',
                style: t.type.codeSmall,
              ),
            const SizedBox(height: 10),
            Row(
              children: [
                AxButton(
                  label: 'Restore the others',
                  onPressed: () => unawaited(app.agent.restore(rc.checkpoint, skipConflicts: true)),
                ),
                const SizedBox(width: 6),
                AxButton(
                  label: 'Cancel',
                  kind: ButtonKind.quiet,
                  onPressed: app.agent.dismissRestoreConflict,
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _Banner extends StatelessWidget {
  const _Banner({required this.text, required this.gutter, required this.onDismiss});
  final String text;
  final double gutter;
  final VoidCallback onDismiss;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    return Padding(
      padding: EdgeInsets.fromLTRB(gutter, 0, gutter, 8),
      child: Semantics(
        liveRegion: true,
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: t.colors.dangerSoft,
            borderRadius: BorderRadius.circular(Metrics.radiusControl),
          ),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
            child: Row(
              children: [
                Expanded(
                  child: Text(text, style: t.type.small.copyWith(color: t.colors.dangerText)),
                ),
                AxButton(label: 'Dismiss', kind: ButtonKind.quiet, onPressed: onDismiss),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _OneField extends StatefulWidget {
  const _OneField({required this.title, required this.label, required this.initial});
  final String title, label, initial;

  @override
  State<_OneField> createState() => _OneFieldState();
}

class _OneFieldState extends State<_OneField> {
  late final _text = TextEditingController(text: widget.initial);

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
      width: 380,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(widget.title, style: t.type.label),
            const SizedBox(height: 8),
            CallbackShortcuts(
              bindings: {const SingleActivator(LogicalKeyboardKey.enter): go},
              child: AxTextInput(
                label: widget.label,
                controller: _text,
                autofocus: true,
                onSubmitted: (_) => go(),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Approvals not on screen: other conversations', external callers' (MCP clients), and the open
/// conversation's while Chat is hidden. A stack at the window's bottom right.
class ApprovalsStack extends StatelessWidget {
  const ApprovalsStack({super.key, required this.chatShowing, required this.onShow});

  /// A Chat view (the page or the dock) shows the open conversation's approvals.
  final bool chatShowing;
  final ValueChanged<String> onShow;

  @override
  Widget build(BuildContext context) {
    final a = AppScope.of(context).agent;
    return ListenableBuilder(
      listenable: a,
      builder: (context, _) {
        final others = [
          for (final r in a.approvals)
            if (!chatShowing || r.sessionId == null || r.sessionId != a.activeId) r,
        ];
        if (others.isEmpty) return const SizedBox.shrink();
        final t = context.axiom;
        return Semantics(
          container: true,
          explicitChildNodes: true,
          label: 'Approvals waiting',
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              for (final r in others.take(3))
                Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: Sheet(
                    radius: Metrics.radiusRaised,
                    padding: const EdgeInsets.all(4),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        ApprovalCard(key: ValueKey(r.requestId), request: r, origin: _origin(a, r)),
                        if (r.sessionId != null)
                          AxButton(
                            label: 'Show the conversation',
                            kind: ButtonKind.quiet,
                            onPressed: () => onShow(r.sessionId!),
                          ),
                      ],
                    ),
                  ),
                ),
              if (others.length > 3)
                Padding(
                  padding: const EdgeInsets.only(top: 6),
                  child: Text('${others.length - 3} more waiting', style: t.type.small),
                ),
            ],
          ),
        );
      },
    );
  }

  static String _origin(AgentStore a, ApprovalRequest r) {
    if (r.principal.startsWith('mcp:')) return 'asked by ${r.principal.substring(4)} over MCP';
    final s = a.sessions.where((x) => x.sessionId == r.sessionId).firstOrNull;
    return s != null ? 'in “${s.title}”' : 'asked by ${r.principal}';
  }
}

void _openLink(String url) => unawaited(openExternal(url));

/// Loads Chat's providers and conversations after the frame that first shows Chat (the store
/// notifies, which a build may not do).
void _loadSoon(AppState app) => WidgetsBinding.instance.addPostFrameCallback((_) {
  if (app.agent.status == 'idle') unawaited(app.agent.load());
});
