/// The wire, as Dart sees it. `engine/crates/proto` is the source; `test/host/proto_test.dart`
/// decodes what it writes, so the two cannot drift apart unnoticed.
library;

const int protocol = 1;

T _enum<T extends Enum>(List<T> all, Object? v, T other) =>
    all.where((e) => e.name == v).firstOrNull ?? other;

enum AgentPhase { waiting, running, asking, paused, done, failed, unknown }

enum Outcome { completed, failed, cancelled, interrupted, unknown }

enum ItemKind { prompt, prose, reasoning, tool, notice, unknown }

/// What an agent may do without asking.
enum Access { read, ask, work, unknown }

/// What a person answers.
enum Answer { once, session, deny }

enum Known { reported, measured, estimated, unknown }

/// What a figure is the sum of. [session] is a running total.
enum Covers { answer, turn, session, unknown }

/// Who is paid, and how. At [plan], a cost is what it would have been by use.
enum Lane { key, plan, program, open, unknown }

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

  Json encode() => {
    'harness': harness,
    'provider': ?provider,
    'model': ?model,
    'connection': ?connection,
    'effort': ?effort,
    'environment': ?environment,
  };
}

/// An account, a key or a server: one way to reach a provider.
class Connection {
  const Connection(
    this.id,
    this.name,
    this.provider, {
    this.url,
    this.secret = false,
    this.lasts = false,
    this.signIn,
    this.problem,
  });
  Connection.decode(Json j)
    : id = j['id']! as String,
      name = j['name']! as String,
      provider = j['provider']! as String,
      url = j['url'] as String?,
      secret = j['secret'] as bool? ?? false,
      lasts = j['lasts'] as bool? ?? false,
      signIn = j['sign_in'] as String?,
      problem = j['problem'] as String?;

  /// Why it cannot be used until somebody signs in to it again.
  final String? problem;

  /// A plan that is signed in to here, and not a key.
  bool get plan => provider == 'chatgpt';

  /// Of an account of a program of the person's own: the command they sign in with, in a terminal.
  final String? signIn;
  final String id, name, provider;
  final String? url;

  /// Whether it has a secret, and whether the system keeps it when the host stops. The secret itself
  /// never comes back.
  final bool secret, lasts;
}

/// A sign-in on its way. [connection], [problem]: how it ended.
class Signin {
  const Signin(
    this.id,
    this.url,
    this.again, {
    this.finishing = false,
    this.connection,
    this.problem,
  });
  final String id, url;

  /// The account that is signed in to again.
  final String? again;

  /// Its person is done in the browser.
  final bool finishing;
  final String? connection, problem;
  bool get waits => connection == null && problem == null;

  Signin told(Json j) => switch (j['state']) {
    'done' => Signin(id, url, again, connection: j['connection']! as String),
    'failed' => Signin(id, url, again, problem: j['problem']! as String),
    'finishing' => Signin(id, url, again, finishing: true),
    _ => this,
  };
}

/// Words of a folder for agents: a file, and which content of it.
class Instructions {
  const Instructions(this.path, this.digest, this.text);
  Instructions.decode(Json j)
    : path = j['path']! as String,
      digest = j['digest']! as String,
      text = j['text'] as String? ?? '';
  final String path, digest, text;

  /// Named, for the host: it reads them itself.
  Json encode() => {'path': path, 'digest': digest};
}

/// What has been changed in a folder since its last commit.
class Changes {
  const Changes({
    this.repository = false,
    this.branch,
    this.files = const [],
    this.diff = '',
    this.whole = true,
  });
  Changes.decode(Json j)
    : repository = j['repository']! as bool,
      branch = j['branch'] as String?,
      files = [
        for (final f in j['files']! as List<Object?>)
          Changed((f! as Json)['path']! as String, (f as Json)['what']! as String),
      ],
      diff = j['diff']! as String,
      whole = j['whole']! as bool;
  final bool repository, whole;
  final String? branch;
  final List<Changed> files;
  final String diff;
}

class Changed {
  const Changed(this.path, this.what);
  final String path, what;
}

