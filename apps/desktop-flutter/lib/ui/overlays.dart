// Tooltips, menus, dialogs and toasts: sheets above the page (ADR-0020 §4). Every overlay is
// keyboard-operable, gives focus back where it came from, and is announced appropriately. One
// positioner places every anchored overlay inside the window (AUD-013, AUD-051).

import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'controls.dart';
import 'icon_data.dart';
import 'pressable.dart';
import 'surfaces.dart';
import 'theme.dart';
import 'tokens.dart';

// ---- placement ----------------------------------------------------------------------------

/// Where an overlay goes relative to its anchor: below or above it (menus, tooltips, the
/// hover card, completion), or beside it (tooltips on the rail).
enum OverlaySide { below, above, right, left }

/// How the overlay lines up with its anchor across [OverlaySide]: the anchor's start edge
/// (left for below/above, top for right/left), its centre, or its end edge.
enum OverlayAlign { start, center, end }

/// How to place an overlay next to an anchor inside bounds (the window): on the preferred
/// [side], flipped to the opposite side when it does not fit there and the other side has more
/// room, then shifted to stay [margin] inside the bounds. Along the side's axis it gets at most
/// the larger side's room, so a menu taller than the window scrolls instead of running off it.
@immutable
final class OverlayPlacement {
  const OverlayPlacement({
    this.side = OverlaySide.below,
    this.align = OverlayAlign.start,
    this.gap = Metrics.s2,
    this.margin = Metrics.s2,
    this.flip = true,
  });

  final OverlaySide side;
  final OverlayAlign align;

  /// Between the anchor and the overlay.
  final double gap;

  /// Kept free between the overlay and the bounds' edges.
  final double margin;

  /// Whether the overlay may go to the opposite side.
  final bool flip;

  bool get _vertical => side == OverlaySide.below || side == OverlaySide.above;

  /// Room before and after the anchor along the side's axis (above/below, or left/right).
  (double before, double after) _room(Rect anchor, Size bounds) => _vertical
      ? (anchor.top - gap - margin, bounds.height - margin - anchor.bottom - gap)
      : (anchor.left - gap - margin, bounds.width - margin - anchor.right - gap);

  bool get _prefersAfter => side == OverlaySide.below || side == OverlaySide.right;

  /// The constraints for the overlay next to [anchor] within [bounds].
  BoxConstraints constraints(Rect anchor, Size bounds) {
    final (before, after) = _room(anchor, bounds);
    final main = math.max(0.0, flip ? math.max(before, after) : (_prefersAfter ? after : before));
    final cross = math.max(0.0, (_vertical ? bounds.width : bounds.height) - 2 * margin);
    return _vertical
        ? BoxConstraints(maxWidth: cross, maxHeight: main)
        : BoxConstraints(maxWidth: main, maxHeight: cross);
  }

  /// Whether a [child]-sized overlay goes after the anchor (below or right).
  bool goesAfter(Rect anchor, Size bounds, Size child) {
    final (before, after) = _room(anchor, bounds);
    final extent = _vertical ? child.height : child.width;
    final preferred = _prefersAfter ? after : before;
    if (!flip || extent <= preferred) return _prefersAfter;
    final other = _prefersAfter ? before : after;
    final flipIt = extent <= other || other > preferred;
    return flipIt ? !_prefersAfter : _prefersAfter;
  }

  /// The top-left of a [child]-sized overlay next to [anchor] within [bounds].
  Offset position(Rect anchor, Size bounds, Size child) {
    final after = goesAfter(anchor, bounds, child);
    double along(double start, double end, double size) => switch (align) {
      OverlayAlign.start => start,
      OverlayAlign.center => (start + end) / 2 - size / 2,
      OverlayAlign.end => end - size,
    };
    double clampTo(double v, double size, double limit) {
      final hi = limit - margin - size;
      return hi < margin ? margin : v.clamp(margin, hi);
    }

    if (_vertical) {
      final y = after ? anchor.bottom + gap : anchor.top - gap - child.height;
      final x = along(anchor.left, anchor.right, child.width);
      return Offset(clampTo(x, child.width, bounds.width), clampTo(y, child.height, bounds.height));
    }
    final x = after ? anchor.right + gap : anchor.left - gap - child.width;
    final y = along(anchor.top, anchor.bottom, child.height);
    return Offset(clampTo(x, child.width, bounds.width), clampTo(y, child.height, bounds.height));
  }

