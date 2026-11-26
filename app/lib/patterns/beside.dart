import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';

/// What is being read, and a panel beside it on the same canvas. The room between them is the handle.
/// The panel is slim while it only lists, and wide while something in it is read. Where there is no
/// room for both, however slim, the panel stands in front, and what is behind it is kept.
class Beside extends StatefulWidget {
  const Beside({
    required this.main,
    required this.panel,
    required this.wide,
    required this.sizes,
    required this.onSizes,
    super.key,
    this.called = 'the panel',
  });
  final Widget main;
  final Widget? panel;
  final String called;
  final bool wide;

  /// How wide it is while slim, and its share of the width while wide.
  final (double, double) sizes;
  final ValueChanged<(double, double)> onSizes;

  static const gap = 8.0, slimmest = 260.0, widest = 400.0, least = 360.0, rest = 360.0;
  static const usual = (304.0, .5);

  /// Whether both have room in [width].
  static bool fits(double width) => width >= slimmest + gap + rest;

  @override
  State<Beside> createState() => _BesideState();
}

class _BesideState extends State<Beside> {
  /// The panel is the same one in front and beside: what is chosen in it is kept across the width.
  final _panel = GlobalKey(debugLabel: 'panel');

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, box) {
      final Beside(:main, :wide, :sizes, :onSizes, :called) = widget;
      final panel = widget.panel == null ? null : KeyedSubtree(key: _panel, child: widget.panel!);
      const gap = Beside.gap, rest = Beside.rest, least = Beside.least;
      const slimmest = Beside.slimmest, widest = Beside.widest;
      final w = box.maxWidth, both = Beside.fits(w);
      final over = panel != null && !both;
      final most = w - rest - gap;
      final has = !both
          ? 0.0
          : wide
          ? (w * sizes.$2).clamp(most < least ? most : least, most)
          : sizes.$1.clamp(slimmest, most < widest ? most : widest);
      final little = most < least ? most : least;
      void set(double v) => onSizes(
        wide ? (sizes.$1, v.clamp(little, most) / w) : (v.clamp(slimmest, widest), sizes.$2),
      );
      return Row(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Expanded(
            key: const ValueKey('main'),
            child: Stack(
              children: [
                Positioned.fill(
                  child: Offstage(
                    offstage: over,
                    // What cannot be seen does not ask for frames.
                    child: TickerMode(
                      enabled: !over,
                      child: ExcludeFocus(excluding: over, child: main),
                    ),
                  ),
                ),
                if (panel != null && over) Positioned.fill(child: panel),
              ],
            ),
          ),
          if (panel != null && both) ...[
            _Handle(label: 'Width of $called', width: has, share: has / w, onTo: set),
            SizedBox(width: has, child: panel),
          ],
        ],
      );
    },
  );
}

/// A canvas: what is on it is cut where it ends.
class AxCanvas extends StatelessWidget {
  const AxCanvas({required this.child, super.key});
  final Widget child;
  @override
  Widget build(BuildContext context) => Surface(
    Ax.of(context).colors.canvas,
    radius: AxRadius.canvas,
    child: ClipPath(clipper: const ContinuousClip(AxRadius.canvas), child: child),
  );
}

/// Dragged, or moved by the arrow keys.
class _Handle extends StatefulWidget {
  const _Handle({
    required this.label,
    required this.width,
    required this.share,
    required this.onTo,
  });
  final String label;

  /// Of the pane.
  final double width, share;
  final ValueChanged<double> onTo;
  @override
  State<_Handle> createState() => _HandleState();
}

class _HandleState extends State<_Handle> {
  bool _over = false, _held = false, _focus = false;
  static const _step = 24.0;

  /// Where it was taken, and how far it has been moved since: what was set last may not be drawn yet.
  double _from = 0, _moved = 0;
  void _by(double d) => widget.onTo(widget.width - d);
  @override
  Widget build(BuildContext context) {
    final c = Ax.of(context).colors;
    final lit = _over || _held || _focus;
    return Semantics(
      slider: true,
      label: widget.label,
      value: '${(widget.share * 100).round()}%',
      increasedValue: 'wider',
      decreasedValue: 'narrower',
      onIncrease: () => _by(-_step),
      onDecrease: () => _by(_step),
      child: FocusableActionDetector(
        mouseCursor: SystemMouseCursors.resizeColumn,
        onShowHoverHighlight: (v) => setState(() => _over = v),
        onShowFocusHighlight: (v) => setState(() => _focus = v),
        shortcuts: const {
          SingleActivator(LogicalKeyboardKey.arrowLeft): _By(-_step),
          SingleActivator(LogicalKeyboardKey.arrowRight): _By(_step),
        },
        actions: {
          _By: CallbackAction<_By>(
            onInvoke: (i) {
              _by(i.d);
              return null;
            },
          ),
        },
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          excludeFromSemantics: true,
          onHorizontalDragStart: (_) => setState(() {
            _held = true;
            _from = widget.width;
            _moved = 0;
          }),
          onHorizontalDragUpdate: (d) => widget.onTo(_from - (_moved += d.delta.dx)),
          onHorizontalDragEnd: (_) => setState(() => _held = false),
          onHorizontalDragCancel: () => setState(() => _held = false),
          child: SizedBox(
            width: Beside.gap,
            child: Center(
              child: SizedBox(
                width: 3,
                height: 36,
                child: Ground(
                  lit ? (_focus ? c.focus : c.ink4) : null,
                  radius: 1.5,
                  child: const SizedBox.expand(),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _By extends Intent {
  const _By(this.d);
  final double d;
}

/// Shows the panel and puts it away. It stands over the panel, at the right, as the sidebar's does over
/// the sidebar, at the left.
class PanelKey extends StatelessWidget {
  const PanelKey({required this.open, required this.onPressed, super.key, this.shortcut});
  final bool open;

  /// Null where there is no panel to be shown.
  final VoidCallback? onPressed;
  final String? shortcut;
  @override
  Widget build(BuildContext context) => AxButton.icon(
    AxIcon.panel,
    label: open ? 'Hide the panel' : 'Show the panel',
    shortcut: shortcut,
    tone: AxTone.quiet,
    iconFilled: open,
    onPressed: onPressed,
  );
}
