// Settings (Mod+,): Appearance (theme, motion), Editor (font size, wrapping), Workspace
// (reopen the last folder), About (versions, the data directory). Values are the daemon's
// settings; they apply as soon as they are chosen.

import 'dart:async';

import 'package:flutter/widgets.dart';

import '../app/app_state.dart';
import '../daemon/client.dart';
import '../state/settings.dart';
import '../ui/controls.dart';
import '../ui/tabs.dart';
import '../ui/theme.dart';
import '../ui/tokens.dart';

class SettingsPage extends StatefulWidget {
  const SettingsPage({super.key, required this.app});
  final AppState app;

  @override
  State<SettingsPage> createState() => _SettingsPageState();
}

class _SettingsPageState extends State<SettingsPage> {
  String _section = 'appearance';

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final s = widget.app.settings;
    return SizedBox(
      width: 640,
      height: 440,
      child: ListenableBuilder(
        listenable: s,
        builder: (context, _) => Padding(
          padding: const EdgeInsets.fromLTRB(20, 14, 20, 18),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Semantics(header: true, child: Text('Settings', style: t.type.title)),
                  const Spacer(),
                  AxButton(
                    label: 'Close',
                    kind: ButtonKind.quiet,
                    hint: 'Close — Esc',
                    onPressed: () => Navigator.of(context).pop(),
                  ),
                ],
              ),
              const SizedBox(height: Metrics.s3),
              AxTabs(
                label: 'Settings sections',
                tabs: const [
                  TabItem(id: 'appearance', label: 'Appearance'),
                  TabItem(id: 'editor', label: 'Editor'),
                  TabItem(id: 'workspace', label: 'Workspace'),
                  TabItem(id: 'about', label: 'About'),
                ],
                selected: _section,
                onSelect: (id) => setState(() => _section = id),
              ),
              const SizedBox(height: Metrics.s6),
              if (s.error.isNotEmpty)
                Padding(
                  padding: const EdgeInsets.only(bottom: Metrics.s4),
                  child: Text(s.error, style: t.type.small.copyWith(color: t.colors.dangerText)),
                ),
              Expanded(child: SingleChildScrollView(child: _body(context, s))),
            ],
          ),
        ),
      ),
    );
  }

  Widget _row(BuildContext context, String label, Widget control, {String? note}) {
    final t = context.axiom;
    return Padding(
      padding: const EdgeInsets.only(bottom: Metrics.s5),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 160,
            child: Padding(
              padding: const EdgeInsets.only(top: 4),
              child: Text(label, style: t.type.label),
            ),
          ),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                control,
                if (note != null)
                  Padding(
                    padding: const EdgeInsets.only(top: 4),
                    child: Text(note, style: t.type.tiny),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _choice<T>(String group, List<(T, String)> options, T value, ValueChanged<T> onChoose) =>
      Wrap(
        spacing: Metrics.s2,
        children: [
          for (final (v, label) in options)
            _ChoiceButton(
              label: label,
              group: group,
              selected: v == value,
              onPressed: () => onChoose(v),
            ),
        ],
      );

  Widget _body(BuildContext context, SettingsStore s) {
    final t = context.axiom;
    switch (_section) {
      case 'appearance':
        return Column(
          children: [
            _row(
              context,
              'Theme',
              _choice(
                'Theme',
                const [
                  (ThemeSetting.system, 'System'),
                  (ThemeSetting.light, 'Light'),
                  (ThemeSetting.dark, 'Dark'),
                ],
                s.theme,
                (v) => unawaited(s.setValue('appearance.theme', v.name)),
              ),
            ),
            _row(
              context,
              'Motion',
              _choice(
                'Motion',
                const [
                  (MotionMode.standard, 'Standard'),
                  (MotionMode.reduced, 'Reduced'),
                  (MotionMode.off, 'Off'),
                ],
                s.motion,
                (v) => unawaited(s.setValue('appearance.motion', v.name)),
              ),
              note: 'Reduced keeps short fades; Off removes every animation.',
            ),
          ],
        );
      case 'editor':
        return Column(
          children: [
            _row(
              context,
              'Font size',
              Row(
                children: [
                  AxButton(
                    label: 'Smaller',
                    kind: ButtonKind.quiet,
                    onPressed: s.editorFontSize <= 8
                        ? null
                        : () => unawaited(s.setValue('editor.fontSize', s.editorFontSize - 1)),
                  ),
                  SizedBox(
                    width: 44,
                    child: Center(
                      child: Semantics(
                        label: 'Font size ${s.editorFontSize}',
                        child: Text(
                          '${s.editorFontSize}',
                          style: t.type.numeric.copyWith(color: t.colors.ink),
                        ),
                      ),
                    ),
                  ),
                  AxButton(
                    label: 'Larger',
                    kind: ButtonKind.quiet,
                    onPressed: s.editorFontSize >= 40
                        ? null
                        : () => unawaited(s.setValue('editor.fontSize', s.editorFontSize + 1)),
                  ),
                ],
              ),
            ),
            _row(
              context,
              'Wrapping',
              AxToggle(
                label: 'Wrap long lines',
                value: s.editorWordWrap,
                onChanged: (v) => unawaited(s.setValue('editor.wordWrap', v)),
              ),
            ),
          ],
        );
      case 'workspace':
        return _row(
          context,
          'On start',
          AxToggle(
            label: 'Reopen the last folder',
            value: s.reopenLast,
            onChanged: (v) => unawaited(s.setValue('workspace.reopenLast', v)),
          ),
        );
      default:
        final d = widget.app.client.state.value;
        String row(String k, String v) => '$k  $v';
        final lines = [
          row('Shell', 'Axiom (Flutter) 0.1.0'),
          if (d is DaemonReady) row('Backend', 'axiomd ${d.serverVersion}'),
          if (d is DaemonReady) row('Protocol', d.protocolVersion),
          row('Data directory', widget.app.args.dataDir ?? 'the platform default'),
          row('Layout file', widget.app.prefs.path),
        ];
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            for (final l in lines)
              Padding(
                padding: const EdgeInsets.only(bottom: 6),
                child: Text(l, style: t.type.codeSmall),
              ),
          ],
        );
    }
  }
}

class _ChoiceButton extends StatelessWidget {
  const _ChoiceButton({
    required this.label,
    required this.group,
    required this.selected,
    required this.onPressed,
  });
  final String label;
  final String group;
  final bool selected;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    return Semantics(
      selected: selected,
      inMutuallyExclusiveGroup: true,
      child: AxButton(
        label: label,
        hint: '$group: $label',
        kind: selected ? ButtonKind.standard : ButtonKind.quiet,
        onPressed: selected ? () {} : onPressed,
      ),
    );
  }
}
