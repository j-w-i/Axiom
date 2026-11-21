// Environments (FL-ENV-001; the Tauri shell's Environments.tsx, M2-UI-001/002): the page lists
// the toolchains the open folder needs, their detected versions and sources, and what can be done
// (Plan install…, Remove); the plan dialog shows exactly the steps the daemon will run, and
// Install is the approval; an install shows its progress and can be cancelled. The side bar lists
// the toolchains with their state.

import 'dart:async';

import 'package:flutter/widgets.dart';

import '../app/app_state.dart';
import '../daemon/protocol.g.dart';
import '../state/environments.dart';
import '../ui/controls.dart';
import '../ui/icon_data.dart';
import '../ui/icons.dart';
import '../ui/overlays.dart';
import '../ui/surfaces.dart';
import '../ui/theme.dart';
import '../ui/tokens.dart';

const _projectFile = '.axiom/project.toml';

DotState _lamp(RowKind k) => switch (k) {
  RowKind.ready => DotState.ok,
  RowKind.installing => DotState.running,
  RowKind.error => DotState.error,
  RowKind.missing => DotState.warn,
};

/// The page in the main area.
class EnvironmentsPage extends StatefulWidget {
  const EnvironmentsPage({super.key});

  @override
  State<EnvironmentsPage> createState() => _EnvironmentsPageState();
}

class _EnvironmentsPageState extends State<EnvironmentsPage> {
  late final AppState app = AppScope.of(context);
  bool _planShown = false;

