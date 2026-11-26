import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

/// Everything that has a key of the window's own. One table: for the keys, for the search and for
/// what is said beside a control. Keys are named by what they give on the layout in use.
enum Command {
  start('New session', LogicalKeyboardKey.keyN),
  find('Search', LogicalKeyboardKey.keyK),
  findHere('Find in the conversation', LogicalKeyboardKey.keyF),
  accounts('Accounts', LogicalKeyboardKey.comma),
  settings('Settings', LogicalKeyboardKey.comma, shift: true),
  // Plus is typed with Shift on many keyboards, and is a key of its own on others.
  zoomIn(
    'Zoom in',
    LogicalKeyboardKey.add,
    also: [LogicalKeyboardKey.equal, LogicalKeyboardKey.numpadAdd],
    eitherShift: true,
  ),
  zoomOut('Zoom out', LogicalKeyboardKey.minus, also: [LogicalKeyboardKey.numpadSubtract]),
  zoomReset('Actual size', LogicalKeyboardKey.digit0, also: [LogicalKeyboardKey.numpad0]),
  larger('Larger text', null),
  smaller('Smaller text', null),
  usual('Usual text size', null),
  panel('Show or hide the panel', LogicalKeyboardKey.keyD),
  list('Show or hide the sidebar', LogicalKeyboardKey.keyB),
  next('Next session', LogicalKeyboardKey.tab, control: true),
  previous('Previous session', LogicalKeyboardKey.tab, control: true, shift: true),
  stop('Stop', LogicalKeyboardKey.period),
  model('Change model or effort', LogicalKeyboardKey.keyM, shift: true),
  access('Change access', LogicalKeyboardKey.keyA, shift: true),
  request('Next request', LogicalKeyboardKey.keyJ, shift: true),
  allow('Allow once', LogicalKeyboardKey.keyY, shift: true),
  deny('Deny', LogicalKeyboardKey.keyX, shift: true),
  area('Next area', LogicalKeyboardKey.f6, bare: true),
  areaBack('Previous area', LogicalKeyboardKey.f6, bare: true, shift: true);

  const Command(
    this.label,
    this.key, {
    this.shift = false,
    this.control = false,
    this.bare = false,
    this.also = const [],
    this.eitherShift = false,
  });
  final String label;

  /// Null: it is only found by the search. [also]: where else keyboards have it.
  final LogicalKeyboardKey? key;
  final List<LogicalKeyboardKey> also;

  /// With Shift; with Control on every system; with neither Control nor Command.
  final bool shift, control, bare;

  /// With Shift and without.
  final bool eitherShift;

  static bool get isMac => defaultTargetPlatform == TargetPlatform.macOS;
  static bool get _mac => isMac;

  /// As it is written beside what it does; null if it has none.
  String? get keys {
    final name = switch (key) {
      null => null,
      LogicalKeyboardKey.tab => 'Tab',
      final k => k.keyLabel,
    };
    if (name == null) return null;
    if (!_mac) {
      return [if (!bare) 'Ctrl', if (shift) 'Shift', name].join('+');
    }
    final held = bare
        ? ''
        : control
        ? '⌃'
        : '⌘';
    return '${control ? held : ''}${shift ? '⇧' : ''}${control ? '' : held}$name';
  }

  bool _held(HardwareKeyboard k) {
    if (k.isAltPressed || !eitherShift && k.isShiftPressed != shift) return false;
    final command = !bare && !control && _mac;
    final ctrl = !bare && !command;
    return k.isControlPressed == ctrl && k.isMetaPressed == command;
  }

  /// What was pressed; null: none of these.
  static Command? of(KeyEvent e) {
    // Once for each press: what is held down is not said again.
    if (e is! KeyDownEvent) return null;
    final k = HardwareKeyboard.instance;
    for (final c in values) {
      if ((c.key == e.logicalKey || c.also.contains(e.logicalKey)) && c._held(k)) return c;
    }
    return null;
  }

  /// The session at a place in the list, from 1; null: none of these.
  static int? place(KeyEvent e) {
    if (e is! KeyDownEvent || !Command.start._held(HardwareKeyboard.instance)) return null;
    final at = e.logicalKey.keyId - LogicalKeyboardKey.digit0.keyId;
    return at >= 1 && at <= 9 ? at : null;
  }
}

/// Where what is on the screen says which commands it takes.
class Commands {
  final _does = <Command, List<(Object, VoidCallback)>>{};
  final _areas = <(int, FocusNode)>[];

  bool can(Command c) => _does[c]?.isNotEmpty ?? false;

  /// The one that came last does it.
  bool run(Command c) {
    final all = _does[c];
    if (all == null || all.isEmpty) return false;
    all.last.$2();
    return true;
  }

  /// The keyboard goes to the next part of the window that can take it.
  void area({bool back = false}) {
    final all = [..._areas]..sort((a, b) => a.$1.compareTo(b.$1));
    final n = all.length;
    if (n == 0) return;
    final at = all.indexWhere((a) => a.$2.hasFocus);
    for (var i = 1; i <= n; i++) {
      final to = back ? (at < 0 ? n : at) - i : at + i;
      final first = all[to % n].$2.traversalDescendants.where((d) => d.canRequestFocus);
      if (first.isEmpty) continue;
      first.first.requestFocus();
      return;
    }
  }

  static Commands? maybeOf(BuildContext context) =>
      context.getInheritedWidgetOfExactType<CommandScope>()?.commands;
}

class CommandScope extends InheritedWidget {
  const CommandScope(this.commands, {required super.child, super.key});
  final Commands commands;
  @override
  bool updateShouldNotify(CommandScope old) => !identical(old.commands, commands);
}

/// Takes a command while it is on the screen; with null it does not.
class OnCommand extends StatefulWidget {
  const OnCommand(this.command, this.run, {required this.child, super.key});
  final Command command;
  final VoidCallback? run;
  final Widget child;
  @override
  State<OnCommand> createState() => _OnCommandState();
}

class _OnCommandState extends State<OnCommand> {
  Commands? _all;
  Command? _of;

  void _leave() => _all?._does[_of]?.removeWhere((x) => identical(x.$1, this));

  void _take() {
    _leave();
    final all = _all = Commands.maybeOf(context);
    _of = widget.command;
    if (widget.run == null || all == null) return;
    (all._does[widget.command] ??= []).add((this, () => widget.run?.call()));
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _take();
  }

  @override
  void didUpdateWidget(OnCommand old) {
    super.didUpdateWidget(old);
    if ((old.run == null) != (widget.run == null) || old.command != widget.command) _take();
  }

  @override
  void dispose() {
    _leave();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => widget.child;
}

/// A part of the window the keyboard can be moved to.
class CommandArea extends StatefulWidget {
  const CommandArea(this.order, {required this.child, super.key});
  final int order;
  final Widget child;
  @override
  State<CommandArea> createState() => _CommandAreaState();
}

class _CommandAreaState extends State<CommandArea> {
  final _node = FocusNode(debugLabel: 'area', canRequestFocus: false, skipTraversal: true);
  Commands? _all;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _all?._areas.removeWhere((a) => identical(a.$2, _node));
    _all = Commands.maybeOf(context)?.._areas.add((widget.order, _node));
  }

  @override
  void dispose() {
    _all?._areas.removeWhere((a) => identical(a.$2, _node));
    _node.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) =>
      Focus(focusNode: _node, canRequestFocus: false, skipTraversal: true, child: widget.child);
}
