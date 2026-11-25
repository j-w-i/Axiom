/// Specimen content for the gallery and its tests. None of it is live and nothing here is shipped
/// in the product: `lib/main.dart` does not import it (test/gallery_isolation_test.dart).
library;

import '../host/proto.dart' as p;
import '../host/session_model.dart';
import '../patterns/context_gauge.dart';
import '../patterns/models.dart';
import '../ui/icons.dart';
import '../ui/marks.dart';
import '../ui/overlay.dart';
import '../ui/status.dart';

const agents = [
  AgentView(
    id: 'lead',
    name: 'Lead',
    state: AgentState.running,
    model: 'Opus 5.5',
    harness: 'Axiom',
    connection: 'Anthropic API',
    elapsed: '12:41',
    context: .34,
    recent: [
      ActivityView(AxIcon.delegate, 'Assigned review', detail: 'to Reviewer'),
      ActivityView(AxIcon.think, 'Reading review notes'),
    ],
  ),
  AgentView(
    id: 'architect',
    name: 'Architect',
    state: AgentState.done,
    model: 'Opus 5.5',
    harness: 'Claude Code',
    connection: 'Claude Max, personal',
    elapsed: '6:08',
    context: .52,
    contextProvenance: Provenance.reported,
    filesChanged: 1,
    recent: [
      ActivityView(AxIcon.search, 'Searched rotate(', detail: '14 hits'),
      ActivityView(AxIcon.edit, 'Wrote docs/refresh.md'),
    ],
  ),
  AgentView(
    id: 'builder',
    name: 'Builder',
    state: AgentState.asking,
    model: 'GPT-5.6',
    harness: 'Codex',
    connection: 'ChatGPT Pro, work',
    elapsed: '9:55',
    context: .71,
    contextProvenance: Provenance.reported,
    filesChanged: 3,
    recent: [
      ActivityView(AxIcon.terminal, 'pnpm test auth', detail: '2 failed', failed: true),
      ActivityView(AxIcon.shield, 'Asks to run a command'),
    ],
  ),
  AgentView(
    id: 'surveyor',
    name: 'Surveyor',
    state: AgentState.running,
    model: 'Gemini 3.5 Pro',
    harness: 'Antigravity',
    connection: 'Google AI Pro',
    elapsed: '3:17',
    recent: [
      ActivityView(AxIcon.file, 'Read queue/worker.ts'),
      ActivityView(AxIcon.file, 'Reading queue/retry.ts'),
    ],
  ),
  AgentView(
    id: 'tester',
    name: 'Tester',
    state: AgentState.waiting,
    model: 'Qwen3 4B',
    harness: 'Axiom',
    connection: 'LM Studio, this Mac',
    elapsed: '2:20',
    context: .16,
    recent: [
      ActivityView(AxIcon.terminal, 'vitest', detail: '118 passed'),
      ActivityView(AxIcon.list, 'Waits for Builder'),
    ],
  ),
  AgentView(
    id: 'reviewer',
    name: 'Reviewer',
    state: AgentState.waiting,
    model: 'Sonnet 5.5',
    harness: 'Claude Code',
    connection: 'Claude Max, team',
    elapsed: '',
    context: 0,
    recent: [ActivityView(AxIcon.list, 'Waits for Builder')],
  ),
];

const request = RequestView(
  agent: 'Builder',
  what: 'asks to run outside the sandbox',
  subject: 'pnpm db:migrate --env test',
  scope: 'Writes db/test.sqlite. No network.',
  always: 'Allow for this session',
);

const services = [
  ServiceView(
    name: 'Claude',
    mark: ServiceMark.claude,
    active: 'c1',
    accounts: [
      AccountView(
        id: 'c1',
        name: 'Max, personal',
        lane: 'plan, through Claude Code',
        identity: 'sam@example.com',
        windows: [
          UsageWindow('5 hours', .62, '62%', Provenance.reported, resets: 'in 2 h 10 min'),
          UsageWindow('7 days', .41, '41%', Provenance.reported, resets: 'Friday'),
        ],
      ),
      AccountView(
        id: 'c2',
        name: 'Team, work',
        lane: 'plan, through Claude Code',
        identity: 'sam@example.org',
        windows: [UsageWindow('5 hours', .08, '8%', Provenance.reported, resets: 'in 4 h')],
      ),
      AccountView(
        id: 'c3',
        name: 'Company key',
        lane: 'API key',
        identity: 'sk-…3f2a',
        windows: [
          UsageWindow('This month', .18, r'$1.84 of $10', Provenance.estimated, resets: '1 Dec'),
        ],
      ),
    ],
  ),
  ServiceView(
    name: 'ChatGPT',
    mark: ServiceMark.openai,
    active: 'o1',
    accounts: [
      AccountView(
        id: 'o1',
        name: 'Pro, work',
        lane: 'plan sign-in',
        identity: 'sam@example.org',
        windows: [UsageWindow('Weekly', .31, '31%', Provenance.reported, resets: 'Sunday')],
      ),
      AccountView(
        id: 'o2',
        name: 'Plus, personal',
        lane: 'plan sign-in',
        identity: 'sam@example.com',
        problem: 'Signed out. Sign in again to use it.',
        windows: [],
      ),
    ],
  ),
  ServiceView(
    name: 'Google',
    mark: ServiceMark.gemini,
    active: 'g1',
    accounts: [
      AccountView(
        id: 'g1',
        name: 'AI Pro',
        lane: 'plan, through Antigravity',
        identity: 'sam@example.com',
        windows: [UsageWindow('Daily', null, 'not reported', Provenance.unknown)],
      ),
    ],
  ),
];

