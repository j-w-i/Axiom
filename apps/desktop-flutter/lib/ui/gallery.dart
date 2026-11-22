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
    // Tauri's gallery: each family on a panel card, a row per variant with its name on the left.
    Widget section(String title, List<Widget> children) => Padding(
      padding: const EdgeInsets.only(bottom: Metrics.s6),
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: c.panel,
          borderRadius: BorderRadius.circular(Metrics.radiusSheet),
          border: Border.all(color: c.groove),
        ),
        child: Padding(
          padding: const EdgeInsets.all(Metrics.s6),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Semantics(
                header: true,
                child: Text(title, style: t.type.small.copyWith(fontWeight: FontWeight.w600)),
              ),
              const SizedBox(height: Metrics.s5),
              ...children,
            ],
          ),
        ),
      ),
    );
    Widget row(String name, List<Widget> children) => Padding(
      padding: const EdgeInsets.symmetric(vertical: Metrics.s3),
      child: Row(
        children: [
          SizedBox(width: 140, child: Text(name, style: t.type.small)),
          Expanded(
            child: Wrap(
              spacing: Metrics.s5,
              runSpacing: Metrics.s4,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: children,
            ),
          ),
        ],
      ),
    );
    return ColoredBox(
      color: c.canvas,
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(Metrics.s7),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const AnswerLine('Component gallery'),
            const SizedBox(height: Metrics.s6),
            section('Surfaces', [
              row('depth', [
                for (final (name, surface) in [
                  ('Recessed', (Widget w) => Well(child: w)),
                  ('Panel', (Widget w) => ColoredBox(color: c.panel, child: w)),
                  ('Raised', (Widget w) => Raised(child: w)),
                  ('Sheet', (Widget w) => Sheet(child: w)),
                ])
                  SizedBox(
                    width: 120,
                    height: 44,
                    child: surface(Center(child: Text(name, style: t.type.small))),
                  ),
              ]),
            ]),
            section('Button', [
              for (final (name, kind) in [
                ('default', ButtonKind.standard),
                ('primary', ButtonKind.primary),
                ('quiet', ButtonKind.quiet),
                ('danger', ButtonKind.danger),
              ])
                row(name, [
                  AxButton(label: 'Run', kind: kind, icon: AxiomIcons.play, onPressed: () {}),
                  AxButton(label: 'Run', kind: kind, icon: AxiomIcons.play, onPressed: null),
                ]),
              row('with keys', [
                AxButton(
                  label: 'Open Folder…',
                  kind: ButtonKind.primary,
                  trailing: const Keycaps('Mod+O'),
                  onPressed: () {},
                ),
              ]),
            ]),
            section('IconButton', [
              row('toggle', [
                AxIconButton(
                  icon: AxiomIcons.sidebar,
                  label: 'Toggle side bar',
                  shortcut: 'Mod+B',
                  onPressed: () {},
                ),
                AxIconButton(
                  icon: AxiomIcons.panel,
                  label: 'Toggle console',
                  shortcut: 'Mod+J',
                  selected: true,
                  onPressed: () {},
                ),
                AxIconButton(
                  icon: AxiomIcons.gear,
                  label: 'Settings',
                  shortcut: 'Mod+,',
                  onPressed: () {},
                ),
              ]),
              row('send', [
                InkCircleButton(
                  icon: AxiomIcons.send,
                  label: 'Send',
                  shortcut: 'Enter',
                  onPressed: () {},
                ),
              ]),
            ]),
            section('Tabs', [
              row('views', [
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
            ]),
            section('Field, Switch, Checkbox', [
              row('fields', [
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
              ]),
              row('choices', [
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
            ]),
            section('Keys, Badge, Status, Spinner', [
              row('status', [
                for (final s in DotState.values)
                  Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      StatusDot(s, ring: c.panel),
                      const SizedBox(width: 4),
                      Text(s.name, style: t.type.small),
                    ],
                  ),
              ]),
              row('badge / spinner', [
                const AxBadge(3),
                const AxBadge(12, attention: false),
                const Spinner(),
              ]),
              row('keys', [
                const Keycaps('Mod+K'),
                const Keycaps('Mod+Shift+R'),
                const Keycaps('F5'),
              ]),
            ]),
            section('Menu', [
              row('menu', [
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
                      style: t.type.small.copyWith(color: open || s.hovered ? c.ink : c.ink2),
                    ),
                  ),
                ),
              ]),
            ]),
            section('Rows and splitter', [
              row('tree', [
                SizedBox(
                  width: 480,
                  height: 120,
                  child: Row(
                    children: [
                      SizedBox(
                        width: _split,
                        child: ColoredBox(
                          color: c.recessed,
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
            ]),
            section('Icons', [
              row('icons', [
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
            ]),
            Builder(
              builder: (context) => Align(
                alignment: Alignment.centerLeft,
                child: AxButton(
                  label: 'Show a toast',
                  kind: ButtonKind.quiet,
                  onPressed: () =>
                      ToastHost.of(context).show('Saved src/main.rs', kind: ToastKind.ok),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
