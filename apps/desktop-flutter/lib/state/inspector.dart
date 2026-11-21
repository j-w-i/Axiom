// The Visual Inspector's state (M5-UI-001; ADR-0019, ADR-0020 §8): the folder's preview (its
// dev server and URL, the page's connection), inspect mode, the person's selection in the page
// with its details, and the latest verification an agent ran. Everything comes from `preview/*`
// and `ui/*`; the page itself is in the person's browser, where the adapter draws the highlight.

import 'dart:async';

import 'package:flutter/foundation.dart';

import '../daemon/client.dart';
import '../daemon/protocol.g.dart';
import 'workspace.dart';

/// How a source candidate is presented: `exact` ones open at their line and column; anything
/// else is a guess and says so.
bool isExact(SourceCandidate c) =>
    c.method == MappingMethod.buildInstrumentation && c.confidence >= 1.0 && c.generated != true;

/// `src/App.jsx:11:9`.
String locationLabel(Location l) => '${l.file}:${l.line}${l.column == null ? '' : ':${l.column}'}';

/// A candidate in words: `src/App.jsx:11:9` for an exact one, else the relation and confidence.
String candidateLabel(SourceCandidate c) {
  final at = locationLabel(c.location);
  if (isExact(c)) return at;
  final how = switch (c.relation) {
    SourceRelation.owner => 'rendered by',
    SourceRelation.definition => 'defined in',
    SourceRelation.style => 'styled in',
    SourceRelation.instantiation => 'created in',
  };
  return 'Uncertain: $how $at (${(c.confidence * 100).round()}% confident)';
}

/// One node in a line: `button "Pay now"`, `section#card`.
String nodeLabel(UiNode n) {
  final b = StringBuffer(n.typeName);
  if (n.name != null) b.write('#${n.name}');
  final t = n.text?.trim();
  if (t != null && t.isNotEmpty) {
    b.write(' "${t.length > 40 ? '${t.substring(0, 40)}…' : t}"');
  }
  return b.toString();
}

/// A computed property's value as text.
String propertyText(Object? v) => v is String ? v : '$v';

/// The verdict in words.
String verdictLabel(Verdict v) => switch (v) {
  Verdict.verified => 'Verified',
  Verdict.failed => 'Failed',
  Verdict.inconclusive => 'Inconclusive',
};

/// A `preview/*` or `ui/*` failure as one sentence for the panel.
String describeInspectorError(Object e) {
  if (e is! DaemonException) return '$e';
  return switch (e.kind) {
    ErrorKind.adapterUnavailable => 'The page is not connected: open the preview in a browser.',
    _ => e.message,
  };
}

class InspectorStore extends ChangeNotifier {
  InspectorStore(this.client, this.workspace) {
    _offs = [
      client.on(Notifications.previewState, (p) {
        if (!workspace.isOpen || p.workspaceId != workspace.id) return;
        _setPreview(p.state == PreviewState.stopped ? null : p);
      }),
      client.on(Notifications.uiSelected, (s) {
        if (s.previewId != preview?.previewId || s.chain.isEmpty) return;
        selection = s.chain;
        details = null;
        notifyListeners();
        unawaited(_loadDetails(s.chain.first));
      }),
      client.on(Notifications.uiHovered, (h) {
        if (h.previewId != preview?.previewId) return;
        hovered = h.node;
        notifyListeners();
      }),
      client.on(Notifications.agentEvent, _onAgentEvent),
    ];
    workspace.addListener(_onWorkspace);
  }

  final DaemonClient client;
  final WorkspaceStore workspace;
  late final List<void Function()> _offs;

  /// The folder's preview, while it runs (or failed).
  PreviewInfo? preview;
  bool starting = false;
  String error = '';
  bool inspectMode = false;

  /// The person's selection: `selection[0]` the node, then its ancestors.
  List<UiNode> selection = const [];

  /// `ui/nodeDetails` of the node shown (properties, every candidate).
  UiNode? details;

  /// The element under the pointer in inspect mode.
  UiNode? hovered;

  /// The latest `ui.verify` an agent ran in this folder, and when (ms since the epoch).
  ({VerificationReport report, int atMs})? report;

  bool get connected => preview?.adapter != null;
  bool get running => preview?.state == PreviewState.ready;

