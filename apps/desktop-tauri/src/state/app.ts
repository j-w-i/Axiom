// Composition root of the UI state (equivalent of the Qt `Shell` object):
// one DaemonClient and the four stores, wired the same way.

import { DaemonClient, type Transport } from "../daemon/client";
import { AgentStore } from "./agent";
import { DocumentsStore } from "./documents";
import { EnvironmentsStore } from "./environments";
import { GitStore } from "./git";
import { LspStore } from "./lsp";
import { ProblemsStore } from "./problems";
import { SettingsStore } from "./settings";
import { TasksStore } from "./tasks";
import { TerminalsStore } from "./terminals";
import { WorkspaceStore } from "./workspace";

export interface AppStores {
  client: DaemonClient;
  workspace: WorkspaceStore;
  documents: DocumentsStore;
  tasks: TasksStore;
  settings: SettingsStore;
  environments: EnvironmentsStore;
  problems: ProblemsStore;
  agent: AgentStore;
  lsp: LspStore;
  terminals: TerminalsStore;
  git: GitStore;
}

export function createApp(transport: Transport): AppStores {
  const client = new DaemonClient(transport);
  const workspace = new WorkspaceStore(client);
  const documents = new DocumentsStore(client, workspace);
  const tasks = new TasksStore(client, workspace);
  const settings = new SettingsStore(client);
  const environments = new EnvironmentsStore(client, workspace);
  const problems = new ProblemsStore(client, workspace);
  const agent = new AgentStore(client, workspace);
  const lsp = new LspStore(client, workspace);
  problems.attach(lsp);
  const terminals = new TerminalsStore(client, workspace);
  const git = new GitStore(client, workspace);
  tasks.onProviderStart = (key, taskId, label) => problems.begin(key, taskId, label);
  // Once per launch: reopen the last workspace when the setting says so.
  let reopened = false;
  client.onState((s) => {
    if (s.state === "ready" && !reopened) {
      reopened = true;
      void workspace.reopenLast();
    }
  });
  return { client, workspace, documents, tasks, settings, environments, problems, agent, lsp, terminals, git };
}
