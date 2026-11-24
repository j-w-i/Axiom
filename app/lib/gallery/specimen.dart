/// Specimen content for the gallery and its tests. None of it is live and nothing here is shipped
/// in the product: `lib/main.dart` does not import it (test/gallery_isolation_test.dart).
library;

import '../patterns/models.dart';
import '../ui/icons.dart';
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
    state: AgentState.running,
    model: 'GPT-5.6',
    harness: 'Codex',
    connection: 'ChatGPT Pro, work',
    elapsed: '9:55',
    context: .71,
    contextProvenance: Provenance.reported,
    filesChanged: 3,
    recent: [
      ActivityView(AxIcon.terminal, 'pnpm test auth', detail: '2 failed', failed: true),
      ActivityView(AxIcon.edit, 'Editing refresh.ts'),
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
    state: AgentState.asking,
    model: 'Qwen3 4B',
    harness: 'Axiom',
    connection: 'LM Studio, this Mac',
    elapsed: '2:20',
    context: .16,
    recent: [
      ActivityView(AxIcon.terminal, 'vitest', detail: '118 passed'),
      ActivityView(AxIcon.shield, 'Asks to run a migration'),
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
  agent: 'Tester',
  what: 'asks to run outside the sandbox',
  subject: 'pnpm db:migrate --env test',
  scope: 'Writes db/test.sqlite. No network.',
  always: 'Allow in this worktree',
);

const services = [
  ServiceView(
    name: 'Claude',
    active: 'c1',
    accounts: [
      AccountView(
        id: 'c1',
        name: 'Max, personal',
        identity: 'sam@example.com',
        windows: [
          UsageWindow('5 hours', .62, '62%', Provenance.reported, resets: 'in 2 h 10 min'),
          UsageWindow('7 days', .41, '41%', Provenance.reported, resets: 'Friday'),
        ],
      ),
      AccountView(
        id: 'c2',
        name: 'Team, work',
        identity: 'sam@example.org',
        windows: [UsageWindow('5 hours', .08, '8%', Provenance.reported, resets: 'in 4 h')],
      ),
      AccountView(
        id: 'c3',
        name: 'API key',
        identity: 'workspace meridian',
        windows: [
          UsageWindow('This month', .18, r'$1.84 of $10', Provenance.estimated, resets: '1 Dec'),
        ],
      ),
    ],
  ),
  ServiceView(
    name: 'ChatGPT',
    active: 'o1',
    accounts: [
      AccountView(
        id: 'o1',
        name: 'Pro, work',
        identity: 'sam@example.org',
        windows: [UsageWindow('Weekly', .31, '31%', Provenance.reported, resets: 'Sunday')],
      ),
      AccountView(
        id: 'o2',
        name: 'Plus, personal',
        identity: 'sam@example.com',
        problem: 'Signed out. Sign in again to use it.',
        windows: [],
      ),
    ],
  ),
  ServiceView(
    name: 'Google',
    active: 'g1',
    accounts: [
      AccountView(
        id: 'g1',
        name: 'AI Pro',
        identity: 'sam@example.com',
        windows: [UsageWindow('Daily', null, 'not reported', Provenance.unknown)],
      ),
    ],
  ),
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
  AxMenuItem('axiom', 'Axiom', detail: 'native engine'),
  AxMenuItem('claude', 'Claude Code', detail: 'installed'),
  AxMenuItem('codex', 'Codex', detail: 'installed'),
  AxMenuItem('opencode', 'OpenCode', enabled: false, why: 'not found'),
];
const models = [AxMenuItem('gpt', 'GPT-5.6'), AxMenuItem('gptm', 'GPT-5.6 mini')];
const connections = [
  AxMenuItem('o1', 'ChatGPT Pro, work', detail: '31% used'),
  AxMenuItem('o2', 'ChatGPT Plus, personal', enabled: false, why: 'signed out'),
  AxMenuItem('ok', 'OpenAI API key', detail: 'metered'),
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
  ServiceView withActive(String id) => ServiceView(name: v.name, accounts: v.accounts, active: id);
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
    TaskView('Migrate sessions', TaskState.active, owner: 'Tester'),
    TaskView('Independent review', TaskState.todo, owner: 'Reviewer'),
  ],
);
final otherSessions = [
  SessionView(id: 's2', title: 'Queue backpressure', age: '2h', agents: [agents[0], agents[3]]),
  const SessionView(id: 's3', title: 'Release notes 4.2', age: '1d'),
  const SessionView(id: 's4', title: 'Flaky export test', age: '3d'),
];
