// Offering a missing language server, and installing it on its own when the person turned that on
// (M5.5-UI-001, ADR-0021 §5). One coordinator for the editor's line and the auto-install switch.

import 'dart:async';

import 'package:flutter/foundation.dart';

import '../daemon/protocol.g.dart';
import '../editor/lsp/lsp_editor.dart';
import 'documents.dart';
import 'environments.dart';
import 'lsp.dart';
import 'settings.dart';

/// What a prerequisite tool is supplied by (a delegate's runtime, a server's SDK).
const prerequisiteProviders = {
  'npm': 'node',
  'node': 'node',
  'go': 'go',
  'gofmt': 'go',
  'cargo': 'rust',
  'rustc': 'rust',
  'java': 'java',
  'dart': 'dart',
  'dotnet': 'dotnet',
};

/// The offer for one document: its missing server and what to install.
@immutable
class ServerOffer {
  const ServerOffer({
    required this.language,
    required this.languageName,
    required this.server,
    required this.providerId,
    required this.providerName,
  });
  final String language;
  final String languageName;
  final ServerInfo server;

  /// What to install: the server's provider, or what it requires.
  final String providerId;
  final String providerName;
}

class ServerInstalls extends ChangeNotifier {
  ServerInstalls({
    required this.documents,
    required this.lsp,
    required this.environments,
    required this.settings,
  }) {
    for (final l in [documents, lsp, environments, settings]) {
      l.addListener(_changed);
    }
  }

  final DocumentsStore documents;
  final LspStore lsp;
  final EnvironmentsStore environments;
  final SettingsStore settings;

  /// Languages the person said "Not now" to, for this session.
  final _dismissed = <String>{};

  /// Providers Axiom installed (or tried to) on its own, so a failure is not retried in a loop.
  final _autoTried = <String>{};
  bool _providersAsked = false;

  void dismiss(String language) {
    _dismissed.add(language);
    notifyListeners();
  }

  /// The offer for [d], if its language has a server that is missing.
  ServerOffer? offerFor(Doc d) {
    final lang = lspLanguageId(d);
    if (_dismissed.contains(lang)) return null;
    final s = lsp.serverFor(lang);
    if (s == null || s.state != ServerState.missing) return null;
    final pid = s.installProvider ?? s.providerId;
    return ServerOffer(
      language: lang,
      languageName: d.language.name,
      server: s,
      providerId: pid,
      providerName: nameOf(pid),
    );
  }

  String nameOf(String providerId) =>
      environments.providers.where((p) => p.id == providerId).firstOrNull?.name ?? providerId;

  /// The provider that supplies what [providerId]'s last plan was missing, if any.
  String? prerequisiteOf(String providerId) {
    for (final tool in environments.prerequisites[providerId] ?? const <String>[]) {
      final p = prerequisiteProviders[tool];
      if (p != null && p != providerId) return p;
    }
    return null;
  }

  /// Install… (the plan dialog), or the prerequisite first.
  Future<void> install(String providerId) => environments.planInstall(providerId);

  void _changed() {
    // The providers' names, once an offer needs them (not detection: that runs tools).
    if (!_providersAsked && environments.providers.isEmpty && documents.active != null) {
      final d = documents.active!;
      if (lsp.serverFor(lspLanguageId(d))?.state == ServerState.missing) {
        _providersAsked = true;
        unawaited(environments.loadProviders());
      }
    }
    notifyListeners();
    if (settings.lspAutoInstall) _autoInstall();
  }

  void _autoInstall() {
    final d = documents.active;
    if (d == null) return;
    final offer = offerFor(d);
    if (offer == null) return;
    final pid = offer.providerId;
    if (environments.installs.isNotEmpty || environments.planning != null) return;
    final pre = prerequisiteOf(pid);
    if (pre != null && _autoTried.add('pre:$pre')) {
      unawaited(environments.planInstall(pre, approve: true));
      return;
    }
    if (environments.failures.containsKey(pid) && pre == null) return;
    if (_autoTried.add(pid) || (pre != null && _autoTried.add('$pid after $pre'))) {
      unawaited(environments.planInstall(pid, approve: true));
    }
  }

  @override
  void dispose() {
    for (final l in [documents, lsp, environments, settings]) {
      l.removeListener(_changed);
    }
    super.dispose();
  }
}
