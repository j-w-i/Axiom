import 'dart:async';

import 'package:flutter/widgets.dart';

import '../host/proto.dart';
import '../patterns/session_panel.dart';
import '../transcript/safe_text.dart';
import '../ui/commands.dart';
import '../ui/controls.dart';
import '../ui/fonts.dart';
import '../ui/overlay.dart';
import '../ui/prefs.dart';
import '../ui/tokens.dart';
import 'start.dart';

/// What a person chooses about the window.
class SettingsPage extends StatelessWidget {
  const SettingsPage({super.key, this.limits});

  /// Asks the host for what it keeps everything to; null without a host.
  final Future<List<HostLimit>> Function()? limits;

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final p = PrefsScope.of(context), all = PrefsScope.maybeOf(context);
    void set(Prefs v) => all?.value = v;
    Widget row(String name, String? words, Widget control) => Row(
      children: [
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(name, style: t.type.uiMedium),
              if (words != null) Text(words, style: t.type.label.ink(c.ink3)),
            ],
          ),
        ),
        const SizedBox(width: 16),
        control,
      ],
    );
    Widget choose<T>(String label, List<AxMenuItem<T>> items, T value, ValueChanged<T> f) =>
        AxSelect<T>(label: label, named: false, items: items, value: value, onChanged: f);
    return PageFrame([
      Card([
        row(
          'Appearance',
          null,
          choose(
            'Appearance',
            const [
              AxMenuItem(Look.system, 'System'),
              AxMenuItem(Look.light, 'Light'),
              AxMenuItem(Look.dark, 'Dark'),
            ],
            p.look,
            (v) => set(p.copyWith(look: v)),
          ),
        ),
        row(
          'Zoom',
          'Everything in the window. ${Command.zoomIn.keys}, ${Command.zoomOut.keys}, ${Command.zoomReset.keys}',
          choose(
            'Zoom',
            [for (final x in Prefs.zooms) AxMenuItem(x, '${(x * 100).round()}%')],
            p.zoom,
            (v) => set(p.copyWith(zoom: v)),
          ),
        ),
        row(
          'Text size',
          'Icons grow with it.',
          choose(
            'Text size',
            [for (final x in Prefs.sizes) AxMenuItem(x, '${(x * 100).round()}%')],
            p.text,
            (v) => set(p.copyWith(text: v)),
          ),
        ),
        row(
          'Density',
          null,
          choose(
            'Density',
            [
              for (final d in AxDensity.values)
                AxMenuItem(d, '${d.name[0].toUpperCase()}${d.name.substring(1)}'),
            ],
            p.density,
            (v) => set(p.copyWith(density: v)),
          ),
        ),
        row(
          'Code font',
          p.code.isEmpty
              ? 'Of code, diffs, paths and commands. Any that is installed.'
              : hasFamily(p.code)
              ? 'Of code, diffs, paths and commands.'
              : '${oneLine(literal(p.code), 60)} is not installed. ${AxType.literals} is used.',
          SizedBox(
            width: 170,
            child: _CodeFont(
              value: p.code,
              onChanged: (v) => set(p.copyWith(code: v)),
            ),
          ),
        ),
        row(
          'Send with Enter',
          p.enterSends
              ? 'Shift+Enter starts a new line.'
              : 'Enter starts a new line. ${Command.isMac ? '⌘Enter' : 'Ctrl+Enter'} sends.',
          AxSwitch(
            label: 'Send with Enter',
            value: p.enterSends,
            onChanged: (v) => set(p.copyWith(enterSends: v)),
          ),
        ),
      ]),
      const SizedBox(height: 24),
      Semantics(header: true, child: Text('Advanced', style: t.type.title)),
      const SizedBox(height: 12),
      Card([
        row(
          'Choose the harness',
          'Which program runs an agent, beside its model. Otherwise it follows from the model.',
          AxSwitch(
            label: 'Choose the harness',
            value: p.harnesses,
            onChanged: (v) => set(p.copyWith(harnesses: v)),
          ),
        ),
      ]),
      if (limits case final f?) ...[const SizedBox(height: 18), _Limits(f)],
    ]);
  }
}

/// Taken when it is written, and not letter by letter.
class _CodeFont extends StatefulWidget {
  const _CodeFont({required this.value, required this.onChanged});
  final String value;
  final ValueChanged<String> onChanged;
  @override
  State<_CodeFont> createState() => _CodeFontState();
}

class _CodeFontState extends State<_CodeFont> {
  late final _name = TextEditingController(text: widget.value);
  final _focus = FocusNode(debugLabel: 'code font');

  @override
  void initState() {
    super.initState();
    _focus.addListener(() {
      if (!_focus.hasFocus) _take();
    });
  }

  void _take() {
    final name = Prefs.family(_name.text);
    if (name != widget.value) widget.onChanged(name);
  }

  @override
  void dispose() {
    // The page was left from the middle of it. Nothing is told while the window is being built.
    final name = Prefs.family(_name.text), told = widget.onChanged;
    if (name != widget.value) WidgetsBinding.instance.addPostFrameCallback((_) => told(name));
    _focus.dispose();
    _name.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AxField(
    label: 'Code font',
    placeholder: AxType.literals,
    controller: _name,
    focusNode: _focus,
    mono: true,
    sunken: true,
    onSubmitted: (_) => _take(),
  );
}

/// What the host keeps everything to, to be read. It is asked whenever somebody looks.
class _Limits extends StatefulWidget {
  const _Limits(this.ask);
  final Future<List<HostLimit>> Function() ask;
  @override
  State<_Limits> createState() => _LimitsState();
}

class _LimitsState extends State<_Limits> {
  bool _open = false;
  List<HostLimit>? _all;
  String? _problem;

  Future<void> _fold() async {
    setState(() {
      _open = !_open;
      _problem = null;
    });
    if (!_open) return;
    try {
      final all = await widget.ask();
      if (mounted) setState(() => _all = all);
    } on Object catch (e) {
      if (mounted) setState(() => _problem = wordsOf(e));
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final all = _all ?? const <HostLimit>[];
    final kinds = {for (final l in all) l.of};
    Widget note(String words, {bool failed = false}) => Semantics(
      liveRegion: true,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(AxSpace.card, 0, AxSpace.card, 4),
        child: Text(words, style: t.type.label.ink(failed ? c.faultText : c.ink3)),
      ),
    );
    Widget row(HostLimit l) => MergeSemantics(
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: AxSpace.card, vertical: 5),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Expanded(child: Text(oneLine(literal(l.what), 200), style: t.type.ui)),
            const SizedBox(width: 16),
            ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 160),
              child: Text(l.amount, textAlign: TextAlign.end, style: t.type.ui.ink(c.ink3)),
            ),
          ],
        ),
      ),
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        PanelHeading('Limits', null, folded: !_open, onFold: () => unawaited(_fold())),
        if (_open) ...[
          if (_problem case final p?)
            note(visible(p), failed: true)
          else if (_all == null)
            note('Asking the host')
          else
            note('What the host keeps agents to. They are not changed here.'),
          if (_problem == null)
            for (final kind in kinds) ...[
              const SizedBox(height: 8),
              PanelGroup(oneLine(literal(kind), 80), null, [
                for (final l in all)
                  if (l.of == kind) row(l),
              ]),
            ],
        ],
      ],
    );
  }
}