  @override
  void initState() {
    super.initState();
    app.environments.addListener(_onStore);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (app.environments.status == 'idle') unawaited(app.environments.refresh());
    });
  }

  @override
  void dispose() {
    app.environments.removeListener(_onStore);
    super.dispose();
  }

  /// The plan the daemon returned is shown for approval.
  void _onStore() {
    final p = app.environments.plan;
    if (p == null || _planShown || !mounted) return;
    _planShown = true;
    unawaited(
      showAxDialog<bool>(
        context: context,
        label: 'Install ${p.providerName} ${p.plan.version}',
        builder: (_) => PlanDialog(plan: p.plan, providerName: p.providerName),
      ).then((install) {
        _planShown = false;
        if (install == true) {
          unawaited(app.environments.approvePlan());
        } else {
          app.environments.dismissPlan();
        }
      }),
    );
  }

  Future<void> _remove(ToolchainRow row) async {
    final answer = await showAxDialog<String>(
      context: context,
      label: 'Remove ${row.name} ${row.removable}',
      builder: (_) => AxQuestion(
        title: 'Remove ${row.name} ${row.removable}?',
        detail:
            'This deletes ${shortenHome(row.source?.where ?? '')} and nothing else. Projects '
            'that select this version will need it installed again.',
        actions: const [('cancel', 'Cancel'), ('delete', 'Remove')],
      ),
    );
    if (answer == 'delete' && row.removable != null) {
      await app.environments.uninstall(row.providerId, row.removable!);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final s = app.environments;
    return ListenableBuilder(
      listenable: Listenable.merge([s, app.workspace]),
      builder: (context, _) {
        Widget centered(List<Widget> children) => Center(
          child: Column(mainAxisSize: MainAxisSize.min, children: children),
        );
        if (!app.workspace.isOpen) {
          return centered([
            const AnswerLine('Open a folder to see the toolchains it needs'),
            const SizedBox(height: Metrics.s5),
            AxButton(
              label: 'Open Folder…',
              icon: AxiomIcons.folderOpen,
              onPressed: () => unawaited(app.pickFolder?.call()),
            ),
          ]);
        }
        if (s.status == 'error') {
          return centered([
            const AnswerLine('The toolchains could not be detected'),
            const SizedBox(height: Metrics.s3),
            Text(s.error, style: t.type.small),
            const SizedBox(height: Metrics.s5),
            AxButton(label: 'Detect again', onPressed: () => unawaited(s.refresh())),
          ]);
        }
        if (s.status != 'ready') return centered([const Spinner(label: 'Detecting toolchains')]);
        final rows = s.rows();
        return ColoredBox(
          color: c.ground,
          child: ListView(
            padding: const EdgeInsets.fromLTRB(28, 20, 28, 28),
            children: [
              Row(
                children: [
                  Expanded(child: AnswerLine('Toolchains for ${app.workspace.name}')),
                  AxIconButton(
                    icon: AxiomIcons.refresh,
                    label: 'Detect the toolchains again',
                    onPressed: () => unawaited(s.refresh()),
                  ),
                ],
              ),
              const SizedBox(height: Metrics.s5),
              if (rows.isEmpty)
                Text(
                  'No provider recognises this folder. Providers recognise projects by their files '
                  '(for example Cargo.toml). Available: '
                  '${s.providers.where((p) => p.trusted && (p.errors?.isEmpty ?? true)).map((p) => p.name).join(', ')}.',
                  style: t.type.small,
                )
              else
                Semantics(
                  container: true,
                  explicitChildNodes: true,
                  label: 'Toolchains',
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      _header(context),
                      const Hairline(),
                      for (final r in rows) _ToolchainRowView(row: r, onRemove: () => _remove(r)),
                    ],
                  ),
                ),
              if (s.untrusted.isNotEmpty) ...[
                const SizedBox(height: Metrics.s7),
                Semantics(
                  container: true,
                  explicitChildNodes: true,
                  label: "This folder's own providers",
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text('This folder has its own providers', style: t.type.label),
                      const SizedBox(height: 4),
                      Text(
                        '${s.untrusted.map((p) => p.name).join(', ')} in .axiom/providers/. They are '
                        'not used until you trust this folder: a provider decides which programs '
                        'run to detect and install toolchains.',
                        style: t.type.small,
                      ),
                      const SizedBox(height: Metrics.s3),
                      AxButton(
                        label: "Trust this folder's providers",
                        icon: AxiomIcons.shield,
                        onPressed: () => unawaited(s.trustWorkspace(true)),
                      ),
                    ],
                  ),
                ),
              ],
              if (s.invalid.isNotEmpty) ...[
                const SizedBox(height: Metrics.s7),
                Text('Providers with errors', style: t.type.label),
                for (final p in s.invalid)
                  Padding(
                    padding: const EdgeInsets.only(top: 4),
                    child: Text(
                      '${p.id} (${p.source.wire}): ${(p.errors ?? const []).join('; ')}',
                      style: t.type.codeSmall.copyWith(color: c.dangerText),
                    ),
                  ),
              ],
            ],
          ),
        );
      },
    );
  }

  Widget _header(BuildContext context) {
    final t = context.axiom;
    Widget h(String s) => Text(s, style: t.type.tiny);
    return ExcludeSemantics(
      child: Padding(
        padding: const EdgeInsets.only(bottom: 6),
        child: Row(
          children: [
            Expanded(flex: 4, child: h('Toolchain')),
            Expanded(flex: 3, child: h('Required by project')),
            Expanded(flex: 2, child: h('Detected')),
            Expanded(flex: 4, child: h('Source')),
            const Expanded(flex: 5, child: SizedBox()),
          ],
        ),
      ),
    );
  }
}

class _ToolchainRowView extends StatelessWidget {
  const _ToolchainRowView({required this.row, required this.onRemove});
  final ToolchainRow row;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final s = app.environments;
    final t = context.axiom;
    final c = t.colors;
    final source = row.source == null
        ? '—'
        : '${row.source!.kind} (${shortenHome(row.source!.where)})';
    final mono = t.type.codeSmall.copyWith(color: c.ink);
    Widget state() {
      switch (row.kind) {
        case RowKind.ready:
          return Row(
            children: [
              AxiomIcon(AxiomIcons.check, size: 14, color: c.okText),
              const SizedBox(width: 4),
              Text('Ready', style: t.type.small.copyWith(color: c.okText)),
            ],
          );
        case RowKind.installing:
          final label = row.progress?.label ?? 'Starting';
          return Row(
            children: [
              SizedBox(
                width: 90,
                child: Meter(value: progressValue(row.progress), label: 'Installing ${row.name}'),
              ),
              const SizedBox(width: 6),
              Flexible(
                child: Text(label, style: t.type.tiny, overflow: TextOverflow.ellipsis),
              ),
              AxButton(
                label: 'Cancel',
                kind: ButtonKind.quiet,
                hint: 'Stop installing ${row.name}',
                onPressed: () => s.cancelInstall(row.providerId),
              ),
            ],
          );
        case RowKind.missing || RowKind.error:
          return Row(
            children: [
              if (row.error != null)
                Flexible(
                  child: Semantics(
                    liveRegion: true,
                    child: Text(
                      row.error!,
                      style: t.type.small.copyWith(color: c.dangerText),
                      overflow: TextOverflow.ellipsis,
                      maxLines: 2,
                    ),
                  ),
                ),
              const SizedBox(width: 6),
              s.planning == row.providerId
                  ? const Spinner(label: 'Planning')
                  : AxButton(
                      label: 'Plan install…',
                      icon: AxiomIcons.download,
                      hint: 'Review what installing ${row.name} would do',
                      onPressed: () => unawaited(s.planInstall(row.providerId)),
                    ),
            ],
          );
      }
    }

