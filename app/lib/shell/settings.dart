import 'package:flutter/widgets.dart';

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
  const SettingsPage({super.key});

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