/// What an agent has changed in its own copy of the folder.
class CopyChanges {
  const CopyChanges({
    this.made = false,
    this.files = const [],
    this.whole = true,
    this.ignored = 0,
    this.odd = 0,
    this.back,
  });
  CopyChanges.decode(Json j)
    : made = j['made']! as bool,
      files = [for (final f in j['files']! as List<Object?>) CopyFile.decode(f! as Json)],
      whole = j['whole']! as bool,
      ignored = j['ignored'] as int? ?? 0,
      odd = j['odd'] as int? ?? 0,
      back = switch (j['back']) {
        {'id': final String id, 'files': final int n} => (id, n),
        _ => null,
      };

  /// There is one: it has been spoken to.
  final bool made, whole;
  final List<CopyFile> files;

  /// What git ignores and only the copy has, and what cannot be listed or was not copied.
  final int ignored, odd;

  /// The last that was brought in, and for how many files that can be put back.
  final (String, int)? back;
}

class CopyFile extends Changed {
  const CopyFile(
    super.path,
    super.what, {
    this.digest,
    this.run = false,
    this.there,
    this.diff,
    this.clash = false,
    this.unsettled = false,
    this.never,
    this.runs = false,
    this.others = const [],
  });
  CopyFile.decode(Json j)
    : digest = j['digest'] as String?,
      run = j['run'] as bool? ?? false,
      there = j['there'] as String?,
      diff = j['diff'] as String?,
      clash = j['clash'] as bool? ?? false,
      unsettled = j['unsettled'] as bool? ?? false,
      never = j['never'] as String?,
      runs = j['runs'] as bool? ?? false,
      others = [for (final o in j['others'] as List<Object?>? ?? const []) o! as String],
      super(j['path']! as String, j['what']! as String);

  /// What the copy has now, and the folder; null if it has none.
  final String? digest, there;

  /// It can be run.
  final bool run;

  /// What bringing it in would do to the folder; null if that cannot be shown.
  final String? diff;

  /// The folder's own has changed too: it is not brought in.
  final bool clash;

  /// The folder's own had changed too when the copy was brought up to date, and its agent has not
  /// touched it since.
  final bool unsettled;

  /// Why it is never brought in.
  final String? never;

  /// It runs when someone builds, tests or installs.
  final bool runs;

  /// Other agents that have changed it in their own copies.
  final List<String> others;
  bool get can => !clash && never == null;

  /// Without anything that somebody should look at first.
  bool get plain => can && !runs && !unsettled && diff != null;

  /// As it was shown.
  Json encode() => {'path': path, 'digest': ?digest, if (run) 'run': true, 'there': ?there};
}

/// What was done, and what was not, each with why.
class Moved {
  const Moved(this.done, this.left);
  Moved.decode(Json j, String done)
    : done = [for (final f in j[done]! as List<Object?>) f! as String],
      left = leftOf(j);
  static List<(String, String)> leftOf(Json j) => [
    for (final f in j['left']! as List<Object?>)
      ((f! as Json)['path']! as String, (f as Json)['why']! as String),
  ];
  final List<String> done;
  final List<(String, String)> left;
}

/// What it is that waits: a question, or what an agent came to that nobody has seen.
enum Need { asks, failed, limit, finished, unknown }

/// Something that waits for a person, wherever it is.
class Waits {
  const Waits(
    this.session,
    this.title,
    this.agent,
    this.name,
    this.request,
    this.what,
    this.subject,
    this.at, {
    this.need = Need.asks,
  });
  Waits.decode(Json j)
    : need = _enum(Need.values, j['need'] ?? 'asks', Need.unknown),
      session = j['session']! as String,
      title = j['title']! as String,
      agent = j['agent']! as String,
      name = j['name']! as String,
      request = j['request'] as String? ?? '',
      what = j['what'] as String? ?? '',
      subject = j['subject']! as String,
      at = j['at']! as int;
  final Need need;

  /// [subject]: how it begins. All of it is where it is asked.
  final String session, title, agent, name, request, what, subject;
  final int at;
}

/// The environment in which what an agent does is done by Axiom, whatever its harness.
const axiomTools = 'axiom';