  @override
  bool operator ==(Object other) =>
      other is OverlayPlacement &&
      other.side == side &&
      other.align == align &&
      other.gap == gap &&
      other.margin == margin &&
      other.flip == flip;

  @override
  int get hashCode => Object.hash(side, align, gap, margin, flip);
}

/// Lays its child out with [OverlayPlacement] next to [anchor], inside the box it is given
/// (an overlay, or a [Stack] the size of the area the overlay must stay in).
class OverlayPlacementDelegate extends SingleChildLayoutDelegate {
  OverlayPlacementDelegate({required this.anchor, this.placement = const OverlayPlacement()});

  /// The anchor, in the coordinates of the box this lays out in.
  final Rect anchor;
  final OverlayPlacement placement;

  @override
  BoxConstraints getConstraintsForChild(BoxConstraints constraints) =>
      placement.constraints(anchor, constraints.biggest);

  @override
  Offset getPositionForChild(Size size, Size childSize) =>
      placement.position(anchor, size, childSize);

  @override
  bool shouldRelayout(OverlayPlacementDelegate old) =>
      old.anchor != anchor || old.placement != placement;
}

/// The anchor's rectangle in the overlay's coordinates, from
/// [OverlayPortal.overlayChildLayoutBuilder]'s layout information.
Rect overlayAnchorRect(OverlayChildLayoutInfo info) =>
    MatrixUtils.transformRect(info.childPaintTransform, Offset.zero & info.childSize);

/// [child] placed next to [anchor] by [placement], filling the box it is given (pointer events
/// outside [child] pass through).
class AnchoredOverlay extends StatelessWidget {
  const AnchoredOverlay({
    super.key,
    required this.anchor,
    required this.child,
    this.placement = const OverlayPlacement(),
  });
  final Rect anchor;
  final OverlayPlacement placement;
  final Widget child;

  @override
  Widget build(BuildContext context) => CustomSingleChildLayout(
    delegate: OverlayPlacementDelegate(anchor: anchor, placement: placement),
    child: child,
  );
}

/// An [OverlayPortal] whose overlay is placed next to [child] by [placement] and stays inside
/// the overlay (the window), following [child] when it moves.
class AnchoredPortal extends StatelessWidget {
  const AnchoredPortal({
    super.key,
    required this.controller,
    required this.overlay,
    required this.child,
    this.placement = const OverlayPlacement(),
  });
  final OverlayPortalController controller;
  final WidgetBuilder overlay;
  final OverlayPlacement placement;
  final Widget child;

  @override
  Widget build(BuildContext context) => OverlayPortal.overlayChildLayoutBuilder(
    controller: controller,
    overlayChildBuilder: (context, info) => AnchoredOverlay(
      anchor: overlayAnchorRect(info),
      placement: placement,
      child: overlay(context),
    ),
    child: child,
  );
}

// ---- tooltip ------------------------------------------------------------------------------

/// Shows [message] (and the shortcut as keycaps) beside [child] after a hover intent, or while a
/// control inside [child] has keyboard focus. Not announced: the control already carries its name.
/// It stays inside the window: centred under its control, flipped above near the bottom,
/// shifted in from the edges; [side] right suits a vertical rail.
class AxTooltip extends StatefulWidget {
  const AxTooltip({
    super.key,
    required this.message,
    required this.child,
    this.shortcut,
    this.side = OverlaySide.below,
  });
  final String message;
  final String? shortcut;
  final Widget child;
  final OverlaySide side;

  @override
  State<AxTooltip> createState() => _AxTooltipState();
}

