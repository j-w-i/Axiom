import 'package:flutter/foundation.dart';

import '../transcript/markdown.dart';
import 'proto.dart';

/// One item of a transcript. Notifies when its own text or state changes, and only then.
class ItemModel extends ChangeNotifier {
  ItemModel(this.id, this.agent, this.run, this.kind, this.name, this.args);
  final String id, agent, run, name;
  final ItemKind kind;
  final Object? args;
  Object? result;
  Outcome? outcome;
  bool get open => outcome == null;

  /// For prose. Tools keep plain text.
  late final MarkdownDoc doc = MarkdownDoc();
  final StringBuffer _plain = StringBuffer();
  int _length = 0;
  String? _ended;
  bool get rich => kind == ItemKind.prose || kind == ItemKind.prompt || kind == ItemKind.reasoning;
  String get plain => _ended ?? _plain.toString();

  void _append(String t) {
    _length += t.length;
    rich ? doc.append(t) : _plain.write(t);
  }

  void _end(ItemEnded e) {
    outcome = e.outcome;
    result = e.result;
    if (rich) {
      // Deltas already gave this text unless history was folded or cut short.
      if (_length != e.text.length) {
        doc.reset();
        doc.append(e.text);
      }
      doc.finish();
    } else {
      _ended = e.text;
      _plain.clear();
    }
  }

  void changed() => notifyListeners();
}

class AgentModel extends ChangeNotifier {
  AgentModel(this.id, this.name, this.execution, this.parent, this.since);
  final String id, name;
  final Execution execution;
  final String? parent;
  final int since;
  AgentPhase phase = AgentPhase.waiting;
  ItemModel? last;
  double? context;
  Known contextKnown = Known.unknown;
  final List<ItemModel> items = [];
  void changed() => notifyListeners();
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
  final Map<String, AgentModel> agents = {};
  final Map<String, ItemModel> _open = {};
  final Map<String, RequestModel> requests = {};

  void apply(List<Event> events) {
    final touched = <ChangeNotifier>{};
    for (final e in events) {
      // Never twice: a follow may overlap what is already here.
      if (e.seq <= seq) continue;
      seq = e.seq;
      _one(e, touched);
    }
    for (final t in touched) {
      switch (t) {
        case final ItemModel i:
          i.changed();
        case final AgentModel a:
          a.changed();
        case _:
          notifyListeners();
      }
    }
  }

  void _one(Event e, Set<ChangeNotifier> touched) {
    switch (e.body) {
      case SessionCreated(:final title, :final workspace):
        this.title = title;
        this.workspace = workspace;
        touched.add(this);
      case SessionTitled(:final title):
        this.title = title;
        touched.add(this);
      case AgentAdded(:final agent, :final name, :final execution, :final parent):
        agents[agent] = AgentModel(agent, name, execution, parent, e.at);
        touched.add(this);
      case AgentStateChanged(:final agent, :final state):
        if (agents[agent] case final a?) {
          a.phase = state;
          touched
            ..add(a)
            ..add(this);
        }
      case RunStarted() || RunEnded():
        touched.add(this);
      case ItemStarted(
        :final item,
        :final agent,
        :final run,
        :final kind,
        :final name,
        :final args,
      ):
        final a = agents[agent];
        if (a == null) {
          unknown++;
          return;
        }
        final i = ItemModel(item, agent, run, kind, name, args);
        _open[item] = i;
        a.items.add(i);
        a.last = i;
        touched.add(a);
      case ItemDelta(:final item, :final text):
        if (_open[item] case final i?) {
          i._append(text);
          touched.add(i);
        }
      case final ItemEnded b:
        if (_open.remove(b.item) case final i?) {
          i._end(b);
          touched.add(i);
          if (agents[i.agent] case final a?) touched.add(a);
        }
      case final RequestOpened b:
        requests[b.request] = RequestModel(b);
        touched.add(this);
      case RequestClosed(:final request):
        requests.remove(request);
        touched.add(this);
      case UsageReported(:final agent, :final usage, :final provenance):
        if (agents[agent] case final a?) {
          a
            ..context = usage.context
            ..contextKnown = provenance;
          touched.add(a);
        }
      case UnknownBody():
        unknown++;
    }
  }
}
