// Every control of the design system in one page (FL-DS-001): used by the golden tests and, in
// debug builds, shown with `--gallery`.

import 'package:flutter/widgets.dart';

import 'controls.dart';
import 'icons.dart';
import 'icon_data.dart';
import 'overlays.dart';
import 'splitter.dart';
import 'surfaces.dart';
import 'tabs.dart';
import 'text_input.dart';
import 'theme.dart';
import 'tokens.dart';

class Gallery extends StatefulWidget {
  const Gallery({super.key});

  @override
  State<Gallery> createState() => _GalleryState();
}

class _GalleryState extends State<Gallery> {
  String _tab = 'files';
  bool _check = true;
  bool _toggle = false;
  double _split = 220;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    Widget section(String title, List<Widget> children) => Padding(
      padding: const EdgeInsets.only(bottom: Metrics.s6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Semantics(header: true, child: Text(title, style: t.type.tiny)),
          const SizedBox(height: Metrics.s3),
          Wrap(
            spacing: Metrics.s4,
            runSpacing: Metrics.s4,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: children,
          ),
        ],
      ),
    );
    return ColoredBox(
      color: c.ground,
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(Metrics.s6),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const AnswerLine(
              'Rust ready · Node needs attention',
              detail: 'The answer line: one sentence, the only large type.',
            ),
            const SizedBox(height: Metrics.s6),
            section('Surfaces', [
              SizedBox(
                width: 120,
                height: 48,
                child: Well(
                  child: Center(child: Text('Recessed', style: t.type.small)),
                ),
              ),
              SizedBox(
                width: 120,
                height: 48,
                child: ColoredBox(
                  color: c.bar,
                  child: Center(child: Text('Bar', style: t.type.small)),
                ),
              ),
              SizedBox(
                width: 120,
                height: 48,
                child: Raised(
                  child: Center(child: Text('Raised', style: t.type.small)),
                ),
              ),
              SizedBox(
                width: 120,
                height: 48,
                child: Sheet(
                  child: Center(child: Text('Sheet', style: t.type.small)),
                ),
              ),
            ]),
            section('Buttons', [
              AxButton(label: 'Open Folder…', onPressed: () {}, icon: AxiomIcons.folder),
              AxButton(label: 'Quiet', kind: ButtonKind.quiet, onPressed: () {}),
              AxButton(label: 'Delete', kind: ButtonKind.danger, onPressed: () {}),
              const AxButton(label: 'Disabled', onPressed: null),
              AxIconButton(
                icon: AxiomIcons.sidebar,
                label: 'Toggle side bar',
                shortcut: 'Mod+B',
                onPressed: () {},
              ),
              AxIconButton(
                icon: AxiomIcons.gear,
                label: 'Settings',
                shortcut: 'Mod+,',
                onPressed: () {},
              ),
              InkCircleButton(
                icon: AxiomIcons.send,
                label: 'Send',
                shortcut: 'Enter',
                onPressed: () {},
              ),
            ]),
            section('Tabs', [
              AxTabs(
                label: 'Example tabs',
                tabs: const [
                  TabItem(id: 'files', label: 'Files'),
                  TabItem(id: 'chat', label: 'Chat'),
                  TabItem(id: 'search', label: 'Search'),
                ],
                selected: _tab,
                onSelect: (id) => setState(() => _tab = id),
              ),
            ]),
            section('Inputs', [
              const SizedBox(
                width: 240,
                child: AxTextInput(label: 'Search files', placeholder: 'Search files'),
              ),
              SizedBox(
                width: 360,
                child: AxTextInput(
                  label: 'Message',
                  placeholder: 'Ask Axiom or an agent…',
                  bezel: true,
                  maxLines: 4,
                  minLines: 2,
                  trailing: InkCircleButton(
                    icon: AxiomIcons.send,
                    label: 'Send message',
                    onPressed: () {},
                  ),
                ),
              ),
              AxCheckbox(
                label: 'Match case',
                value: _check,
                onChanged: (v) => setState(() => _check = v),
              ),
              AxToggle(
                label: 'Reduce motion',
                value: _toggle,
                onChanged: (v) => setState(() => _toggle = v),
              ),
            ]),
            section('Status', [
              for (final s in DotState.values)
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    StatusDot(s, ring: c.ground),
                    const SizedBox(width: 4),
                    Text(s.name, style: t.type.small),
                  ],
                ),
              const AxBadge(3),
              const AxBadge(12, attention: false),
              const Spinner(),
              const Keycaps('Mod+Shift+P'),
            ]),
            section('Menu', [
              AxMenu(
                label: 'File menu',
                entries: () => [
                  const MenuEntry.heading('Workspace'),
                  MenuEntry.item(
                    'Open Folder…',
                    () {},
                    shortcut: 'Mod+O',
                    icon: AxiomIcons.folderOpen,
                  ),
                  MenuEntry.item('Close Folder', () {}),
                  const MenuEntry.item('Disabled item', null, enabled: false),
                ],
                trigger: (context, s, open) => Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                  child: Text(
                    'File',
                    style: t.type.label.copyWith(color: open || s.hovered ? c.ink : c.ink2),
                  ),
                ),
              ),
            ]),
            section('Rows and splitter', [
              SizedBox(
                width: 480,
                height: 120,
                child: Row(
                  children: [
                    SizedBox(
                      width: _split,
                      child: ColoredBox(
                        color: c.bar,
                        child: Column(
                          children: [
                            AxListRow(
                              label: 'src',
                              onPressed: () {},
                              child: Row(
                                children: [
                                  const SizedBox(width: 6),
                                  AxiomIcon(AxiomIcons.folder, color: c.ink2),
                                  const SizedBox(width: 6),
                                  const Text('src'),
                                ],
                              ),
                            ),
                            AxListRow(
                              label: 'main.rs',
                              selected: true,
                              onPressed: () {},
                              child: Row(
                                children: [
                                  const SizedBox(width: 6),
                                  AxiomIcon(AxiomIcons.file, color: c.ink2),
                                  const SizedBox(width: 6),
                                  const Text('main.rs'),
                                ],
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                    AxSplitter(
                      label: 'Resize the example',
                      value: _split,
                      min: 120,
                      max: 360,
                      defaultValue: 220,
                      onChanged: (v) => setState(() => _split = v),
                    ),
                    Expanded(child: ColoredBox(color: c.ground)),
                  ],
                ),
              ),
            ]),
            section('Icons', [
              for (final icon in const [
                AxiomIcons.files,
                AxiomIcons.chat,
                AxiomIcons.search,
                AxiomIcons.branch,
                AxiomIcons.layers,
                AxiomIcons.terminal,
                AxiomIcons.play,
                AxiomIcons.stop,
                AxiomIcons.warning,
                AxiomIcons.check,
              ])
                AxiomIcon(icon, color: c.ink2),
            ]),
            Builder(
              builder: (context) => AxButton(
                label: 'Show a toast',
                kind: ButtonKind.quiet,
                onPressed: () =>
                    ToastHost.of(context).show('Saved src/main.rs', kind: ToastKind.ok),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