const changed = [
  ('src/auth/refresh.ts', 7, 2),
  ('src/auth/inflight.ts', 31, 0),
  ('test/refresh.test.ts', 19, 0),
];
const diff = [
  DiffLine(DiffKind.same, 41, 'async refresh(account: Account) {'),
  DiffLine(DiffKind.remove, 42, '  if (!expired(session.token)) return session;'),
  DiffLine(DiffKind.remove, 43, '  const lock = await this.locks.acquire(account.id);'),
  DiffLine(DiffKind.add, 42, '  return this.inflight.run(account.id, async () => {'),
  DiffLine(DiffKind.add, 43, '    await using lock = await this.locks.acquire(account.id);'),
  DiffLine(DiffKind.add, 44, '    const session = await this.store.read(account.id);'),
  DiffLine(DiffKind.add, 45, '    if (!expired(session.token)) return session;'),
  DiffLine(DiffKind.same, 46, '    const next = await this.provider.rotate(session);'),
  DiffLine(DiffKind.add, 47, '    await this.store.write(account.id, next);'),
  DiffLine(DiffKind.add, 48, '    return next;'),
  DiffLine(DiffKind.add, 49, '  });'),
  DiffLine(DiffKind.same, 50, '}'),
];

const harnesses = [
  AxMenuItem('axiom', 'Axiom', mark: ServiceMark.axiom, detail: 'native engine'),
  AxMenuItem('claude', 'Claude Code', mark: ServiceMark.claudecode, detail: 'installed'),
  AxMenuItem('codex', 'Codex', mark: ServiceMark.codex, detail: 'installed'),
  AxMenuItem('opencode', 'OpenCode', mark: ServiceMark.opencode, enabled: false, why: 'not found'),
];
const models = [
  AxMenuItem('gpt', 'GPT-5.6', mark: ServiceMark.openai),
  AxMenuItem('gptm', 'GPT-5.6 mini', mark: ServiceMark.openai),
];
const harnessesForGpt = [
  AxMenuItem('axiom', 'Axiom', mark: ServiceMark.axiom, detail: 'native engine'),
  AxMenuItem('codex', 'Codex', mark: ServiceMark.codex, detail: 'installed'),
  AxMenuItem(
    'claude',
    'Claude Code',
    mark: ServiceMark.claudecode,
    enabled: false,
    why: 'Claude models only',
  ),
  AxMenuItem('opencode', 'OpenCode', mark: ServiceMark.opencode, enabled: false, why: 'not found'),
];
const axiomModels = [
  ...models,
  AxMenuItem('opus', 'Opus 5.5', mark: ServiceMark.claude),
  AxMenuItem('gem', 'Gemini 3.5 Pro', mark: ServiceMark.gemini),
];
const otherConnections = {
  'opus': [AxMenuItem('ck', 'Anthropic API key', mark: ServiceMark.claude, detail: 'metered')],
  'gem': [AxMenuItem('gk', 'Gemini API key', mark: ServiceMark.gemini, detail: 'metered')],
};
const connections = [
  AxMenuItem('o1', 'ChatGPT Pro, work', mark: ServiceMark.openai, detail: '31% used'),
  AxMenuItem(
    'o2',
    'ChatGPT Plus, personal',
    mark: ServiceMark.openai,
    enabled: false,
    why: 'signed out',
  ),
  AxMenuItem('ok', 'OpenAI API key', mark: ServiceMark.openai, detail: 'metered'),
];
const efforts = [
  AxMenuItem('low', 'Low effort'),
  AxMenuItem('medium', 'Medium effort'),
  AxMenuItem('high', 'High effort'),
];
const access = [
  AxMenuItem('ask', 'Ask for everything'),
  AxMenuItem('tree', 'Worktree only'),
  AxMenuItem('full', 'Full access', enabled: false, why: 'untrusted folder'),
];

