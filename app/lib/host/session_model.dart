import 'package:flutter/foundation.dart';

import '../transcript/markdown.dart';
import '../transcript/safe_text.dart';
import 'proto.dart';

/// One item of a transcript. Notifies when its own text or state changes, and only then.
class ItemModel extends ChangeNotifier {
  ItemModel(this.id, this.agent, this.run, this.kind, String name, this.args, {this.argsCut = 0})
    : name = oneLine(name, 60);
  final String id, agent, run, name;
  final ItemKind kind;
  final Object? args;
  Object? result;

  /// Bytes that were too many for the host to keep.
  final int argsCut;
  int resultCut = 0;
  Outcome? outcome;
  bool get open => outcome == null;

  /// For prose, beside its source.
  late final MarkdownDoc doc = MarkdownDoc();
  final StringBuffer _plain = StringBuffer();

  /// A running hash of the text so far: an ended item's text is compared with what was streamed
  /// without putting that together.
  int _length = 0, _hash = 0;
  String? _ended;
  bool get rich => switch (kind) {
    ItemKind.prose || ItemKind.prompt || ItemKind.reasoning || ItemKind.plan => true,
    _ => false,
  };
  String get plain => _ended ?? (_now ??= _plain.toString());

  /// What [plain] was last, until more comes: it is asked on every build of its row.
  String? _now;

  /// What was said, as it was written: Markdown, with whatever hides in it.
  String get source => plain;

  static int _fold(int h, String t) {
    for (var i = 0; i < t.length; i++) {
      h = (h * 31 + t.codeUnitAt(i)) & 0x3FFFFFFFFFFF;
    }
    return h;
  }

  void _append(String t) {
    _length += t.length;
    _hash = _fold(_hash, t);
    if (rich) doc.append(t);
    _plain.write(t);
    _now = null;
  }

  void _end(ItemEnded e) {
    outcome = e.outcome;
    result = e.result;
    resultCut = e.resultCut;
    if (rich) {
      // `text` is authoritative: if what was streamed differs in any way, it replaces it.
      if (_length != e.text.length || _hash != _fold(0, e.text)) {
        doc.reset();
        doc.append(e.text);
      }
      doc.finish();
    }
    _ended = e.text;
    _plain.clear();
    _now = null;
  }

  void _changed() => notifyListeners();
}

class AgentModel extends ChangeNotifier {
  AgentModel(
    this.id,
    String name,
    this.execution,
    this.parent,
    this.since, {
    this.access,
    this.apart = false,
    this.planning = false,
  }) : name = oneLine(name, 60);
  final String id, name;
  Execution execution;

  /// Null: an agent that does nothing here, as a replayed one.
  Access? access;

  /// It works in a copy of the folder of its own.
  final bool apart;

  /// It only reads, and hands in a plan.
  bool planning;

  /// The plan that waits for its person: the last it handed in, if nothing has been said since.
  ItemModel? get proposed {
    if (!planning || run != null) return null;
    for (final i in items.reversed) {
      if (i.kind == ItemKind.prompt) return null;
      if (i.kind == ItemKind.plan && i.agent == id && !i.open) return i;
    }
    return null;
  }

  /// The run it is in.
  String? run;
  final String? parent;
  final int since;
  AgentPhase phase = AgentPhase.waiting;
  ItemModel? last;

  /// What it says that it means to do. Its claim, and nothing more.
  List<Todo> steps = const [];
  String? stepsText;
  bool get plans => steps.isNotEmpty || stepsText != null;
  double? context;
  Known contextKnown = Known.unknown;

  /// How its last run ended, if it ended badly; what the provider or harness said.
  Outcome? ended;
  String? error;
  final List<ItemModel> items = [];

  /// Where its conversation can be branched: what a person said that began a run, and the last answer
  /// of one that was completed. Not of a program: it keeps its own conversation.
  final _places = <ItemModel>{};
  bool canBranch(ItemModel i) => execution.harness == 'axiom' && _places.contains(i);

  void _begins(ItemModel i) {
    if (i.kind != ItemKind.prompt || i.name.isNotEmpty) return;
    // What others have come to may be told with it.
    final before = items.reversed.where((o) => o.kind != ItemKind.prompt || o.name.isEmpty);
    if (before.firstOrNull?.run != i.run) _places.add(i);
  }

  void _answered(String run) {
    for (final i in items.reversed) {
      if (i.run != run) return;
      if (i.kind == ItemKind.prose) {
        _places.add(i);
        return;
      }
    }
  }

  /// It goes on from another's conversation: those items are its own too.
  void _goesOn(AgentModel of, Branched from) {
    final at = of.items.indexWhere((i) => i.id == from.item);
    if (at < 0) return;
    items.addAll(of.items.take(from.kept ? at + 1 : at));
    _places.addAll(items.where(of._places.contains));
    last = items.lastOrNull;
    items.add(
      ItemModel('$id from', id, '', ItemKind.notice, '', null)
        ..outcome = Outcome.completed
        .._ended = 'Branched from ${of.name}',
    );
  }

