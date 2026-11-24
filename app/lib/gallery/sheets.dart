import 'package:flutter/widgets.dart';

import '../patterns/models.dart';
import '../patterns/session_panel.dart';
import '../patterns/transcript_parts.dart';
import '../patterns/usage.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/marks.dart';
import '../ui/overlay.dart';
import '../ui/press.dart';
import '../ui/status.dart';
import '../ui/surface.dart';
import '../ui/tokens.dart';
import 'specimen.dart' as s;

/// The gallery's pages; the golden tests render the same ones.
final sheets = <(String, WidgetBuilder)>[
  ('Controls', (_) => const ControlsSheet()),
  ('Surfaces', (_) => const SurfacesSheet()),
  ('Type', (_) => const TypeSheet()),
  ('Status', (_) => const StatusSheet()),
];

class _Sheet extends StatelessWidget {
  const _Sheet(this.sections);
  final List<(String, List<Widget>)> sections;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context);
    return Padding(
      padding: const EdgeInsets.all(24),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (final (name, kids) in sections) ...[
            Text(name, style: t.type.labelStrong.ink(t.colors.ink3)),
            const SizedBox(height: 10),
            Wrap(
              spacing: 12,
              runSpacing: 12,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: kids,
            ),
            const SizedBox(height: 24),
          ],
        ],
      ),
    );
  }
}

class ControlsSheet extends StatefulWidget {
  const ControlsSheet({super.key});
  @override
  State<ControlsSheet> createState() => _ControlsSheetState();
}

class _ControlsSheetState extends State<ControlsSheet> {
  String _seg = 'a', _sel = 'high';
  bool _on = true, _off = false;
  int _tab = 0;
  @override
  Widget build(BuildContext context) => _Sheet([
    (
      'Buttons',
      [
        AxButton(label: 'Run the tests', onPressed: () {}),
        AxButton(label: 'Queue', icon: AxIcon.send, tone: AxTone.primary, onPressed: () {}),
        AxButton(label: 'Allow once', tone: AxTone.primary, onPressed: () {}),
        AxButton(label: 'Deny', tone: AxTone.quiet, onPressed: () {}),
        AxButton(label: 'Engaged', engaged: true, onPressed: () {}),
        const AxButton(label: 'Disabled', onPressed: null),
        const AxButton(label: 'Disabled', tone: AxTone.primary, onPressed: null),
        AxButton.icon(AxIcon.pause, label: 'Pause', onPressed: () {}),
        AxButton.icon(AxIcon.search, label: 'Find', tone: AxTone.quiet, onPressed: () {}),
      ],
    ),
    (
      'Choices',
      [
        AxSegments(
          label: 'View',
          items: const [('a', 'Conversation', null), ('b', 'Score', null)],
          value: _seg,
          onChanged: (v) => setState(() => _seg = v),
        ),
        AxSelect(
          label: 'Effort',
          items: s.efforts,
          value: _sel,
          onChanged: (v) => setState(() => _sel = v),
        ),
        AxSwitch(label: 'Notify', value: _on, onChanged: (v) => setState(() => _on = v)),
        AxSwitch(label: 'Notify less', value: _off, onChanged: (v) => setState(() => _off = v)),
        const AxSwitch(label: 'Fixed', value: true, onChanged: null),
        AxCheck(label: 'Include tests', value: _on, onChanged: (v) => setState(() => _on = v)),
        AxCheck(label: 'Include docs', value: _off, onChanged: (v) => setState(() => _off = v)),
        const AxCheck(label: 'Fixed', value: true, onChanged: null),
      ],
    ),
    (
      'Menu',
      [
        SizedBox(
          width: 260,
          child: Surface(
            Ax.of(context).colors.overlay,
            radius: AxRadius.overlay,
            child: Padding(
              padding: const EdgeInsets.all(5),
              child: ExcludeFocus(
                child: AxMenu<String>(items: s.harnesses, value: 'codex', onChosen: (_) {}),
              ),
            ),
          ),
        ),
      ],
    ),
    (
      'Fields',
      [
        const SizedBox(
          width: 220,
          child: AxField(label: 'Name', placeholder: 'Session name'),
        ),
        const SizedBox(
          width: 220,
          child: AxField(
            label: 'Find',
            placeholder: 'Find',
            leading: AxGlyph(AxIcon.search, size: 12),
          ),
        ),
        const SizedBox(
          width: 220,
          child: AxField(label: 'Port', placeholder: '1234', error: 'Nothing answers on this port'),
        ),
        const SizedBox(
          width: 220,
          child: AxField(label: 'Key', placeholder: 'Not editable', enabled: false),
        ),
      ],
    ),
    (
      'Tabs, keys',
      [
        AxTabs(
          tabs: const [('Conversation', null), ('Changes', '3'), ('Terminal', null)],
          index: _tab,
          onChanged: (i) => setState(() => _tab = i),
        ),
        const AxKeycap('⌘K'),
        const AxKeycap('Esc'),
      ],
    ),
    (
      'Focus',
      [
        CustomPaint(
          foregroundPainter: FocusRing(Ax.of(context).colors.focus, AxRadius.key),
          child: AxButton(label: 'Focused', onPressed: () {}),
        ),
      ],
    ),
  ]);
}