/// A program of the person's own that the host can drive.
class HarnessInfo {
  const HarnessInfo(
    this.id,
    this.name, {
    this.program,
    this.problem,
    this.needsTrust = false,
    this.access = const [],
    this.accounts,
    this.hands = false,
    this.effort = false,
    this.own = false,
  });
  HarnessInfo.decode(Json j)
    : id = j['id']! as String,
      name = j['name']! as String,
      program = j['program'] as String?,
      problem = j['problem'] as String?,
      needsTrust = j['needs_trust'] as bool? ?? true,
      accounts = j['accounts'] as String?,
      hands = j['hands'] as bool? ?? false,
      effort = j['effort'] as bool? ?? false,
      own = j['own'] as bool? ?? false,
      access = [
        for (final a in j['access'] as List<Object?>? ?? const [])
          _enum(Access.values, a, Access.unknown),
      ];
  final String id, name;

  /// Null if it is not there or may not be started; then [problem] says why.
  final String? program, problem;

  /// Its person added it themself.
  final bool own;

  /// It can be told how hard to try.
  final bool effort;

  /// Axiom's tools can stand in place of its own: the environment [axiomTools].
  final bool hands;

  /// What an account of it beside the first is called as a provider, if there can be any.
  final String? accounts;

  /// It cannot be told to ignore a folder's own settings for it.
  final bool needsTrust;
  final List<Access> access;
}

/// How a program is started, as its person has said it, of it or of one account of it.
class ProgramInfo {
  const ProgramInfo(
    this.harness, {
    this.account,
    this.name,
    this.path,
    this.args = const [],
    this.vars = const [],
  });
  ProgramInfo.decode(Json j)
    : harness = j['harness']! as String,
      account = j['account'] as String?,
      name = j['name'] as String?,
      path = j['path'] as String?,
      args = [for (final a in j['args'] as List<Object?>? ?? const []) a! as String],
      vars = [for (final v in j['vars'] as List<Object?>? ?? const []) VarInfo.decode(v! as Json)];
  final String harness;
  final String? account, name, path;
  final List<String> args;
  final List<VarInfo> vars;
}

class VarInfo {
  const VarInfo(this.name, {this.value, this.lasts = false});
  VarInfo.decode(Json j)
    : name = j['name']! as String,
      value = j['value'] as String?,
      lasts = j['lasts'] as bool? ?? false;
  final String name;

  /// Null: it is a secret, and not shown.
  final String? value;

  /// It is there when the host has been restarted.
  final bool lasts;
}

/// A variable a program is to be given; without a value, the secret that is there stays.
class VarSet {
  const VarSet(this.name, {this.value, this.secret = false});
  final String name;
  final String? value;
  final bool secret;
  Json encode() => {'name': name, 'value': ?value, 'secret': secret};
}

/// What a provider says of a model.
class ModelInfo {
  const ModelInfo(this.id, {this.name, this.context, this.effort = false});
  ModelInfo.decode(Json j)
    : id = j['id']! as String,
      name = j['name'] as String?,
      context = j['context'] as int?,
      effort = j['effort'] as bool? ?? false;

  /// How hard it tries can be said.
  final bool effort;
  final String id;
  final String? name;
  final int? context;
}

enum LimitUnit { count, bytes, seconds, unknown }

/// A limit the host keeps: of what, and what it limits, in its own words.
class HostLimit {
  const HostLimit(this.of, this.what, this.most, this.unit);
  HostLimit.decode(Json j)
    : of = j['of']! as String,
      what = j['what']! as String,
      most = j['most'] as int?,
      unit = _enum(LimitUnit.values, j['unit'], LimitUnit.unknown);
  final String of, what;

  /// Null: there is none.
  final int? most;
  final LimitUnit unit;

  /// In the largest unit it is a whole number of.
  String get amount {
    final n = most;
    if (n == null) return 'No limit';
    String? whole(List<(int, String)> of) {
      for (final (size, name) in of) {
        if (n == 0 || n % size != 0) continue;
        final k = n ~/ size;
        return name.endsWith('B') || k == 1 ? '$k $name' : '$k ${name}s';
      }
      return null;
    }

    final named = switch (unit) {
      LimitUnit.bytes => whole([(1 << 30, 'GiB'), (1 << 20, 'MiB'), (1 << 10, 'KiB'), (1, 'B')]),
      LimitUnit.seconds => whole([(3600, 'hour'), (60, 'minute'), (1, 'second')]),
      LimitUnit.count || LimitUnit.unknown => null,
    };
    return named ?? '$n'.replaceAllMapped(RegExp(r'\B(?=(\d{3})+$)'), (_) => '\u{a0}');
  }
}

