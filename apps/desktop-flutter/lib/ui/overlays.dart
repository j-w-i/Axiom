// Tooltips, menus, dialogs and toasts: sheets above the page (ADR-0020 §4). Every overlay is
// keyboard-operable, gives focus back where it came from, and is announced appropriately.

import 'dart:async';

import 'dart:ui' show SemanticsRole;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'controls.dart';
import 'icon_data.dart';
import 'pressable.dart';
import 'surfaces.dart';
import 'theme.dart';
import 'tokens.dart';

// ---- tooltip ------------------------------------------------------------------------------

/// Shows [message] (and the shortcut as keycaps) beside [child] after a hover intent, or while a
/// control inside [child] has keyboard focus. Not announced: the control already carries its name.
class AxTooltip extends StatefulWidget {
  const AxTooltip({super.key, required this.message, required this.child, this.shortcut});
  final String message;
  final String? shortcut;
  final Widget child;

  @override
  State<AxTooltip> createState() => _AxTooltipState();
}

class _AxTooltipState extends State<AxTooltip> {
  final _portal = OverlayPortalController();
  final _link = LayerLink();
  Timer? _timer;

  void _show({bool now = false}) {
    _timer?.cancel();
    if (now) {
      if (!_portal.isShowing) _portal.show();
      return;
    }
    _timer = Timer(context.axiom.motion.intent, () {
      if (mounted && !_portal.isShowing) _portal.show();
    });
  }

  void _hide() {
    _timer?.cancel();
    if (_portal.isShowing) _portal.hide();
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    return CompositedTransformTarget(
      link: _link,
      child: OverlayPortal(
        controller: _portal,
        overlayChildBuilder: (context) => CompositedTransformFollower(
          link: _link,
          targetAnchor: Alignment.bottomCenter,
          followerAnchor: Alignment.topCenter,
          offset: const Offset(0, 6),
          child: Align(
            alignment: Alignment.topLeft,
            child: IgnorePointer(
              child: ExcludeSemantics(
                child: Sheet(
                  radius: Metrics.radiusControl,
                  padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(widget.message, style: t.type.small.copyWith(color: t.colors.ink)),
                      if (widget.shortcut != null) ...[
                        const SizedBox(width: Metrics.s3),
                        Keycaps(widget.shortcut!),
                      ],
                    ],
                  ),
                ),
              ),
            ),
          ),
        ),
        child: MouseRegion(
          onEnter: (_) => _show(),
          onExit: (_) => _hide(),
          child: Focus(
            canRequestFocus: false,
            skipTraversal: true,
            onFocusChange: (has) {
              if (has && FocusManager.instance.highlightMode == FocusHighlightMode.traditional) {
                _show(now: true);
              } else {
                _hide();
              }
            },
            child: Listener(onPointerDown: (_) => _hide(), child: widget.child),
          ),
        ),
      ),
    );
  }
}

// ---- menu ---------------------------------------------------------------------------------

/// One entry of a menu: an item, or a heading.
final class MenuEntry {
  const MenuEntry.item(this.label, this.onSelected, {this.shortcut, this.enabled = true, this.icon})
    : heading = false;
  const MenuEntry.heading(this.label)
    : onSelected = null,
      shortcut = null,
      enabled = false,
      icon = null,
      heading = true;
  final String label;
  final VoidCallback? onSelected;
  final String? shortcut;
  final bool enabled;
  final IconData16? icon;
  final bool heading;
}

/// A trigger that opens a menu below it. Arrows move, Enter chooses, Escape closes and focus goes
/// back to the trigger; a click outside closes.
class AxMenu extends StatefulWidget {
  const AxMenu({
    super.key,
    required this.label,
    required this.entries,
    required this.trigger,
    this.hint,
  });

  /// The trigger's accessible name.
  final String label;
  final List<MenuEntry> Function() entries;
  final Widget Function(BuildContext context, PressState state, bool open) trigger;
  final String? hint;

  @override
  State<AxMenu> createState() => _AxMenuState();
}

class _AxMenuState extends State<AxMenu> {
  final _portal = OverlayPortalController();
  final _link = LayerLink();
  final _triggerFocus = FocusNode(debugLabel: 'menu trigger');
  final _menuScope = FocusScopeNode(debugLabel: 'menu');
  List<MenuEntry> _entries = const [];

  void open() {
    setState(() => _entries = widget.entries());
    _portal.show();
  }