class SurfacesSheet extends StatelessWidget {
  const SurfacesSheet({super.key});
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    Widget sw(String n, SurfaceSpec x) => SizedBox(
      width: 150,
      height: 72,
      child: Surface(
        x,
        child: Padding(
          padding: const EdgeInsets.all(10),
          child: Text(n, style: t.type.label.ink(n.startsWith('primary') ? c.onPrimary : c.ink2)),
        ),
      ),
    );
    return _Sheet([
      (
        'Structure',
        [
          sw('chassis', c.chassis),
          sw('plate', c.plate),
          sw('plateHover', c.plateHover),
          sw('plateSelected', c.plateSelected),
          sw('overlay', c.overlay),
        ],
      ),
      ('Displays', [sw('bay', c.bay), sw('block', c.block), sw('well', c.well)]),
      (
        'Keys',
        [
          sw('key', c.key),
          sw('keyHover', c.keyHover),
          sw('keyDown', c.keyDown),
          sw('primary', c.primary),
          sw('primaryHover', c.primaryHover),
          sw('primaryDown', c.primaryDown),
        ],
      ),
      (
        'Services',
        [
          for (final m in ServiceMark.values)
            SizedBox(
              width: 118,
              child: Row(
                children: [
                  AxServiceMark(m, size: 15),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      m.label,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: t.type.label.ink(c.ink2),
                    ),
                  ),
                ],
              ),
            ),
        ],
      ),
      (
        'Icons',
        [
          for (final i in AxIcon.values) AxGlyph(i, size: 18, color: i.kind(c) ?? c.ink2),
          const AxMark(size: 22),
        ],
      ),
    ]);
  }
}

class TypeSheet extends StatelessWidget {
  const TypeSheet({super.key});
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context).type;
    return _Sheet([
      for (final (n, st) in [
        ('heading', t.heading),
        ('title', t.title),
        ('prose', t.prose),
        ('ui', t.ui),
        ('uiStrong', t.uiStrong),
        ('label', t.label),
        ('labelStrong', t.labelStrong),
        ('small', t.small),
        ('caption', t.caption),
        ('code', t.code),
        ('codeSmall', t.codeSmall),
        ('readout', t.readout),
      ])
        (n, [Text('Rotate once per account, 1,284 → 96 ms, Ilf1 O0 8B rn', style: st)]),
    ]);
  }
}

class StatusSheet extends StatelessWidget {
  const StatusSheet({super.key});
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    Widget m(String n, double? v, Provenance p) => SizedBox(
      width: 150,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(n, style: t.type.small.ink(c.ink3)),
          const SizedBox(height: 4),
          AxMeter(label: n, value: v, provenance: p),
        ],
      ),
    );
    return _Sheet([
      (
        'Lamps',
        [
          for (final st in AgentState.values)
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                AxLamp(st),
                const SizedBox(width: 8),
                Text(st.word, style: t.type.label.ink(st.text(c))),
              ],
            ),
        ],
      ),
      (
        'Meters',
        [
          m('Reported', .62, Provenance.reported),
          m('Measured', .31, Provenance.measured),
          m('Estimated', .44, Provenance.estimated),
          m('Not available', null, Provenance.unknown),
          m('Near the limit', .92, Provenance.reported),
        ],
      ),
      ('Accounts', [for (final v in s.services) UsageKey(v, onUse: (_) {}, onLink: () {})]),
      (
        'Session',
        [
          SizedBox(
            width: 248,
            child: SessionPanel(
              session: s.session,
              selected: 'tester',
              onSelect: (_) {},
              onAddAgent: () {},
            ),
          ),
        ],
      ),
      (
        'Work',
        [
          const SizedBox(
            width: 460,
            child: DiffBlock(path: 'src/auth/refresh.ts', lines: s.diff),
          ),
        ],
      ),
      (
        'Request',
        [
          SizedBox(
            width: 760,
            child: RequestBar(s.request, onOnce: () {}, onAlways: () {}, onDeny: () {}),
          ),
        ],
      ),
      (
        'Tasks',
        [
          SizedBox(
            width: 240,
            child: Column(
              children: [
                for (final k in TaskState.values) TaskRow(TaskView(k.name, k, owner: 'Builder')),
              ],
            ),
          ),
        ],
      ),
    ]);
  }
}