class Usage {
  Usage.decode(Json j)
    : prompt = j['prompt'] as int?,
      cacheWriteLong = j['cache_write_long'] as int?,
      input = j['input'] as int?,
      cacheRead = j['cache_read'] as int?,
      cacheWrite = j['cache_write'] as int?,
      output = j['output'] as int?,
      reasoning = j['reasoning'] as int?,
      context = (j['context'] as num?)?.toDouble();
  final int? input, cacheRead, cacheWrite, output, reasoning;

  /// The whole prompt; of what was written to a cache, what is kept for an hour.
  final int? prompt, cacheWriteLong;
  final double? context;
}

/// What was used, as it was then.
class UsedOn {
  UsedOn.decode(Json j)
    : harness = j['harness']! as String,
      lane = _enum(Lane.values, j['lane'], Lane.unknown),
      provider = j['provider'] as String?,
      connection = j['connection'] as String?,
      model = j['model'] as String?,
      answered = j['answered'] as String?,
      speed = j['speed'] as String?;
  final String harness;
  final Lane lane;

  /// [model] was asked for, [answered] is said to have answered.
  final String? provider, connection, model, answered, speed;
}

class Cost {
  Cost.decode(Json j)
    : amount = (j['amount']! as num).toDouble(),
      currency = j['currency']! as String,
      provenance = _enum(Known.values, j['provenance'], Known.unknown),
      of = _enum(Covers.values, j['of'], Covers.unknown);
  final double amount;
  final String currency;
  final Known provenance;
  final Covers of;
}

/// One model's part of everything so far; of no model, all of it.
class ModelTotal {
  ModelTotal.decode(Json j)
    : model = j['model'] as String?,
      usage = Usage.decode(j['usage']! as Json),
      cost = j['cost'] == null ? null : Cost.decode(j['cost']! as Json);
  final String? model;
  final Usage usage;
  final Cost? cost;
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
          access: j['access'] == null ? null : _enum(Access.values, j['access'], Access.unknown),
          apart: j['apart'] as bool? ?? false,
          from: switch (j['from']) {
            final Json f => Branched(
              f['agent']! as String,
              f['item']! as String,
              kept: f['with'] as bool? ?? false,
            ),
            _ => null,
          },
        ),
        'agent.changed' => AgentChanged(
          j['agent']! as String,
          Execution.decode(j['execution']! as Json),
          j['access'] == null ? null : _enum(Access.values, j['access'], Access.unknown),
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
          argsCut: j['args_cut'] as int? ?? 0,
        ),
        'item.delta' => ItemDelta(j['item']! as String, j['text']! as String),
        'item.ended' => ItemEnded(
          j['item']! as String,
          _enum(Outcome.values, j['outcome'], Outcome.unknown),
          j['text']! as String,
          j['result'],
          resultCut: j['result_cut'] as int? ?? 0,
        ),
        'request.opened' => RequestOpened(
          j['request']! as String,
          j['agent']! as String,
          j['what']! as String,
          j['subject']! as String,
          j['scope']! as String,
          _enum(Answerable.values, j['answerable'], Answerable.unknown),
          digest: j['digest'] as String? ?? '',
          item: j['item'] as String?,
          session: j['session'] as bool? ?? false,
        ),
        'request.closed' => RequestClosed(j['request']! as String, j['answer']! as String),
        'usage' => UsageReported(
          j['agent']! as String,
          Usage.decode(j['usage']! as Json),
          _enum(Known.values, j['provenance'], Known.unknown),
          of: _enum(Covers.values, j['of'], Covers.unknown),
          run: j['run'] as String?,
          on: j['on'] == null ? null : UsedOn.decode(j['on']! as Json),
          cost: j['cost'] == null ? null : Cost.decode(j['cost']! as Json),
          totals: [
            for (final t in (j['totals'] as List<Object?>?) ?? const [])
              ModelTotal.decode(t! as Json),
          ],
          cut: j['cut'] as bool? ?? false,
          aside: j['aside'] as bool? ?? false,
        ),
        'todo' => TodoSaid(
          j['agent']! as String,
          [for (final s in (j['steps'] as List<Object?>?) ?? const []) Todo.decode(s! as Json)],
          run: j['run'] as String?,
          text: j['text'] as String?,
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
    'agent.changed',
    'agent.state',
    'run.started',
    'run.ended',
    'item.started',
    'item.delta',
    'item.ended',
    'request.opened',
    'request.closed',
    'usage',
    'todo',
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

/// Where a branch begins: with another's items up to one, which is [kept] with them or not.
class Branched {
  const Branched(this.agent, this.item, {this.kept = false});
  final String agent, item;
  final bool kept;
}

/// Whether there can be a branch, and what of.
class BranchInfo {
  const BranchInfo({required this.apart, this.files = false});
  BranchInfo.decode(Json j) : this(apart: j['apart']! as bool, files: j['files'] as bool? ?? false);

  /// It would work in a copy of its own; what the files were there is kept.
  final bool apart, files;
}

class AgentAdded extends Body {
  const AgentAdded(
    this.agent,
    this.name,
    this.execution,
    this.parent, {
    this.access,
    this.apart = false,
    this.from,
  });

  /// It works in a copy of the folder of its own.
  final bool apart;

  /// It goes on from another's conversation.
  final Branched? from;
  final String agent, name;
  final Execution execution;
  final String? parent;
  final Access? access;
}

/// What it runs on or may do is another from here on.
class AgentChanged extends Body {
  const AgentChanged(this.agent, this.execution, this.access);
  final String agent;
  final Execution execution;
  final Access? access;
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
  const ItemStarted(
    this.item,
    this.run,
    this.agent,
    this.kind,
    this.name,
    this.args, {
    this.argsCut = 0,
  });

  /// How many bytes of arguments were too many to keep; then there are none.
  final int argsCut;
  final String item, run, agent, name;
  final ItemKind kind;
  final Object? args;
}

class ItemDelta extends Body {
  const ItemDelta(this.item, this.text);
  final String item, text;
}

class ItemEnded extends Body {
  const ItemEnded(this.item, this.outcome, this.text, this.result, {this.resultCut = 0});
  final int resultCut;
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
    this.answerable, {
    this.digest = '',
    this.item,
    this.session = false,
  });
  final String request, agent, what, subject, scope;
  final Answerable answerable;

  /// Names exactly this question; sent back with the answer (SI-2).
  final String digest;

  /// The tool call it is about.
  final String? item;

  /// Whether it can be allowed for the rest of the session.
  final bool session;
}