    return Semantics(
      container: true,
      explicitChildNodes: true,
      label:
          '${row.name}: ${row.word}, required ${row.selected ?? 'any'}, detected '
          '${row.detected ?? 'not found'}, $source',
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 8),
        child: Row(
          children: [
            Expanded(
              flex: 4,
              child: Row(
                children: [
                  StatusDot(_lamp(row.kind)),
                  const SizedBox(width: 8),
                  Text(row.name, style: t.type.label),
                  const SizedBox(width: 8),
                  Flexible(
                    child: Text(
                      row.tools.join(' · '),
                      style: t.type.tiny,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                ],
              ),
            ),
            Expanded(
              flex: 3,
              child: Text(
                row.selected == null ? '—' : '${row.selected} ($_projectFile)',
                style: mono,
              ),
            ),
            Expanded(flex: 2, child: Text(row.detected ?? 'not found', style: mono)),
            Expanded(
              flex: 4,
              child: Text(source, style: t.type.codeSmall, overflow: TextOverflow.ellipsis),
            ),
            Expanded(
              flex: 5,
              child: Row(
                mainAxisAlignment: MainAxisAlignment.end,
                children: [
                  Flexible(child: state()),
                  if (row.removable != null && row.kind != RowKind.installing)
                    AxIconButton(
                      icon: AxiomIcons.trash,
                      label: 'Remove ${row.name} ${row.removable}',
                      size: 26,
                      onPressed: onRemove,
                    ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// A progress bar (0…1).
class Meter extends StatelessWidget {
  const Meter({super.key, required this.value, required this.label});
  final double value;
  final String label;

  @override
  Widget build(BuildContext context) {
    final c = context.colors;
    return Semantics(
      label: label,
      value: '${(value * 100).round()}%',
      child: SizedBox(
        height: 6,
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: c.recessed,
            borderRadius: BorderRadius.circular(3),
            border: Border.all(color: c.hairline),
          ),
          child: FractionallySizedBox(
            alignment: Alignment.centerLeft,
            widthFactor: value.clamp(0, 1),
            child: DecoratedBox(
              decoration: BoxDecoration(color: c.accent, borderRadius: BorderRadius.circular(3)),
            ),
          ),
        ),
      ),
    );
  }
}

/// The install plan: every step the daemon will run, and what it does to the machine. Install
/// (focused) is the approval; Cancel or Esc closes it.
class PlanDialog extends StatelessWidget {
  const PlanDialog({super.key, required this.plan, required this.providerName});
  final InstallPlan plan;
  final String providerName;

  static String fileName(String url) => url.split('/').last.isEmpty ? url : url.split('/').last;

  List<String> notes() => [
    if (plan.scope == InstallScope.system) "This changes your system outside Axiom's folders.",
    if (plan.effects.contains(Effect.elevation))
      'Your system will ask for an administrator password; Axiom never sees it.',
    if (plan.effects.contains(Effect.writeManaged) && plan.scope == InstallScope.managed)
      "Everything goes into Axiom's own toolchains folder and can be removed.",
  ];

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    Widget step(Step s) {
      final (icon, name, details) = switch (s) {
        StepDownload(:final url, :final bytes, :final sha256) => (
          AxiomIcons.download,
          'Download ${fileName(url)}${bytes == null ? '' : ' · ${formatBytes(bytes)}'}',
          [
            url,
            'SHA-256 ${switch (sha256) {
              Sha256SourceDigest(:final value) => value,
              Sha256SourceUrl(:final url) => 'from $url',
            }}, checked before anything is used',
          ],
        ),
        StepExtract(:final format) => (
          AxiomIcons.archive,
          format == ArchiveFormat.file
              ? 'Place the downloaded program'
              : 'Extract the ${format.wire} archive',
          ["Into Axiom's toolchains folder only"],
        ),
        StepRun(:final argv, :final env) => (
          AxiomIcons.terminal,
          'Run ${fileName(argv.firstOrNull ?? '')}',
          [argv.join(' '), for (final e in (env ?? const {}).entries) '${e.key}=${e.value}'],
        ),
        StepVerify(:final toolId) => (
          AxiomIcons.check,
          'Check that $toolId runs',
          const <String>[],
        ),
        _ => (AxiomIcons.shield, s.toJson().toString(), const <String>[]),
      };
      return Semantics(
        container: true,
        label: [name, ...details].join('. '),
        excludeSemantics: true,
        child: Padding(
          padding: const EdgeInsets.only(bottom: Metrics.s4),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              AxiomIcon(icon, size: 16, color: c.ink2),
              const SizedBox(width: 10),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(name, style: t.type.label),
                    for (final d in details)
                      Padding(
                        padding: const EdgeInsets.only(top: 2),
                        child: Text(d, style: t.type.codeSmall),
                      ),
                  ],
                ),
              ),
            ],
          ),
        ),
      );
    }

