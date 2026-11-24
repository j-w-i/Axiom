/// The wire, as Dart sees it. `engine/crates/proto` is the source; `test/host/proto_test.dart`
/// decodes what it writes, so the two cannot drift apart unnoticed.
library;

const int protocol = 1;

T _enum<T extends Enum>(List<T> all, Object? v, T other) =>
    all.where((e) => e.name == v).firstOrNull ?? other;

enum AgentPhase { waiting, running, asking, paused, done, failed, unknown }

enum Outcome { completed, failed, cancelled, interrupted, unknown }

enum ItemKind { prompt, prose, reasoning, tool, notice, unknown }

enum Known { reported, measured, estimated, unknown }

enum Answerable { live, message, no, unknown }

typedef Json = Map<String, Object?>;

class Execution {
  const Execution({
    required this.harness,
    this.provider,
    this.model,
    this.connection,
    this.effort,
    this.environment,
  });
  Execution.decode(Json j)
    : harness = j['harness']! as String,
      provider = j['provider'] as String?,
      model = j['model'] as String?,
      connection = j['connection'] as String?,
      effort = j['effort'] as String?,
      environment = j['environment'] as String?;
  final String harness;
  final String? provider, model, connection, effort, environment;
}

class Usage {
  Usage.decode(Json j)
    : input = j['input'] as int?,
      cacheRead = j['cache_read'] as int?,
      cacheWrite = j['cache_write'] as int?,
      output = j['output'] as int?,
      reasoning = j['reasoning'] as int?,
      context = (j['context'] as num?)?.toDouble();
  final int? input, cacheRead, cacheWrite, output, reasoning;
  final double? context;
}

sealed class Body {
  const Body();

  /// A body this build does not know, or one that does not decode, becomes [UnknownBody].
  static Body decode(Json j) {
    final type = j['type'];
    try {
      return switch (type) {
        'session.created' => SessionCreated(j['title']! as String, j['workspace'] as String?),
        'session.titled' => SessionTitled(j['title']! as String),
        'agent.added' => AgentAdded(
          j['agent']! as String,
          j['name']! as String,
          Execution.decode(j['execution']! as Json),
          j['parent'] as String?,
        ),
        'agent.state' => AgentStateChanged(
          j['agent']! as String,
          _enum(AgentPhase.values, j['state'], AgentPhase.unknown),
        ),
        'run.started' => RunStarted(j['run']! as String, j['agent']! as String),
        'run.ended' => RunEnded(
          j['run']! as String,
          _enum(Outcome.values, j['outcome'], Outcome.unknown),
          j['error'] as String?,
        ),
        'item.started' => ItemStarted(
          j['item']! as String,
          j['run']! as String,
          j['agent']! as String,
          _enum(ItemKind.values, j['kind'], ItemKind.unknown),
          j['name'] as String? ?? '',
          j['args'],
        ),
        'item.delta' => ItemDelta(j['item']! as String, j['text']! as String),
        'item.ended' => ItemEnded(
          j['item']! as String,
          _enum(Outcome.values, j['outcome'], Outcome.unknown),
          j['text']! as String,
          j['result'],
        ),
        'request.opened' => RequestOpened(
          j['request']! as String,
          j['agent']! as String,
          j['what']! as String,
          j['subject']! as String,
          j['scope']! as String,
          _enum(Answerable.values, j['answerable'], Answerable.unknown),
        ),
        'request.closed' => RequestClosed(j['request']! as String, j['answer']! as String),
        'usage' => UsageReported(
          j['agent']! as String,
          Usage.decode(j['usage']! as Json),
          _enum(Known.values, j['provenance'], Known.unknown),
        ),
        _ => UnknownBody('$type'),
      };
    } on TypeError {
      return UnknownBody('$type');
    }
  }

  static const types = {
    'session.created',
    'session.titled',
    'agent.added',
    'agent.state',
    'run.started',
    'run.ended',
    'item.started',
    'item.delta',
    'item.ended',
    'request.opened',
    'request.closed',
    'usage',
  };
}