class RequestClosed extends Body {
  const RequestClosed(this.request, this.answer);
  final String request, answer;
}

class UsageReported extends Body {
  const UsageReported(
    this.agent,
    this.usage,
    this.provenance, {
    this.of = Covers.unknown,
    this.run,
    this.on,
    this.cost,
    this.totals = const [],
    this.cut = false,
    this.aside = false,
  });
  final String agent;
  final Usage usage;
  final Known provenance;

  /// What [usage] covers.
  final Covers of;
  final String? run;
  final UsedOn? on;
  final Cost? cost;
  final List<ModelTotal> totals;

  /// The answer broke off: there may have been more.
  final bool cut;

  /// Not for the conversation itself, but to summarise it.
  final bool aside;
}

enum TodoState { todo, doing, done, unknown }

/// One of the things an agent means to do.
class Todo {
  const Todo(this.text, this.state);
  Todo.decode(Json j)
    : text = j['text']! as String,
      state = _enum(TodoState.values, j['state'], TodoState.unknown);
  final String text;
  final TodoState state;
}

/// What an agent means to do, as it says itself: all of it, in place of what it said before.
class TodoSaid extends Body {
  const TodoSaid(this.agent, this.steps, {this.run, this.text});
  final String agent;
  final List<Todo> steps;
  final String? run;

  /// Markdown.
  final String? text;
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
    if (j case {'waiting': final int n}) return WaitingChanged(n);
    if (j case {'open': final String path}) return FolderOpened(path);
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

/// What waits for a person is something else now.
/// A person asked in a terminal for a folder to be shown.
class FolderOpened extends Message {
  const FolderOpened(this.path);
  final String path;
}

class WaitingChanged extends Message {
  const WaitingChanged(this.count);
  final int count;
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