  void close({bool refocus = true}) {
    if (!_portal.isShowing) return;
    _portal.hide();
    setState(() {});
    if (refocus) _triggerFocus.requestFocus();
  }

  @override
  void dispose() {
    _triggerFocus.dispose();
    _menuScope.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return CompositedTransformTarget(
      link: _link,
      child: OverlayPortal(
        controller: _portal,
        overlayChildBuilder: (context) => CompositedTransformFollower(
          link: _link,
          targetAnchor: Alignment.bottomLeft,
          offset: const Offset(0, 4),
          child: Align(
            alignment: Alignment.topLeft,
            child: TapRegion(
              groupId: this,
              onTapOutside: (_) => close(refocus: false),
              child: _MenuSheet(
                label: widget.label,
                entries: _entries,
                scope: _menuScope,
                onClose: close,
              ),
            ),
          ),
        ),
        child: TapRegion(
          groupId: this,
          child: Pressable(
            label: widget.label,
            hint: widget.hint,
            focusNode: _triggerFocus,
            onPressed: () => _portal.isShowing ? close() : open(),
            builder: (context, s) => widget.trigger(context, s, _portal.isShowing),
          ),
        ),
      ),
    );
  }
}

class _MenuSheet extends StatefulWidget {
  const _MenuSheet({
    required this.label,
    required this.entries,
    required this.scope,
    required this.onClose,
  });
  final String label;
  final List<MenuEntry> entries;
  final FocusScopeNode scope;
  final void Function({bool refocus}) onClose;

  @override
  State<_MenuSheet> createState() => _MenuSheetState();
}

