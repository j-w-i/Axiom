// Settings (Mod+,): Appearance (theme, motion), Editor (font size, wrapping), Workspace
// (reopen the last folder), About (versions, the data directory). Values are the daemon's
// settings; they apply as soon as they are chosen.

import 'dart:async';

import 'dart:ui' show SemanticsRole;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../app/app_state.dart';
import '../chat/models.dart';
import '../daemon/client.dart';
import '../state/settings.dart';
import '../ui/controls.dart';
import '../ui/pressable.dart';
import '../ui/surfaces.dart';
import '../ui/theme.dart';
import '../ui/tokens.dart';

class SettingsPage extends StatefulWidget {
  const SettingsPage({super.key, required this.app, this.section = 'appearance'});
  final AppState app;

  /// The section shown first.
  final String section;

  @override
  State<SettingsPage> createState() => _SettingsPageState();
}

class _SettingsPageState extends State<SettingsPage> {
  late String _section = widget.section;

  static const _sections = [
    ('appearance', 'Appearance'),
    ('editor', 'Editor'),
    ('workspace', 'Workspace'),
    ('models', 'Models'),
    ('about', 'About'),
  ];

  void _step(int d) {
    final i = _sections.indexWhere((x) => x.$1 == _section);
    setState(() => _section = _sections[(i + d + _sections.length) % _sections.length].$1);
  }