    return SizedBox(
      width: 560,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(20, 18, 20, 16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Semantics(
              header: true,
              child: Text('Install $providerName ${plan.version}', style: t.type.title),
            ),
            const SizedBox(height: Metrics.s4),
            if (plan.scope == InstallScope.system)
              Text('System install', style: t.type.label.copyWith(color: c.warnText)),
            for (final n in notes())
              Padding(
                padding: const EdgeInsets.only(bottom: 6),
                child: Text(n, style: t.type.small),
              ),
            const SizedBox(height: Metrics.s3),
            Semantics(
              container: true,
              explicitChildNodes: true,
              label: 'Steps',
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [for (final s in plan.steps) step(s)],
              ),
            ),
            if (plan.downloadBytes != null)
              Text('Download size: ${formatBytes(plan.downloadBytes!)}', style: t.type.small),
            const SizedBox(height: Metrics.s5),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                AxButton(
                  label: 'Cancel',
                  kind: ButtonKind.quiet,
                  onPressed: () => Navigator.of(context).pop(false),
                ),
                const SizedBox(width: Metrics.s3),
                AxButton(
                  label: 'Install',
                  autofocus: true,
                  onPressed: () => Navigator.of(context).pop(true),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

/// The side bar's list: each toolchain with its state.
class EnvironmentsSide extends StatelessWidget {
  const EnvironmentsSide({super.key});

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final s = app.environments;
    final t = context.axiom;
    return ListenableBuilder(
      listenable: s,
      builder: (context, _) {
        final rows = s.rows();
        if (rows.isEmpty) {
          return Padding(
            padding: const EdgeInsets.all(Metrics.s5),
            child: Text(
              s.status == 'loading' ? 'Detecting…' : 'No toolchains for this folder',
              style: t.type.small,
            ),
          );
        }
        return Semantics(
          container: true,
          explicitChildNodes: true,
          label: 'Toolchains',
          child: ListView(
            padding: const EdgeInsets.symmetric(horizontal: Metrics.s2),
            children: [
              for (final r in rows)
                AxListRow(
                  label:
                      '${r.name}: ${r.kind == RowKind.ready ? 'Ready${r.detected == null ? '' : ', ${r.detected}'}' : r.word}',
                  height: 28,
                  onPressed: () {},
                  child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 8),
                    child: Row(
                      children: [
                        StatusDot(_lamp(r.kind)),
                        const SizedBox(width: 8),
                        Expanded(child: Text(r.name, style: t.type.body)),
                        Text(r.detected ?? r.word, style: t.type.tiny),
                      ],
                    ),
                  ),
                ),
            ],
          ),
        );
      },
    );
  }
}

/// The rail's word for the toolchains that need the person.
String environmentsRailLabel(int n) => n == 0 ? 'Environments' : 'Environments, $n need attention';

/// Shown while an install runs (the transport).
String? installTransportLabel(EnvironmentsStore s) {
  final a = s.activeInstall;
  return a == null ? null : 'Installing ${a.name}';
}