class SessionCreated extends Body {
  const SessionCreated(this.title, this.workspace);
  final String title;
  final String? workspace;
}

class SessionTitled extends Body {
  const SessionTitled(this.title);
  final String title;
}

class AgentAdded extends Body {
  const AgentAdded(this.agent, this.name, this.execution, this.parent);
  final String agent, name;
  final Execution execution;
  final String? parent;
}

class AgentStateChanged extends Body {
  const AgentStateChanged(this.agent, this.state);
  final String agent;
  final AgentPhase state;
}

class RunStarted extends Body {
  const RunStarted(this.run, this.agent);
  final String run, agent;
}

class RunEnded extends Body {
  const RunEnded(this.run, this.outcome, this.error);
  final String run;
  final Outcome outcome;
  final String? error;
}

class ItemStarted extends Body {
  const ItemStarted(this.item, this.run, this.agent, this.kind, this.name, this.args);
  final String item, run, agent, name;
  final ItemKind kind;
  final Object? args;
}

class ItemDelta extends Body {
  const ItemDelta(this.item, this.text);
  final String item, text;
}

class ItemEnded extends Body {
  const ItemEnded(this.item, this.outcome, this.text, this.result);
  final String item, text;
  final Outcome outcome;
  final Object? result;
}

class RequestOpened extends Body {
  const RequestOpened(
    this.request,
    this.agent,
    this.what,
    this.subject,
    this.scope,
    this.answerable,
  );
  final String request, agent, what, subject, scope;
  final Answerable answerable;
}

class RequestClosed extends Body {
  const RequestClosed(this.request, this.answer);
  final String request, answer;
}

class UsageReported extends Body {
  const UsageReported(this.agent, this.usage, this.provenance);
  final String agent;
  final Usage usage;
  final Known provenance;
}

class UnknownBody extends Body {
  const UnknownBody(this.type);
  final String type;
}

class Event {
  const Event(this.seq, this.at, this.body);
  Event.decode(Json j)
    : seq = j['seq']! as int,
      at = j['at']! as int,
      body = Body.decode(j['body']! as Json);
  final int seq, at;
  final Body body;
}

class SessionInfo {
  SessionInfo.decode(Json j)
    : id = j['id']! as String,
      title = j['title']! as String,
      workspace = j['workspace'] as String?,
      created = j['created']! as int,
      updated = j['updated']! as int,
      lastSeq = j['last_seq']! as int,
      running = j['running']! as bool;
  final String id, title;
  final String? workspace;
  final int created, updated, lastSeq;
  final bool running;
}

sealed class Message {
  const Message();
  static Message decode(Json j) {
    if (j case {'session': final String s, 'events': final List<Object?> e}) {
      return Events(s, [for (final x in e) Event.decode(x! as Json)]);
    }
    if (j case {'session': final String s, 'resync': final int n}) return Resync(s, n);
    if (j case {'session': final String s, 'fault': final String f}) return Fault(s, f);
    if (j case {'id': final int id, 'error': final Json e}) {
      return Failed(id, e['code'] as String? ?? 'unknown', e['message'] as String? ?? '');
    }
    if (j case {'id': final int id}) return Answered(id, j['result']);
    throw FormatException('not a message', j);
  }
}

class Events extends Message {
  const Events(this.session, this.events);
  final String session;
  final List<Event> events;
}

class Resync extends Message {
  const Resync(this.session, this.last);
  final String session;
  final int last;
}

/// The host cannot read or write this session. Nothing more comes for it.
class Fault extends Message {
  const Fault(this.session, this.words);
  final String session, words;
}

class Answered extends Message {
  const Answered(this.id, this.result);
  final int id;
  final Object? result;
}

class Failed extends Message implements Exception {
  const Failed(this.id, this.code, this.message);
  final int id;
  final String code, message;
  @override
  String toString() => message;
}