  /// What its person has said to it, the last first, the same only once in a row.
  List<String> get said {
    final all = <String>[];
    for (final i in items.reversed) {
      if (i.kind != ItemKind.prompt || i.name.isNotEmpty || i.open) continue;
      if (all.isEmpty || all.last != i.source) all.add(i.source);
      if (all.length == 200) break;
    }
    return all;
  }

  void _changed() => notifyListeners();
}

class RequestModel {
  const RequestModel(this.opened);
  final RequestOpened opened;
}

/// A session, rebuilt from its events. [apply] takes a whole batch and notifies each listener at
/// most once for it.
class SessionModel extends ChangeNotifier {
  SessionModel(this.id);
  final String id;
  String title = '';
  String? workspace;

  /// Follow from here after a reconnect.
  int seq = 0;
  int unknown = 0;

  /// Set when the host cannot read or write the session.
  String? fault;

  /// False until the host has sent what it had.
  bool loaded = false;
  final Map<String, AgentModel> agents = {};
  final Map<String, ItemModel> _open = {};
  final Map<String, String> _runs = {};
  final Map<String, RequestModel> requests = {};

  void apply(List<Event> events) {
    final items = <ItemModel>{}, touched = <AgentModel>{};
    var self = !loaded;
    loaded = true;
    for (final e in events) {
      // Never twice: a follow may overlap what is already here.
      if (e.seq <= seq) continue;
      seq = e.seq;
      self = _one(e, items, touched) || self;
    }
    for (final i in items) {
      i._changed();
    }
    for (final a in touched) {
      a._changed();
    }
    if (self) notifyListeners();
  }

  void failed(String words) {
    fault = words;
    notifyListeners();
  }

  /// True if the session itself changed.
  bool _one(Event e, Set<ItemModel> items, Set<AgentModel> touched) {
    switch (e.body) {
      case SessionCreated(:final title, :final workspace):
        this.title = oneLine(title);
        this.workspace = workspace;
        return true;
      case SessionTitled(:final title):
        this.title = oneLine(title);
        return true;
      case AgentAdded(
        :final agent,
        :final name,
        :final execution,
        :final parent,
        :final access,
        :final apart,
        :final from,
        :final plan,
      ):
        final a = AgentModel(
          agent,
          name,
          execution,
          parent,
          e.at,
          access: access,
          apart: apart,
          planning: plan,
        );
        if (from != null) {
          if (agents[from.agent] case final of?) a._goesOn(of, from);
        }
        agents[agent] = a;
        return true;
      case AgentChanged(:final agent, :final execution, :final access, :final plan):
        if (agents[agent] case final a?) {
          a
            ..planning = plan
            ..execution = execution
            ..access = access;
          touched.add(a);
          return true;
        }
      case AgentStateChanged(:final agent, :final state):
        if (agents[agent] case final a?) {
          a.phase = state;
          touched.add(a);
          return true;
        }
      case RunStarted(:final run, :final agent):
        _runs[run] = agent;
        if (agents[agent] case final a?) {
          a
            ..run = run
            ..ended = null
            ..error = null;
          touched.add(a);
        }
        return true;
      case RunEnded(:final run, :final outcome, :final error):
        if (agents[_runs.remove(run)] case final a?) {
          if (outcome == Outcome.completed) a._answered(run);
          a
            ..run = null
            ..ended = outcome
            ..error = error == null ? null : visible(error);
          // Whatever its last state said, nothing of it runs any more.
          if (a.phase == AgentPhase.running || a.phase == AgentPhase.asking) {
            a.phase = switch (outcome) {
              Outcome.completed => AgentPhase.done,
              Outcome.failed => AgentPhase.failed,
              _ => AgentPhase.paused,
            };
          }
          touched.add(a);
        }
        return true;
      case ItemStarted(
        :final item,
        :final agent,
        :final run,
        :final kind,
        :final name,
        :final args,
        :final argsCut,
      ):
        final a = agents[agent];
        if (a == null) {
          unknown++;
          return false;
        }
        final i = ItemModel(item, agent, run, kind, name, args, argsCut: argsCut);
        _open[item] = i;
        a._begins(i);
        a.items.add(i);
        a.last = i;
        touched.add(a);
      case ItemDelta(:final item, :final text):
        if (_open[item] case final i?) {
          i._append(text);
          items.add(i);
        }
      case final ItemEnded b:
        if (_open.remove(b.item) case final i?) {
          i._end(b);
          items.add(i);
          if (agents[i.agent] case final a?) touched.add(a);
        }
      case final RequestOpened b:
        requests[b.request] = RequestModel(b);
        return true;
      case RequestClosed(:final request):
        requests.remove(request);
        return true;
      case UsageReported(:final agent, :final usage, :final provenance, :final cut):
        // An answer that broke off says nothing of how full the context is.
        if (agents[agent] case final a? when !cut) {
          a
            ..context = usage.context
            ..contextKnown = provenance;
          touched.add(a);
        }
      case TodoSaid(:final agent, :final steps, :final text):
        if (agents[agent] case final a?) {
          a
            ..steps = steps
            ..stepsText = text;
          touched.add(a);
        }
      case UnknownBody():
        unknown++;
    }
    return false;
  }
}
