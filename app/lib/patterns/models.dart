/// What the patterns draw. Plain immutable views; the host client produces them.
library;

import '../ui/icons.dart';
import '../ui/marks.dart';
import '../ui/status.dart';

class AgentView {
  const AgentView({
    required this.id,
    required this.name,
    required this.state,
    required this.model,
    required this.harness,
    required this.connection,
    required this.elapsed,
    this.context,
    this.contextProvenance = Provenance.measured,
    this.recent = const [],
    this.filesChanged = 0,
  });
  final String id, name, model, harness, connection, elapsed;
  final AgentState state;

  ServiceMark? get mark => ServiceMark.of(model);

  /// 0–1 of the context window, or null when the harness does not say.
  final double? context;
  final Provenance contextProvenance;
  final List<ActivityView> recent;
  final int filesChanged;
}

class ActivityView {
  const ActivityView(this.icon, this.text, {this.detail, this.failed = false});
  final AxIcon icon;
  final String text;
  final String? detail;
  final bool failed;
}

class RequestView {
  const RequestView({
    required this.agent,
    required this.what,
    required this.subject,
    required this.scope,
    required this.always,
  });

  /// [subject] is the exact command, path or address. [scope] says what allowing it would permit.
  final String agent, what, subject, scope, always;
}

/// One linked account. A service can have several; see docs/product/requirements.md PR-4.
class AccountView {
  const AccountView({
    required this.id,
    required this.name,
    required this.identity,
    required this.lane,
    required this.windows,
    this.problem,
  });
  final String id, name, identity;

  /// How it is reached: a plan, a key, or an agent installed on this machine (decision 0007).
  final String lane;
  final List<UsageWindow> windows;
  final String? problem;
}

class UsageWindow {
  const UsageWindow(this.name, this.used, this.text, this.provenance, {this.resets});
  final String name, text;
  final double? used;
  final String? resets;
  final Provenance provenance;
}

class ServiceView {
  const ServiceView({required this.name, required this.accounts, required this.active, this.mark});
  final String name, active;
  final ServiceMark? mark;
  final List<AccountView> accounts;
}

enum DiffKind { same, add, remove }

class DiffLine {
  const DiffLine(this.kind, this.number, this.text);
  final DiffKind kind;
  final int number;
  final String text;
}

enum TaskState { todo, active, asking, done, blocked }

class TaskView {
  const TaskView(this.title, this.state, {this.owner});
  final String title;
  final TaskState state;

  /// The agent it is assigned to.
  final String? owner;
}

class SessionView {
  const SessionView({
    required this.id,
    required this.title,
    required this.age,
    this.agents = const [],
    this.tasks = const [],
  });
  final String id, title, age;
  final List<AgentView> agents;
  final List<TaskView> tasks;
}
