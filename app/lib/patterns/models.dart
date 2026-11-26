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
    this.service,
  });

  /// Whose it is, where its model's name does not say so.
  final ServiceMark? service;
  final String id, name, model, harness, connection, elapsed;
  final AgentState state;

  ServiceMark? get mark => service ?? ServiceMark.of(model);

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
    this.id = '',
  });

  @override
  bool operator ==(Object other) =>
      other is RequestView &&
      other.id == id &&
      other.agent == agent &&
      other.what == what &&
      other.subject == subject &&
      other.scope == scope &&
      other.always == always;
  @override
  int get hashCode => Object.hash(id, agent, what, subject, scope, always);

  /// [subject] is the exact command, path or address. [scope] says what allowing it would permit.
  final String agent, what, subject, scope;

  /// What allowing it for longer is called; null if it can be allowed only once.
  final String? always;

  /// Two questions of the same words are two questions.
  final String id;
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
  const DiffLine(this.kind, this.number, this.text, {this.was = 0, this.start = false});
  final DiffKind kind;

  /// Where it is now; of a removed line, where it was.
  final int number;
  final String text;

  /// Of a line that is the same on both sides: where it was. 0 where that is not known.
  final int was;

  /// The first line of a part of the diff that does not follow on from the one before.
  final bool start;

  @override
  bool operator ==(Object other) =>
      other is DiffLine &&
      other.kind == kind &&
      other.number == number &&
      other.text == text &&
      other.was == was;
  @override
  int get hashCode => Object.hash(kind, number, text, was);
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
    this.working = false,
  });

  /// Something of it runs. Who, is known of the one that is open only.
  final bool working;
  final String id, title, age;
  final List<AgentView> agents;
  final List<TaskView> tasks;
}
