import 'package:flutter/widgets.dart';

import '../ui/commands.dart';
import '../ui/controls.dart';
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