class _MenuSheetState extends State<_MenuSheet> {
  late final List<FocusNode> _nodes = [
    for (final e in widget.entries) FocusNode(skipTraversal: e.heading || !e.enabled),
  ];

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _move(0, from: -1));
  }

  @override
  void dispose() {
    for (final n in _nodes) {
      n.dispose();
    }
    super.dispose();
  }

  int get _focused => _nodes.indexWhere((n) => n.hasFocus);

  void _move(int delta, {int? from}) {
    final n = widget.entries.length;
    if (n == 0) return;
    var i = from ?? _focused;
    for (var step = 0; step < n; step++) {
      i = (i + (delta == 0 ? 1 : delta) + n) % n;
      final e = widget.entries[i];
      if (!e.heading && e.enabled) {
        _nodes[i].requestFocus();
        return;
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    return FocusScope(
      node: widget.scope,
      child: Shortcuts(
        shortcuts: const {
          SingleActivator(LogicalKeyboardKey.arrowDown): _MoveIntent(1),
          SingleActivator(LogicalKeyboardKey.arrowUp): _MoveIntent(-1),
          SingleActivator(LogicalKeyboardKey.escape): DismissIntent(),
          SingleActivator(LogicalKeyboardKey.tab): DismissIntent(),
        },
        child: Actions(
          actions: {
            _MoveIntent: CallbackAction<_MoveIntent>(onInvoke: (i) => _move(i.delta)),
            DismissIntent: CallbackAction<DismissIntent>(onInvoke: (_) => widget.onClose()),
          },
          child: Semantics(
            role: SemanticsRole.menu,
            label: widget.label,
            container: true,
            child: ConstrainedBox(
              constraints: const BoxConstraints(minWidth: 180, maxWidth: 360),
              child: Sheet(
                radius: Metrics.radiusRaised,
                padding: const EdgeInsets.all(Metrics.s2),
                child: IntrinsicWidth(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      for (var i = 0; i < widget.entries.length; i++)
                        widget.entries[i].heading
                            ? Padding(
                                padding: const EdgeInsets.fromLTRB(8, 6, 8, 2),
                                // A visual group label: a menu's children are its items.
                                child: ExcludeSemantics(
                                  child: Text(widget.entries[i].label, style: t.type.tiny),
                                ),
                              )
                            : _MenuItem(
                                entry: widget.entries[i],
                                node: _nodes[i],
                                onChosen: () {
                                  widget.onClose();
                                  widget.entries[i].onSelected?.call();
                                },
                              ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _MoveIntent extends Intent {
  const _MoveIntent(this.delta);
  final int delta;
}

class _MenuItem extends StatelessWidget {
  const _MenuItem({required this.entry, required this.node, required this.onChosen});
  final MenuEntry entry;
  final FocusNode node;
  final VoidCallback onChosen;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    return Pressable(
      label: entry.label,
      role: PressRole.menuItem,
      focusNode: node,
      onPressed: entry.enabled ? onChosen : null,
      builder: (context, s) {
        final active = s.hovered || node.hasFocus;
        return DecoratedBox(
          decoration: BoxDecoration(
            color: active ? c.hover : null,
            borderRadius: BorderRadius.circular(Metrics.radiusControl),
          ),
          child: SizedBox(
            height: 26,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 8),
              child: Row(
                children: [
                  if (entry.icon != null) ...[
                    AxiomIcon(entry.icon!, color: s.enabled ? c.ink2 : c.ink3),
                    const SizedBox(width: Metrics.s4),
                  ],
                  Expanded(
                    child: Text(
                      entry.label,
                      style: t.type.body.copyWith(color: s.enabled ? c.ink : c.ink3),
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                  if (entry.shortcut != null) ...[
                    const SizedBox(width: Metrics.s6),
                    Keycaps(entry.shortcut!),
                  ],
                ],
              ),
            ),
          ),
        );
      },
    );
  }
}

// ---- dialog -------------------------------------------------------------------------------

/// Shows a modal dialog: a scrim, a sheet, focus trapped inside, Escape closes, focus returns
/// to where it was. Resolves with the value the dialog pops.
Future<T?> showAxDialog<T>({
  required BuildContext context,
  required String label,
  required WidgetBuilder builder,
  bool dismissible = true,
}) {
  final t = context.axiom;
  return showGeneralDialog<T>(
    context: context,
    barrierDismissible: dismissible,
    barrierLabel: 'Close $label',
    barrierColor: t.colors.scrim,
    transitionDuration: t.motion.medium,
    pageBuilder: (context, _, _) => SafeArea(
      child: Center(
        child: Semantics(
          scopesRoute: true,
          namesRoute: true,
          label: label,
          explicitChildNodes: true,
          child: AxiomTheme(
            data: t,
            child: Sheet(child: Builder(builder: builder)),
          ),
        ),
      ),
    ),
    transitionBuilder: (context, a, _, child) {
      if (!t.motion.animates) return child;
      final curved = CurvedAnimation(parent: a, curve: t.motion.curve);
      return FadeTransition(
        opacity: curved,
        child: ScaleTransition(scale: Tween(begin: 0.98, end: 1.0).animate(curved), child: child),
      );
    },
  );
}

// ---- toasts -------------------------------------------------------------------------------

enum ToastKind { info, ok, error }

/// Hosts toasts at the bottom right. They are announced (status or alert) and never take focus.
class ToastHost extends StatefulWidget {
  const ToastHost({super.key, required this.child});
  final Widget child;

  static ToastHostState of(BuildContext context) =>
      context.findAncestorStateOfType<ToastHostState>()!;

  @override
  State<ToastHost> createState() => ToastHostState();
}

class ToastHostState extends State<ToastHost> {
  final _toasts = <(int, String, ToastKind)>[];
  int _next = 0;

  void show(String message, {ToastKind kind = ToastKind.info, Duration? duration}) {
    final id = _next++;
    setState(() => _toasts.add((id, message, kind)));
    Timer(duration ?? const Duration(seconds: 5), () {
      if (mounted) setState(() => _toasts.removeWhere((x) => x.$1 == id));
    });
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    return Stack(
      children: [
        widget.child,
        Positioned(
          right: Metrics.s6,
          bottom: Metrics.s7 + Metrics.statusBarHeight,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.end,
            mainAxisSize: MainAxisSize.min,
            children: [
              for (final (id, message, kind) in _toasts)
                Padding(
                  key: ValueKey(id),
                  padding: const EdgeInsets.only(top: Metrics.s4),
                  child: Semantics(
                    liveRegion: true,
                    label: '${kind == ToastKind.error ? 'Error: ' : ''}$message',
                    excludeSemantics: true,
                    child: Sheet(
                      radius: Metrics.radiusRaised,
                      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          StatusDot(switch (kind) {
                            ToastKind.info => DotState.running,
                            ToastKind.ok => DotState.ok,
                            ToastKind.error => DotState.error,
                          }, ring: t.colors.sheet),
                          const SizedBox(width: Metrics.s4),
                          ConstrainedBox(
                            constraints: const BoxConstraints(maxWidth: 360),
                            child: Text(message, style: t.type.body),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
            ],
          ),
        ),
      ],
    );
  }
}