class _AxTooltipState extends State<AxTooltip> {
  final _portal = OverlayPortalController();
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
    return AnchoredPortal(
      controller: _portal,
      placement: OverlayPlacement(side: widget.side, align: OverlayAlign.center, gap: 6),
      overlay: (context) => IgnorePointer(
        child: ExcludeSemantics(
          child: Sheet(
            radius: Metrics.radiusControl,
            padding: const EdgeInsets.symmetric(horizontal: Metrics.s4, vertical: Metrics.s2),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Flexible(
                  child: Text(
                    widget.message,
                    style: t.type.small.copyWith(color: t.colors.ink),
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                if (widget.shortcut != null) ...[
                  const SizedBox(width: Metrics.s3),
                  Keycaps(widget.shortcut!),
                ],
              ],
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

/// A trigger that opens a menu below it (above when there is more room there, shifted to stay in
/// the window, scrolling when taller than the room). Arrows move, Enter chooses, Escape closes
/// and focus goes back to the trigger; a click outside closes.
class AxMenu extends StatefulWidget {
  const AxMenu({
    super.key,
    required this.label,
    required this.entries,
    required this.trigger,
    this.hint,
    this.skipTraversal = false,
  });

  /// The trigger's accessible name.
  final String label;
  final List<MenuEntry> Function() entries;
  final Widget Function(BuildContext context, PressState state, bool open) trigger;
  final String? hint;

  /// The trigger is out of the Tab order (a row's own menu inside a list with one Tab stop).
  final bool skipTraversal;

  @override
  State<AxMenu> createState() => AxMenuState();
}

/// Public so a context menu (a right-click) can open a menu by its key.
class AxMenuState extends State<AxMenu> {
  final _portal = OverlayPortalController();
  late final _triggerFocus = FocusNode(
    debugLabel: 'menu trigger',
    skipTraversal: widget.skipTraversal,
  );
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
  void didUpdateWidget(AxMenu oldWidget) {
    super.didUpdateWidget(oldWidget);
    _triggerFocus.skipTraversal = widget.skipTraversal;
  }

  @override
  void dispose() {
    _triggerFocus.dispose();
    _menuScope.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnchoredPortal(
      controller: _portal,
      placement: const OverlayPlacement(gap: Metrics.s2),
      overlay: (context) => TapRegion(
        groupId: this,
        onTapOutside: (_) => close(refocus: false),
        child: _MenuSheet(
          label: widget.label,
          entries: _entries,
          scope: _menuScope,
          onClose: close,
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
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _move(0, from: -1);
    });
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
        // A menu taller than the window scrolls: keep the focused item in view.
        final ctx = _nodes[i].context;
        if (ctx != null) {
          unawaited(
            Scrollable.ensureVisible(
              ctx,
              alignmentPolicy: delta < 0
                  ? ScrollPositionAlignmentPolicy.keepVisibleAtStart
                  : ScrollPositionAlignmentPolicy.keepVisibleAtEnd,
            ),
          );
        }
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
                child: SingleChildScrollView(
                  primary: false,
                  child: IntrinsicWidth(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        for (var i = 0; i < widget.entries.length; i++)
                          widget.entries[i].heading
                              ? Padding(
                                  padding: const EdgeInsets.fromLTRB(
                                    Metrics.s4,
                                    Metrics.s3,
                                    Metrics.s4,
                                    Metrics.s1,
                                  ),
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
            height: context.scaled(Metrics.controlHeight),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: Metrics.s4),
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
                    // A disabled item's keys are as faint as its name.
                    Opacity(opacity: s.enabled ? 1 : 0.5, child: Keycaps(entry.shortcut!)),
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
/// to where it was. Resolves with the value the dialog pops. The sheet and the scrim follow the
/// theme in scope while open (a theme or motion change in Settings shows at once; AUD-046). A
/// dialog whose question can change while it is open names itself from [liveLabel] (its first
/// value is [label]).
Future<T?> showAxDialog<T>({
  required BuildContext context,
  required String label,
  required WidgetBuilder builder,
  bool dismissible = true,
  ValueListenable<String>? liveLabel,
}) {
  // Only for a caller outside every AxiomTheme (the route's own context normally has one).
  final opened = context.axiom;
  final motion = opened.motion;
  AxiomThemeData live(BuildContext c) => AxiomTheme.maybeOf(c) ?? opened;
  return showGeneralDialog<T>(
    context: context,
    barrierDismissible: dismissible,
    barrierLabel: 'Close $label',
    // The scrim is drawn by the page below, in the live theme's colour.
    barrierColor: opened.colors.scrim.withValues(alpha: 0),
    transitionDuration: motion.medium,
    pageBuilder: (context, animation, _) {
      final t = live(context);
      Widget named(String name, Widget child) => Semantics(
        scopesRoute: true,
        namesRoute: true,
        label: name,
        explicitChildNodes: true,
        child: child,
      );
      final sheet = Sheet(child: Builder(builder: builder));
      Widget page = SafeArea(
        child: Center(
          child: liveLabel == null
              ? named(label, sheet)
              : ValueListenableBuilder<String>(
                  valueListenable: liveLabel,
                  builder: (_, name, child) => named(name, child!),
                  child: sheet,
                ),
        ),
      );
      if (AxiomTheme.maybeOf(context) == null) page = AxiomTheme(data: t, child: page);
      return Stack(
        fit: StackFit.expand,
        children: [
          IgnorePointer(
            child: FadeTransition(
              opacity: animation,
              child: ColoredBox(color: t.colors.scrim),
            ),
          ),
          page,
        ],
      );
    },
    transitionBuilder: (context, a, _, child) {
      final m = live(context).motion;
      if (!m.animates) return child;
      final curved = CurvedAnimation(parent: a, curve: m.curve);
      // The scrim fades with the page; only Standard scales the sheet (Reduced: fade only).
      if (!m.travels) return FadeTransition(opacity: curved, child: child);
      return FadeTransition(
        opacity: curved,
        child: ScaleTransition(scale: Tween(begin: 0.98, end: 1.0).animate(curved), child: child),
      );
    },
  );
}

// ---- toasts -------------------------------------------------------------------------------

enum ToastKind { info, ok, error }

class _Toast {
  _Toast(this.id, this.message, this.kind, this.controller);
  final int id;
  final String message;
  final ToastKind kind;
  final AnimationController controller;
  Timer? timer;
  bool leaving = false;
}

/// Hosts toasts at the bottom right. They are announced (status or alert) and never take focus.
/// At most [maxShown] show at once (the oldest leaves first); the same message again restarts its
/// toast instead of stacking a copy. They slide and fade in at Standard, fade at Reduced and
/// appear at once with motion Off (motion-system.md §4). A [ToastAvoid] (the approvals stack)
/// pushes them up so they never cover it.
class ToastHost extends StatefulWidget {
  const ToastHost({super.key, required this.child});
  final Widget child;

  static const maxShown = 3;

  static ToastHostState of(BuildContext context) =>
      context.findAncestorStateOfType<ToastHostState>()!;

  static ToastHostState? maybeOf(BuildContext context) =>
      context.findAncestorStateOfType<ToastHostState>();

  @override
  State<ToastHost> createState() => ToastHostState();
}

class ToastHostState extends State<ToastHost> with TickerProviderStateMixin {
  final _toasts = <_Toast>[];
  int _next = 0;
  final _avoid = <Object, double>{};

  /// The messages showing (oldest first), for tests.
  @visibleForTesting
  List<String> get messages => [
    for (final x in _toasts)
      if (!x.leaving) x.message,
  ];

  void show(String message, {ToastKind kind = ToastKind.info, Duration? duration}) {
    final life = duration ?? const Duration(seconds: 5);
    final same = _toasts.where((x) => !x.leaving && x.message == message && x.kind == kind);
    if (same.isNotEmpty) {
      final x = same.first;
      x.timer?.cancel();
      x.timer = Timer(life, () => _dismiss(x));
      return;
    }
    final m = context.axiom.motion;
    final controller = AnimationController(
      vsync: this,
      duration: m.slow,
      reverseDuration: m.medium,
    );
    final toast = _Toast(_next++, message, kind, controller);
    setState(() => _toasts.add(toast));
    if (m.animates) {
      unawaited(controller.forward());
    } else {
      controller.value = 1;
    }
    toast.timer = Timer(life, () => _dismiss(toast));
    final showing = _toasts.where((x) => !x.leaving).toList();
    for (var i = 0; i < showing.length - ToastHost.maxShown; i++) {
      _dismiss(showing[i]);
    }
  }

  void _dismiss(_Toast x) {
    if (!mounted || x.leaving) return;
    x.timer?.cancel();
    x.leaving = true;
    void remove() {
      if (!mounted) return;
      setState(() => _toasts.remove(x));
      x.controller.dispose();
    }

    if (!context.axiom.motion.animates || x.controller.value == 0) {
      remove();
      return;
    }
    setState(() {});
    unawaited(x.controller.reverse().whenComplete(remove));
  }

  /// [owner] occupies [height] px above the bottom-right corner's base line (0 to release).
  void avoid(Object owner, double height) {
    final old = _avoid[owner];
    if (height <= 0) {
      if (_avoid.remove(owner) != null) _rebuildSoon();
    } else if (old != height) {
      _avoid[owner] = height;
      _rebuildSoon();
    }
  }

  void _rebuildSoon() {
    // Called from a layout callback: rebuild after this frame.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) setState(() {});
    });
  }

  @override
  void dispose() {
    for (final x in _toasts) {
      x.timer?.cancel();
      x.controller.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final lift = _avoid.values.fold(0.0, math.max);
    return Stack(
      children: [
        widget.child,
        Positioned(
          right: Metrics.s6,
          bottom: Metrics.s7 + Metrics.statusBarHeight + (lift > 0 ? lift + Metrics.s4 : 0),
          // Toasts only announce: a click goes to what is under them.
          child: IgnorePointer(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.end,
              mainAxisSize: MainAxisSize.min,
              children: [
                for (final x in _toasts)
                  _ToastView(key: ValueKey(x.id), toast: x, travel: t.motion.travels ? 10 : 0),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

class _ToastView extends StatelessWidget {
  const _ToastView({super.key, required this.toast, required this.travel});
  final _Toast toast;
  final double travel;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final a = CurvedAnimation(parent: toast.controller, curve: t.motion.curve);
    return AnimatedBuilder(
      animation: a,
      builder: (context, child) => Opacity(
        opacity: a.value,
        // Announced as it enters, not once it is opaque.
        alwaysIncludeSemantics: true,
        child: Transform.translate(offset: Offset(0, travel * (1 - a.value)), child: child),
      ),
      child: Padding(
        padding: const EdgeInsets.only(top: Metrics.s4),
        child: Semantics(
          liveRegion: !toast.leaving,
          label: '${toast.kind == ToastKind.error ? 'Error: ' : ''}${toast.message}',
          excludeSemantics: true,
          child: Sheet(
            radius: Metrics.radiusRaised,
            padding: const EdgeInsets.symmetric(horizontal: Metrics.s5, vertical: Metrics.s4),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                StatusDot(switch (toast.kind) {
                  ToastKind.info => DotState.running,
                  ToastKind.ok => DotState.ok,
                  ToastKind.error => DotState.error,
                }, ring: t.colors.sheet),
                const SizedBox(width: Metrics.s4),
                ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 360),
                  child: Text(toast.message, style: t.type.body),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Reports its child's height to the [ToastHost] so toasts stack above it rather than over it
/// (the approvals stack at the window's bottom right; AUD-057).
class ToastAvoid extends SingleChildRenderObjectWidget {
  const ToastAvoid({super.key, required super.child});

  @override
  RenderObject createRenderObject(BuildContext context) =>
      _RenderToastAvoid(ToastHost.maybeOf(context));

  @override
  void updateRenderObject(BuildContext context, RenderObject renderObject) =>
      (renderObject as _RenderToastAvoid).host = ToastHost.maybeOf(context);
}

class _RenderToastAvoid extends RenderProxyBox {
  _RenderToastAvoid(this.host);
  ToastHostState? host;

  @override
  void performLayout() {
    super.performLayout();
    host?.avoid(this, size.height);
  }

  @override
  void detach() {
    host?.avoid(this, 0);
    super.detach();
  }
}

/// A question with its answers (`(id, label)`; `cancel` is quiet, `discard` and `delete` are
/// dangerous). Pops the chosen id. The [defaultAction] (an id; the first answer when null) is
/// focused when the dialog opens, so Enter chooses it, and it carries the primary look (a
/// dangerous default keeps the danger look). The answers keep their order.
class AxQuestion extends StatelessWidget {
  const AxQuestion({
    super.key,
    required this.title,
    required this.detail,
    required this.actions,
    this.defaultAction,
  });
  final String title;
  final String detail;
  final List<(String, String)> actions;

  /// The id of the answer focused by default; the first answer's when null.
  final String? defaultAction;

  static bool _dangerous(String id) => id == 'discard' || id == 'delete';

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final first = defaultAction != null && actions.any((a) => a.$1 == defaultAction)
        ? defaultAction!
        : (actions.isEmpty ? '' : actions.first.$1);
    return SizedBox(
      width: 460,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(Metrics.s6, Metrics.s6, Metrics.s6, Metrics.s6),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Semantics(header: true, child: Text(title, style: t.type.title)),
            const SizedBox(height: Metrics.s4),
            Text(detail, style: t.type.small),
            const SizedBox(height: Metrics.s6),
            Wrap(
              alignment: WrapAlignment.end,
              spacing: Metrics.s3,
              runSpacing: Metrics.s3,
              children: [
                for (final (id, label) in actions)
                  AxButton(
                    label: label,
                    autofocus: id == first,
                    kind: _dangerous(id)
                        ? ButtonKind.danger
                        : id == first
                        ? ButtonKind.primary
                        : (id == 'cancel' ? ButtonKind.quiet : ButtonKind.standard),
                    onPressed: () => Navigator.of(context).pop(id),
                  ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
