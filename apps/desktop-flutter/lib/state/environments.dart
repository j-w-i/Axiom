// The Environment Manager's state (FL-ENV-001; a port of the Tauri shell's EnvironmentsStore,
// M2-UI-001/002): which providers apply to the open folder, each tool's state, install plans and
// their progress. Everything comes from `env/*` (ADR-0014): the UI never edits a plan, it shows
// what the daemon returned and runs it only on the person's approval.

import 'dart:async';

import 'package:flutter/foundation.dart';

import '../daemon/client.dart';
import '../daemon/protocol.g.dart';
import 'workspace.dart';

enum RowKind { ready, missing, installing, error }

final class ToolchainRow {
  const ToolchainRow({
    required this.providerId,
    required this.name,
    required this.tools,
    required this.selected,
    required this.detected,
    required this.source,
    required this.removable,
    required this.kind,
    this.progress,
    this.error,
  });
  final String providerId, name;

  /// Tool ids, required ones first.
  final List<String> tools;

  /// What `.axiom/project.toml` selects, if anything.
  final String? selected;

  /// The version of the first required tool found.
  final String? detected;

  /// `managed` or `system`, and where.
  final ({String kind, String where})? source;

  /// A managed version that can be removed.
  final String? removable;
  final RowKind kind;
  final InstallProgress? progress;
  final String? error;

  /// The lamp's word.
  String get word => switch (kind) {
    RowKind.ready => 'Ready',
    RowKind.installing => 'Installing',
    RowKind.error => 'Install failed',
    RowKind.missing => 'Missing',
  };
}

/// An env/* failure as one sentence for the page.
String describeEnvError(Object e) {
  if (e is! DaemonException) return '$e';
  switch (e.kind) {
    case ErrorKind.checksumMismatch:
      return 'The download did not match its published checksum, so nothing was installed.';
    case ErrorKind.approvalDenied:
      return 'The plan expired or changed before it ran. Review a new plan.';
    case ErrorKind.unsupportedPlatform:
      return 'There is no install for this platform.';
    case ErrorKind.prerequisiteMissing:
      final d = e.details is Map ? (e.details! as Map)['missing'] : null;
      final missing = d is List ? d.join(', ') : '';
      return missing.isNotEmpty ? 'Needs $missing first.' : e.message;
    case ErrorKind.cancelled:
      return 'The install was cancelled.';
    default:
      return e.message;
  }
}

/// 1.2 MB style sizes.
String formatBytes(int n) {
  if (n < 1024) return '$n B';
  if (n < 1024 * 1024) return '${(n / 1024).toStringAsFixed(1)} KB';
  if (n < 1024 * 1024 * 1024) return '${(n / (1024 * 1024)).toStringAsFixed(1)} MB';
  return '${(n / (1024 * 1024 * 1024)).toStringAsFixed(2)} GB';
}

/// Fraction done: bytes within the download step, else steps done.
double progressValue(InstallProgress? p) {
  if (p == null || p.steps == 0) return 0;
  final within = (p.bytesTotal ?? 0) > 0 ? (p.bytesDone ?? 0) / p.bytesTotal! : 0.0;
  final v = (p.step + within) / p.steps;
  return v > 1 ? 1 : v;
}

/// `~` for the home directory.
String shortenHome(String path) => path.replaceFirst(
  RegExp(r'^(/home/[^/]+|/Users/[^/]+|[A-Za-z]:\\Users\\[^\\]+)(?=[/\\]|$)'),
  '~',
);

class EnvironmentsStore extends ChangeNotifier {
  EnvironmentsStore(this.client, this.workspace) {
    _offs = [
      client.on(Notifications.envInstallProgress, (p) {
        final id = _planProvider[p.planId];
        if (id == null || !installs.containsKey(id)) return;
        installs = {...installs, id: (planId: p.planId, progress: p)};
        notifyListeners();
      }),
      client.on(Notifications.envInstalled, (_) => unawaited(refresh())),
    ];
    workspace.addListener(_onWorkspace);
  }

  final DaemonClient client;
  final WorkspaceStore workspace;
  late final List<void Function()> _offs;

  String status = 'idle'; // idle, loading, ready, error
  String error = '';
  List<ProviderInfo> providers = const [];
  List<ProviderDetection> detection = const [];

  /// providerId → the plan being installed and its latest progress.
  Map<String, ({String planId, InstallProgress? progress})> installs = const {};