extension type ServiceViewX(ServiceView v) {
  ServiceView withActive(String id) =>
      ServiceView(name: v.name, mark: v.mark, accounts: v.accounts, active: id);
}

const session = SessionView(
  id: 's1',
  title: 'Double token rotation',
  age: '12:41',
  agents: agents,
  tasks: [
    TaskView('Reproduce rotation', TaskState.done, owner: 'Tester'),
    TaskView('Survey call sites', TaskState.done, owner: 'Surveyor'),
    TaskView('Design note', TaskState.done, owner: 'Architect'),
    TaskView('Failing test', TaskState.done, owner: 'Tester'),
    TaskView('Single-flight guard', TaskState.active, owner: 'Builder'),
    TaskView('Migrate sessions', TaskState.todo, owner: 'Tester'),
    TaskView('Independent review', TaskState.todo, owner: 'Reviewer'),
  ],
);
final otherSessions = [
  SessionView(id: 's2', title: 'Queue backpressure', age: '2h', agents: [agents[0], agents[3]]),
  const SessionView(id: 's3', title: 'Release notes 4.2', age: '1d'),
  const SessionView(id: 's4', title: 'Flaky export test', age: '3d'),
];

/// The specimen conversation, through the same model and view as a real one.
SessionModel transcript() {
  var n = 0;
  const x = p.Execution(harness: 'codex', model: 'GPT-5.6');
  final e = <p.Body>[
    const p.SessionCreated('Double token rotation', null),
    const p.AgentAdded('builder', 'Builder', x, null),
  ];
  void say(String id, p.ItemKind k, String text) => e
    ..add(p.ItemStarted(id, 'r', 'builder', k, '', null))
    ..add(p.ItemEnded(id, p.Outcome.completed, text, null));
  void work(
    String id,
    String name,
    Map<String, Object?> args,
    String summary, {
    bool failed = false,
  }) => e
    ..add(p.ItemStarted(id, 'r', 'builder', p.ItemKind.tool, name, args))
    ..add(
      p.ItemEnded(
        id,
        failed ? p.Outcome.failed : p.Outcome.completed,
        failed ? '2 failed, 116 passed' : '',
        {'summary': summary},
      ),
    );
  say(
    'a',
    p.ItemKind.prompt,
    'Two tabs can both see an expired token and rotate it twice. Follow `docs/refresh.md`: re-check '
        'expiry inside the lock and make rotation single-flight per account.',
  );
  work('b', 'read', {'path': 'docs/refresh.md'}, '84 lines');
  say(
    'c',
    p.ItemKind.prose,
    'I will read the refresh path and its callers first, then run the new test to see it fail.',
  );
  work('d', 'read', {'path': 'src/auth/refresh.ts'}, '220 lines');
  work('f', 'search', {'pattern': 'locks.acquire'}, '9 in 4 files');
  work('g', 'test', {'command': 'pnpm test auth'}, '2 failed', failed: true);
  say(
    'h',
    p.ItemKind.prose,
    'The expiry check happens **before** the lock is held, so the second tab acts on a token the first '
        'already replaced. I am moving the read inside the critical section and adding a single-flight '
        'map keyed by account.\n\n```ts\nasync refresh(account: Account) {\n  return this.inflight.run'
        '(account.id, async () => {\n    await using lock = await this.locks.acquire(account.id);\n    '
        'const session = await this.store.read(account.id);\n    if (!expired(session.token)) return '
        'session;\n  });\n}\n```',
  );
  work('i', 'edit', {'path': 'src/auth/refresh.ts'}, '+7 −2');
  work('j', 'edit', {'path': 'src/auth/inflight.ts'}, '+31 −0');
  e.add(const p.ItemStarted('l', 'r', 'builder', p.ItemKind.prose, '', null));
  e.add(
    const p.ItemDelta(
      'l',
      'Both writers now go through one entry point. Next is the migration, which adds a `rotated_at` column.',
    ),
  );
  return SessionModel('s1')..apply([for (final b in e) p.Event(++n, n, b)]);
}

const contextView = ContextView(
  share: .71,
  used: 142000,
  window: 200000,
  provenance: Provenance.reported,
  parts: [
    ContextPart('Instructions and tools', 19400),
    ContextPart('Conversation', 61200),
    ContextPart('Files read', 44800),
    ContextPart('Command output', 16600),
  ],
  note: 'Summarised at 85%.',
);