  /// The node shown in the panel (details once loaded).
  UiNode? get shown => details ?? (selection.isEmpty ? null : selection.first);

  final _verifyCalls = <String>{};
  int _generation = 0;
  String? _lastWs;

  void _onWorkspace() {
    final id = workspace.isOpen ? workspace.id : null;
    if (id == _lastWs) return;
    _lastWs = id;
    _generation++;
    preview = null;
    starting = false;
    error = '';
    inspectMode = false;
    selection = const [];
    details = null;
    hovered = null;
    report = null;
    _verifyCalls.clear();
    notifyListeners();
    if (id != null) unawaited(_loadRunning(id, _generation));
  }

  Future<void> _loadRunning(String ws, int gen) async {
    try {
      final l = await client.call(Methods.previewList, WorkspaceRef(workspaceId: ws));
      if (gen != _generation) return;
      _setPreview(l.previews.isEmpty ? null : l.previews.first);
    } on DaemonException {
      // No previews: nothing to show.
    }
  }

  void _setPreview(PreviewInfo? p) {
    final was = preview?.previewId;
    preview = p;
    if (p == null || p.previewId != was || p.adapter == null) {
      // A new page (or none): the old selection and mode no longer apply.
      if (p == null || p.previewId != was) {
        selection = const [];
        details = null;
      }
      hovered = null;
      inspectMode = false;
    }
    notifyListeners();
  }

  /// Starts the folder's preview (the daemon returns the running one if there is one).
  Future<void> start() async {
    if (!workspace.isOpen || starting) return;
    final gen = _generation;
    starting = true;
    error = '';
    notifyListeners();
    try {
      final p = await client.call(
        Methods.previewStart,
        StartParams2(workspaceId: workspace.id),
        timeout: const Duration(seconds: 90),
      );
      if (gen != _generation) return;
      _setPreview(p);
    } on DaemonException catch (e) {
      if (gen != _generation) return;
      error = describeInspectorError(e);
    } finally {
      if (gen == _generation) {
        starting = false;
        notifyListeners();
      }
    }
  }

  Future<void> stop() async {
    final p = preview;
    if (p == null) return;
    try {
      await client.call(Methods.previewStop, PreviewRef(previewId: p.previewId));
      _setPreview(null);
    } on DaemonException catch (e) {
      error = describeInspectorError(e);
      notifyListeners();
    }
  }

  Future<void> setInspectMode(bool on) async {
    final p = preview;
    if (p == null) return;
    try {
      await client.call(Methods.uiInspectMode, InspectModeParams(previewId: p.previewId, on_: on));
      inspectMode = on;
      if (!on) hovered = null;
      error = '';
    } on DaemonException catch (e) {
      error = describeInspectorError(e);
    }
    notifyListeners();
  }

  /// Shows an ancestor from the hierarchy (and highlights it in the page).
  Future<void> select(int index) async {
    if (index < 0 || index >= selection.length) return;
    final chain = selection.sublist(index);
    selection = chain;
    details = null;
    notifyListeners();
    await _loadDetails(chain.first);
    final p = preview;
    if (p == null) return;
    try {
      await client.call(
        Methods.uiHighlight,
        HighlightParams(previewId: p.previewId, node: chain.first.id),
      );
    } on DaemonException {
      // The page went away: the panel still shows the node.
    }
  }

  Future<void> _loadDetails(UiNode n) async {
    final p = preview;
    if (p == null) return;
    try {
      final d = await client.call(
        Methods.uiNodeDetails,
        NodeParams(previewId: p.previewId, node: n.id),
      );
      if (selection.isNotEmpty && selection.first.id == n.id) {
        details = d;
        notifyListeners();
      }
    } on DaemonException {
      // The page reloaded under it: keep what the selection carried.
    }
  }

  void _onAgentEvent(AgentEvent e) {
    switch (e.event) {
      case EventBodyToolCall(:final callId, :final tool) when tool == 'ui.verify':
        _verifyCalls.add(callId);
      case EventBodyToolResult(:final callId, :final isError, :final output)
          when _verifyCalls.remove(callId) && !isError && output != null:
        try {
          report = (report: VerificationReport.fromJson(output), atMs: e.tsMs);
          notifyListeners();
        } on Object {
          // Not a report (an older daemon): nothing to show.
        }
      default:
    }
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