  /// providerId → the last install failure, in plain language.
  Map<String, String> failures = const {};

  /// The plan shown for approval, and its provider's name.
  ({InstallPlan plan, String providerName})? plan;

  /// Tools a provider's last plan was missing (`prerequisite_missing`), by provider: the shell
  /// offers the provider that supplies them (M5.5-UI-001).
  Map<String, List<String>> prerequisites = const {};

  /// Called when an install of a provider succeeds (language servers start again).
  void Function(String providerId)? onInstalled;

  /// Plans approved without the dialog (the auto-install switch, ADR-0021 §5).
  final _autoApprove = <String>{};

  /// Whether the shown plan is being approved by Axiom itself (no dialog).
  bool get planIsAutomatic => plan != null && _autoApprove.contains(plan!.plan.providerId);
  String? planning;

  final _planProvider = <String, String>{};
  final _cancels = <String, CancelToken>{};
  int _generation = 0;
  String? _lastWs;

  void _onWorkspace() {
    final id = workspace.isOpen ? workspace.id : null;
    if (id == _lastWs) return;
    _lastWs = id;
    _generation++;
    status = 'idle';
    error = '';
    providers = const [];
    detection = const [];
    failures = const {};
    plan = null;
    notifyListeners();
    // Detection runs the providers' version commands; its result also feeds the rail's badge.
    if (id != null) unawaited(refresh());
  }

  /// The table, from the daemon's answers.
  List<ToolchainRow> rows() => [for (final d in detection) _row(d)];

  ToolchainRow _row(ProviderDetection d) {
    final info = providers
        .where((p) => p.id == d.providerId && p.trusted && (p.errors?.isEmpty ?? true))
        .firstOrNull;
    final required = d.tools.where((t) => t.required_).toList();
    final tools = [
      ...required.map((t) => t.toolId),
      ...d.tools.where((t) => !t.required_).map((t) => t.toolId),
    ];
    final firstFound =
        required.where((t) => t.found != null).firstOrNull ??
        d.tools.where((t) => t.found != null).firstOrNull;
    final allRequired = required.every((t) => t.found != null);
    final loc = firstFound?.found?.location;
    final source = switch (loc) {
      ToolLocationManaged(:final root) => (kind: 'managed', where: root),
      ToolLocationSystem(:final path) => (kind: 'system', where: path),
      _ => null,
    };
    final install = installs[d.providerId];
    final failure = failures[d.providerId];
    final kind = install != null
        ? RowKind.installing
        : failure != null
        ? RowKind.error
        : allRequired
        ? RowKind.ready
        : RowKind.missing;
    return ToolchainRow(
      providerId: d.providerId,
      name: info?.name ?? d.providerId,
      tools: tools,
      selected: d.selectedVersion,
      detected: firstFound?.found?.version,
      source: source,
      removable: source?.kind == 'managed' ? source!.where.split(RegExp(r'[\\/]')).last : null,
      kind: kind,
      progress: install?.progress,
      error: failure,
    );
  }

  /// Rows that need the person (missing or failed): the rail's badge.
  int get needAttention =>
      rows().where((r) => r.kind == RowKind.missing || r.kind == RowKind.error).length;

  /// The folder's own providers the person has not trusted yet (listed, never loaded).
  List<ProviderInfo> get untrusted => [
    for (final p in providers)
      if (p.source == ProviderSource.workspace && !p.trusted) p,
  ];

  /// Manifests that failed validation.
  List<ProviderInfo> get invalid => [
    for (final p in providers)
      if ((p.errors?.length ?? 0) > 0) p,
  ];

  /// The providers alone (their names), without detecting anything: cheap enough for opening a
  /// folder (the language server line names the provider it offers).
  Future<void> loadProviders() async {
    if (!workspace.isOpen || providers.isNotEmpty) return;
    try {
      final r = await client.call(Methods.envProviders, ProvidersParams(workspaceId: workspace.id));
      if (providers.isEmpty) {
        providers = r.providers;
        notifyListeners();
      }
    } on DaemonException {
      // Ids stand in for names.
    }
  }

