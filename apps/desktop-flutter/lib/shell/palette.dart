// The command palette (Mod+K, Mod+Shift+P): a sheet at the top with a query, the commands
// ranked fuzzily with their shortcuts; ↑/↓ move, Enter runs, Escape closes and focus returns.

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../app/app_state.dart';
import '../ui/controls.dart';
import '../ui/fuzzy.dart';
import '../ui/icon_data.dart';
import '../ui/icons.dart';
import '../ui/pressable.dart';
import '../ui/surfaces.dart';
import '../ui/text_input.dart';
import '../ui/theme.dart';
import '../ui/tokens.dart';

class Palette extends StatefulWidget {
  const Palette({
    super.key,
    required this.commands,
    required this.onClose,
    this.seed = '',
    this.load,
    this.label = 'Command palette',
    this.inputLabel = 'Run a command',
    this.placeholder = 'Type a command',
    this.empty = 'No matching command',
  });
  final List<Command> commands;
  final VoidCallback onClose;
  final String seed;

  /// Items fetched when the palette opens (Go to File's file list); [commands] shows meanwhile.
  final Future<List<Command>> Function()? load;
  final String label;
  final String inputLabel;
  final String placeholder;
  final String empty;

  @override
  State<Palette> createState() => _PaletteState();
}

class _PaletteState extends State<Palette> {
  late final _query = TextEditingController(text: widget.seed);
  final _focus = FocusNode(debugLabel: 'palette query');
  final _scroll = ScrollController();
  FocusNode? _returnTo;
  int _active = 0;
  List<Command>? _loaded;
  bool _loading = false;

  @override
  void initState() {
    super.initState();
    _returnTo = FocusManager.instance.primaryFocus;
    if (widget.load != null) {
      _loading = true;
      widget.load!().then((items) {
        if (!mounted) return;
        setState(() {
          _loaded = items;
          _loading = false;
        });
      });
    }
    _query.addListener(() => setState(() => _active = 0));
    WidgetsBinding.instance.addPostFrameCallback((_) => _focus.requestFocus());
  }

  @override
  void dispose() {
    _query.dispose();
    _focus.dispose();
    _scroll.dispose();
    super.dispose();
  }

  List<({Command item, FuzzyMatch match})> get _results => rankFuzzy(
    (_loaded ?? widget.commands).where((c) => c.isEnabled),
    _query.text,
    (c) => c.title,
    limit: 60,
  );

  void _close() {
    widget.onClose();
    final back = _returnTo;
    if (back != null && back.context != null) back.requestFocus();
  }

  void _run(Command c) {
    _close();
    c.run();
  }

  void _move(int d, int n) {
    if (n == 0) return;
    setState(() => _active = (_active + d + n) % n);
    const extent = 30.0;
    final top = _active * extent;
    if (!_scroll.hasClients) return;
    if (top < _scroll.offset) {
      _scroll.jumpTo(top);
    } else if (top + extent > _scroll.offset + _scroll.position.viewportDimension) {
      _scroll.jumpTo(top + extent - _scroll.position.viewportDimension);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final results = _results;
    if (_active >= results.length) _active = results.isEmpty ? 0 : results.length - 1;
    return Positioned.fill(
      child: Stack(
        children: [
          // A click outside closes it.
          Positioned.fill(
            child: Semantics(
              label: 'Close ${widget.label}',
              child: GestureDetector(
                behavior: HitTestBehavior.opaque,
                onTap: _close,
                child: const SizedBox.expand(),
              ),
            ),
          ),
          Align(
            alignment: const Alignment(0, -0.82),
            child: Shortcuts(
              shortcuts: const {
                SingleActivator(LogicalKeyboardKey.arrowDown): _MoveIntent(1),
                SingleActivator(LogicalKeyboardKey.arrowUp): _MoveIntent(-1),
                SingleActivator(LogicalKeyboardKey.escape): DismissIntent(),
                SingleActivator(LogicalKeyboardKey.enter): ActivateIntent(),
              },
              child: Actions(
                actions: {
                  _MoveIntent: CallbackAction<_MoveIntent>(
                    onInvoke: (i) => _move(i.delta, results.length),
                  ),
                  DismissIntent: CallbackAction<DismissIntent>(onInvoke: (_) => _close()),
                  ActivateIntent: CallbackAction<ActivateIntent>(
                    onInvoke: (_) => results.isEmpty ? null : _run(results[_active].item),
                  ),
                },
                child: Semantics(
                  scopesRoute: true,
                  namesRoute: true,
                  label: widget.label,
                  explicitChildNodes: true,
                  child: SizedBox(
                    width: 600,
                    child: Sheet(
                      padding: const EdgeInsets.all(Metrics.s3),
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          AxTextInput(
                            label: widget.inputLabel,
                            controller: _query,
                            focusNode: _focus,
                            placeholder: widget.placeholder,
                            leading: AxiomIcon(AxiomIcons.search, size: 14, color: c.ink3),
                          ),
                          const SizedBox(height: Metrics.s3),
                          ConstrainedBox(
                            constraints: const BoxConstraints(maxHeight: 360),
                            child: results.isEmpty
                                ? Padding(
                                    padding: const EdgeInsets.all(Metrics.s4),
                                    child: _loading
                                        ? const Align(
                                            alignment: Alignment.centerLeft,
                                            child: Spinner(label: 'Listing files'),
                                          )
                                        : Text(widget.empty, style: t.type.small),
                                  )
                                : ListView.builder(
                                    controller: _scroll,
                                    shrinkWrap: true,
                                    itemExtent: 30,
                                    itemCount: results.length,
                                    itemBuilder: (context, i) {
                                      final (:item, :match) = results[i];
                                      return _PaletteRow(
                                        command: item,
                                        match: match,
                                        active: i == _active,
                                        onPressed: () => _run(item),
                                      );
                                    },
                                  ),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _MoveIntent extends Intent {
  const _MoveIntent(this.delta);
  final int delta;
}

class _PaletteRow extends StatelessWidget {
  const _PaletteRow({
    required this.command,
    required this.match,
    required this.active,
    required this.onPressed,
  });
  final Command command;
  final FuzzyMatch match;
  final bool active;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final spans = <TextSpan>[];
    var at = 0;
    for (final (s, e) in match.ranges) {
      if (s > at) spans.add(TextSpan(text: command.title.substring(at, s)));
      spans.add(
        TextSpan(
          text: command.title.substring(s, e),
          style: TextStyle(color: c.accentText, fontWeight: FontWeight.w600),
        ),
      );
      at = e;
    }
    if (at < command.title.length) spans.add(TextSpan(text: command.title.substring(at)));
    return Pressable(
      label: command.title,
      selected: active,
      onPressed: onPressed,
      builder: (context, s) => DecoratedBox(
        decoration: BoxDecoration(
          color: active ? c.raised : (s.hovered ? c.hover : null),
          borderRadius: BorderRadius.circular(Metrics.radiusControl),
        ),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 10),
          child: Row(
            children: [
              if (command.category != null) ...[
                Text(command.category!, style: t.type.tiny),
                const SizedBox(width: 8),
              ],
              Expanded(
                child: Text.rich(
                  TextSpan(children: spans),
                  style: t.type.body,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              if (command.shortcut != null) Keycaps(command.shortcut!),
            ],
          ),
        ),
      ),
    );
  }
}