  // Tauri's Settings: two panes on the overlay; the sections on the chrome tone at the left (the
  // selected one with the selected tint and a brass inset edge), the rows on the right.
  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final s = widget.app.settings;
    return SizedBox(
      width: 820,
      height: 600,
      child: ListenableBuilder(
        listenable: s,
        builder: (context, _) => Row(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Container(
              width: 200,
              padding: const EdgeInsets.symmetric(vertical: Metrics.s5, horizontal: Metrics.s4),
              decoration: BoxDecoration(
                color: c.bar,
                border: Border(right: BorderSide(color: c.groove)),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Padding(
                    padding: const EdgeInsets.fromLTRB(Metrics.s5, 0, 0, Metrics.s4),
                    child: Semantics(
                      header: true,
                      child: Text(
                        'Settings',
                        style: t.type.small.copyWith(color: c.ink, fontWeight: FontWeight.w600),
                      ),
                    ),
                  ),
                  Shortcuts(
                    shortcuts: const {
                      SingleActivator(LogicalKeyboardKey.arrowDown): _StepIntent(1),
                      SingleActivator(LogicalKeyboardKey.arrowUp): _StepIntent(-1),
                    },
                    child: Actions(
                      actions: {
                        _StepIntent: CallbackAction<_StepIntent>(onInvoke: (i) => _step(i.d)),
                      },
                      child: Semantics(
                        role: SemanticsRole.tabBar,
                        label: 'Settings sections',
                        container: true,
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            for (final (id, label) in _sections)
                              Padding(
                                padding: const EdgeInsets.only(bottom: 1),
                                child: Pressable(
                                  label: label,
                                  role: PressRole.tab,
                                  selected: id == _section,
                                  onPressed: () => setState(() => _section = id),
                                  builder: (context, st) {
                                    final on = id == _section;
                                    return FocusRing(
                                      visible: st.focused,
                                      child: Stack(
                                        children: [
                                          Container(
                                            height: 28,
                                            alignment: Alignment.centerLeft,
                                            padding: const EdgeInsets.symmetric(
                                              horizontal: Metrics.s5,
                                            ),
                                            decoration: BoxDecoration(
                                              color: on
                                                  ? c.selected
                                                  : (st.hovered ? c.hover : null),
                                              borderRadius: BorderRadius.circular(
                                                Metrics.radiusControl,
                                              ),
                                            ),
                                            child: Text(
                                              label,
                                              style: t.type.body.copyWith(
                                                color: on || st.hovered ? c.ink : c.ink2,
                                              ),
                                            ),
                                          ),
                                          if (on)
                                            Positioned(
                                              left: 0,
                                              top: 0,
                                              bottom: 0,
                                              width: 2,
                                              child: ColoredBox(color: c.accent),
                                            ),
                                        ],
                                      ),
                                    );
                                  },
                                ),
                              ),
                          ],
                        ),
                      ),
                    ),
                  ),
                  const Spacer(),
                  Align(
                    alignment: Alignment.centerLeft,
                    child: AxButton(
                      label: 'Close',
                      kind: ButtonKind.quiet,
                      hint: 'Close — Esc',
                      onPressed: () => Navigator.of(context).pop(),
                    ),
                  ),
                ],
              ),
            ),
            Expanded(
              child: Padding(
                padding: const EdgeInsets.fromLTRB(Metrics.s7, Metrics.s6, Metrics.s7, Metrics.s6),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    if (s.error.isNotEmpty)
                      Padding(
                        padding: const EdgeInsets.only(bottom: Metrics.s4),
                        child: Text(s.error, style: t.type.small.copyWith(color: c.dangerText)),
                      ),
                    Expanded(child: SingleChildScrollView(child: _body(context, s))),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  /// Tauri's setting row: the label (and its description) at the left, the control at the
  /// right, at least 52 px tall, a seam below.
  Widget _row(BuildContext context, String label, Widget control, {String? note}) {
    final t = context.axiom;
    return DecoratedBox(
      decoration: BoxDecoration(
        border: Border(bottom: BorderSide(color: t.colors.hairline)),
      ),
      child: ConstrainedBox(
        constraints: const BoxConstraints(minHeight: 52),
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: Metrics.s4),
          child: Row(
            children: [
              SizedBox(
                width: 200,
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(label, style: t.type.body.copyWith(fontWeight: FontWeight.w500)),
                    if (note != null)
                      Padding(
                        padding: const EdgeInsets.only(top: 2),
                        child: Text(note, style: t.type.small),
                      ),
                  ],
                ),
              ),
              const SizedBox(width: Metrics.s6),
              Expanded(
                child: Align(alignment: Alignment.centerRight, child: control),
              ),
            ],
          ),
        ),
      ),
    );
  }

  /// Tauri's segmented control: a recessed track, the choice raised.
  Widget _choice<T>(String group, List<(T, String)> options, T value, ValueChanged<T> onChoose) =>
      Well(
        radius: Metrics.radiusRaised,
        border: context.colors.lineInput,
        child: Padding(
          padding: const EdgeInsets.all(2),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              for (final (v, label) in options)
                _ChoiceButton(
                  label: label,
                  group: group,
                  selected: v == value,
                  onPressed: () => onChoose(v),
                ),
            ],
          ),
        ),
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
            // M5.5 (ADR-0021 §5): a missing language server is installed without asking.
            _row(
              context,
              'Language servers',
              AxToggle(
                label: 'Install language servers automatically',
                value: s.lspAutoInstall,
                onChanged: (v) => unawaited(s.setValue('lsp.autoInstall', v)),
              ),
              note: 'Off: Axiom asks first and shows what it will download.',
            ),
          ],
        );
      case 'models':
        return const ModelsPane();
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
    final t = context.axiom;
    final c = t.colors;
    return Semantics(
      selected: selected,
      inMutuallyExclusiveGroup: true,
      child: Pressable(
        label: label,
        hint: '$group: $label',
        onPressed: selected ? () {} : onPressed,
        builder: (context, s) {
          final text = Padding(
            padding: const EdgeInsets.symmetric(horizontal: Metrics.s5),
            child: Center(
              widthFactor: 1,
              child: Text(
                label,
                style: t.type.small.copyWith(color: selected || s.hovered ? c.ink : c.ink2),
              ),
            ),
          );
          return FocusRing(
            visible: s.focused,
            child: SizedBox(
              height: Metrics.controlHeightSmall - 2,
              child: selected ? Raised(radius: Metrics.radiusControl, child: text) : text,
            ),
          );
        },
      ),
    );
  }
}

class _StepIntent extends Intent {
  const _StepIntent(this.d);
  final int d;
}