  Future<void> refresh() async {
    if (!workspace.isOpen) return;
    final gen = ++_generation;
    status = status == 'ready' ? 'ready' : 'loading';
    error = '';
    notifyListeners();
    try {
      final ws = workspace.id;
      final r = await Future.wait([
        client.call(Methods.envProviders, ProvidersParams(workspaceId: ws)),
        client.call(Methods.envDetect, WorkspaceRef(workspaceId: ws)),
      ]);
      if (gen != _generation) return;
      providers = (r[0] as ProvidersResult).providers;
      detection = (r[1] as DetectResult).providers;
      status = 'ready';
      notifyListeners();
    } on DaemonException catch (e) {
      if (gen != _generation) return;
      status = 'error';
      error = describeEnvError(e);
      notifyListeners();
    }
  }

  Future<void> trustWorkspace(bool trusted) async {
    if (!workspace.isOpen) return;
    try {
      await client.call(Methods.envTrust, TrustParams(workspaceId: workspace.id, trusted: trusted));
    } on DaemonException catch (e) {
      error = describeEnvError(e);
    }
    await refresh();
  }

  Future<void> uninstall(String providerId, String version) async {
    try {
      await client.call(
        Methods.envUninstall,
        UninstallParams(providerId: providerId, version: version),
      );
      failures = {...failures}..remove(providerId);
    } on DaemonException catch (e) {
      failures = {...failures, providerId: describeEnvError(e)};
    }
    await refresh();
  }

  /// Asks the daemon for a plan: the selected version, else the newest available.
  /// Plans [providerId]'s install; with [approve] runs it at once (no dialog).
  Future<void> planInstall(String providerId, {bool approve = false}) async {
    if (approve) _autoApprove.add(providerId);
    prerequisites = {...prerequisites}..remove(providerId);
    final row = rows().where((r) => r.providerId == providerId).firstOrNull;
    planning = providerId;
    notifyListeners();
    try {
      var version = row?.selected;
      if (version == null || version == 'system') {
        final v = await client.call(Methods.envVersions, VersionsParams(providerId: providerId));
        version = v.available.firstOrNull;
      }
      if (version == null) {
        throw DaemonException(ErrorKind.notFound, 'The provider lists no installable versions.');
      }
      final p = await client.call(
        Methods.envPlanInstall,
        PlanInstallParams(providerId: providerId, version: version),
      );
      final name =
          row?.name ?? providers.where((x) => x.id == providerId).firstOrNull?.name ?? providerId;
      plan = (plan: p, providerName: name);
      planning = null;
      notifyListeners();
      if (approve) await approvePlan();
    } on DaemonException catch (e) {
      planning = null;
      _autoApprove.remove(providerId);
      failures = {...failures, providerId: describeEnvError(e)};
      if (e.kind == ErrorKind.prerequisiteMissing) {
        final d = e.details is Map ? (e.details! as Map)['missing'] : null;
        prerequisites = {
          ...prerequisites,
          providerId: [if (d is List) ...d.whereType<String>()],
        };
      }
      notifyListeners();
    }
  }

  void dismissPlan() {
    plan = null;
    notifyListeners();
  }

  /// Runs the shown plan (the approval). Completes when the install ends.
  Future<void> approvePlan() async {
    final shown = plan;
    if (shown == null) return;
    final planId = shown.plan.planId;
    final providerId = shown.plan.providerId;
    _planProvider[planId] = providerId;
    failures = {...failures}..remove(providerId);
    plan = null;
    installs = {...installs, providerId: (planId: planId, progress: null)};
    final cancel = _cancels[providerId] = CancelToken();
    notifyListeners();
    var ok = false;
    try {
      await client.call(
        Methods.envInstall,
        InstallParams(planId: planId),
        timeout: const Duration(hours: 1),
        cancel: cancel,
      );
      ok = true;
    } on DaemonException catch (e) {
      failures = {...failures, providerId: describeEnvError(e)};
    } finally {
      _autoApprove.remove(providerId);
      if (ok) onInstalled?.call(providerId);
      _cancels.remove(providerId);
      _planProvider.remove(planId);
      installs = {...installs}..remove(providerId);
      notifyListeners();
      await refresh();
    }
  }

  void cancelInstall(String providerId) => _cancels[providerId]?.cancel();

  /// The install that is running, if any (the transport shows it).
  ({String providerId, String name, InstallProgress? progress})? get activeInstall {
    if (installs.isEmpty) return null;
    final e = installs.entries.first;
    final name = providers.where((p) => p.id == e.key).firstOrNull?.name ?? e.key;
    return (providerId: e.key, name: name, progress: e.value.progress);
  }

  @override
  void dispose() {
    for (final off in _offs) {
      off();
    }
    workspace.removeListener(_onWorkspace);
    super.dispose();
  }
}
