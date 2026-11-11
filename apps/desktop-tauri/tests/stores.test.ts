// UI stores against the real axiomd (no webview). Mirrors the scenarios of the
// Qt suites tst_workspace, tst_documents, tst_tasks and tst_settings so the
// parity matrix can cite equivalent evidence for both shells.

import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { DocumentStore, type ModelLike } from "@editor/documents";
import { createApp, type AppStores } from "../src/state/app";
import type { EditorPort } from "../src/state/documents";
import { OutputBuffer, stripAnsi } from "../src/state/tasks";
import { daemonPath, NodeTransport, waitFor } from "./support/node-transport";

const haveDaemon = existsSync(daemonPath());

/** A Monaco-less editor surface: the real bridge-1 DocumentStore over a string model. */
class FakeEditor implements EditorPort {
  readonly store: DocumentStore;
  private readonly models = new Map<string, StringModel>();
  constructor(listener: AppStores["documents"]) {
    this.store = new DocumentStore(listener, (path, text) => {
      const m = new StringModel(text);
      this.models.set(path, m);
      return m;
    });
  }
  type(path: string, text: string) {
    this.models.get(path)!.edit(text);
  }
  text(path: string) {
    return this.models.get(path)?.getValue();
  }
  configure() {}
  openDocument(p: Parameters<EditorPort["openDocument"]>[0]) { this.store.open(p); }
  activateDocument() {}
  closeDocument(p: { path: string }) { this.store.close(p.path); }
  requestSave(p: { path: string }) { this.store.requestSave(p.path); }
  documentSaved(p: { path: string }) { this.store.documentSaved(p.path); }
  saveFailed(p: { path: string }) { this.store.saveFailed(p.path); }
  revealPosition() {}
  runCommand() {}
}

class StringModel implements ModelLike {
  private version = 1;
  private readonly listeners = new Set<() => void>();
  constructor(private text: string) {}
  getValue() { return this.text; }
  setValue(t: string) { this.text = t; this.version++; for (const l of this.listeners) l(); }
  edit(t: string) { this.setValue(t); }
  getAlternativeVersionId() { return this.version; }
  onDidChangeContent(l: () => void) { this.listeners.add(l); return { dispose: () => this.listeners.delete(l) }; }
  dispose() {}
}

describe.skipIf(!haveDaemon)("stores against the real daemon", () => {
  let transport: NodeTransport;
  let app: AppStores;
  let ws: string;
  let editor: FakeEditor;

  async function boot() {
    transport = new NodeTransport();
    app = createApp(transport);
    editor = new FakeEditor(app.documents);
    app.documents.attachEditor(editor);
    app.documents.ready({ bridgeVersion: 1, monacoVersion: "test" });
    await app.client.connect();
    await waitFor(() => app.client.ready && app.settings.getSnapshot().loaded, 5000, "ready");
  }

  beforeEach(async () => {
    ws = mkdtempSync(join(tmpdir(), "axiom-ws-"));
    mkdirSync(join(ws, "src", "deep"), { recursive: true });
    writeFileSync(join(ws, "src", "main.rs"), "fn main() {}\n");
    writeFileSync(join(ws, "src", "deep", "x.txt"), "x\n");
    writeFileSync(join(ws, "README.md"), "# hi\n");
    writeFileSync(join(ws, ".gitignore"), "target/\n");
    mkdirSync(join(ws, "target"));
    await boot();
  });

  afterEach(async () => {
    await transport.dispose();
    rmSync(ws, { recursive: true, force: true });
  });

  const names = () => app.workspace.visibleRows().map((r) => r.node.path);

  it("opens a workspace and lists the root lazily with ignored entries flagged", async () => {
    await app.workspace.openFolder(ws);
    const s = app.workspace.getSnapshot();
    expect(s.id).not.toBe("");
    expect(s.rootLoaded).toBe(true);
    expect(names()).toEqual([".gitignore", "README.md", "src", "target"].sort((a, b) => {
      const da = a === "src" || a === "target", db = b === "src" || b === "target";
      return da === db ? a.toLowerCase().localeCompare(b.toLowerCase()) : da ? -1 : 1;
    }));
    expect(app.workspace.node("target")!.ignored).toBe(true);
    await waitFor(() => app.workspace.getSnapshot().recent.some((r) => r.root === s.root), 3000, "recent list");
    expect(app.workspace.node("src/main.rs")).toBeUndefined(); // not listed until expanded
    app.workspace.setExpanded("src", true);
    await waitFor(() => app.workspace.node("src/main.rs") !== undefined, 3000, "src listing");
  });

  it("re-lists an expanded directory when a file appears on disk, keeping expansion", async () => {
    await app.workspace.openFolder(ws);
    app.workspace.setExpanded("src", true);
    await waitFor(() => !!app.workspace.node("src/main.rs"));
    app.workspace.setExpanded("src/deep", true);
    await waitFor(() => !!app.workspace.node("src/deep/x.txt"));
    writeFileSync(join(ws, "src", "new.rs"), "\n");
    await waitFor(() => !!app.workspace.node("src/new.rs"), 3000, "new file to appear");
    expect(app.workspace.node("src/deep")!.expanded).toBe(true);
  });

  it("persists expanded folders, tabs and the active tab across a restart", async () => {
    await app.workspace.openFolder(ws);
    app.workspace.setExpanded("src", true);
    await waitFor(() => !!app.workspace.node("src/main.rs"));
    // Tab order follows readFile answers (as in the Qt model): open one at a time.
    app.documents.open("README.md");
    await waitFor(() => app.documents.getSnapshot().docs.length === 1);
    app.documents.open("src/main.rs");
    await waitFor(() => app.documents.getSnapshot().docs.length === 2);
    app.documents.activate("README.md");
    await app.workspace.flushState();
    await app.documents.saveTabState();
    await transport.stop();
    // Same data dir, new daemon and new stores: a full application restart.
    const dataDir = transport.dataDir;
    transport = Object.assign(new NodeTransport(), {});
    (transport as unknown as { dataDir: string }).dataDir = dataDir;
    app = createApp(transport);
    editor = new FakeEditor(app.documents);
    app.documents.attachEditor(editor);
    app.documents.ready({ bridgeVersion: 1, monacoVersion: "test" });
    await app.client.connect();
    // reopenLast is automatic.
    await waitFor(() => app.workspace.getSnapshot().rootLoaded, 5000, "reopen last workspace");
    await waitFor(() => !!app.workspace.node("src/main.rs"), 3000, "expanded src restored");
    await waitFor(() => app.documents.getSnapshot().docs.length === 2, 3000, "tabs restored");
    expect(app.documents.getSnapshot().docs.map((d) => d.path)).toEqual(["README.md", "src/main.rs"]);
    await waitFor(() => app.documents.getSnapshot().activePath === "README.md", 3000, "active tab");
  });

  it("edits, saves through the daemon, and the bytes on disk match", async () => {
    await app.workspace.openFolder(ws);
    app.documents.open("src/main.rs");
    await waitFor(() => editor.text("src/main.rs") !== undefined);
    editor.type("src/main.rs", "fn main() { println!(\"hi\"); }\n");
    await waitFor(() => app.documents.getSnapshot().docs[0]!.dirty, 2000, "dirty");
    app.documents.save("src/main.rs");
    await waitFor(() => !app.documents.getSnapshot().docs[0]!.dirty, 3000, "clean after save");
    expect(readFileSync(join(ws, "src", "main.rs"), "utf8")).toBe("fn main() { println!(\"hi\"); }\n");
  });

  it("reloads a clean document changed on disk; a dirty one conflicts and Overwrite wins", async () => {
    await app.workspace.openFolder(ws);
    app.documents.open("README.md");
    await waitFor(() => editor.text("README.md") === "# hi\n");
    writeFileSync(join(ws, "README.md"), "# external\n");
    await waitFor(() => editor.text("README.md") === "# external\n", 3000, "clean reload");

    editor.type("README.md", "# mine\n");
    await waitFor(() => app.documents.getSnapshot().docs[0]!.dirty);
    writeFileSync(join(ws, "README.md"), "# theirs\n");
    await waitFor(() => app.documents.getSnapshot().docs[0]!.changedOnDisk, 3000, "changed on disk");
    app.documents.save("README.md");
    await waitFor(() => app.documents.getSnapshot().conflictName === "README.md", 3000, "conflict");
    app.documents.resolveConflict("overwrite");
    await waitFor(() => readFileSync(join(ws, "README.md"), "utf8") === "# mine\n", 3000, "overwrite");
    await waitFor(() => !app.documents.getSnapshot().docs[0]!.dirty, 3000, "clean");
  });

  it("closing a dirty tab prompts; Discard closes without writing", async () => {
    await app.workspace.openFolder(ws);
    app.documents.open("README.md");
    await waitFor(() => editor.text("README.md") !== undefined);
    editor.type("README.md", "changed\n");
    await waitFor(() => app.documents.getSnapshot().docs[0]!.dirty);
    app.documents.close("README.md");
    expect(app.documents.getSnapshot().prompt?.kind).toBe("close");
    app.documents.resolvePrompt("discard");
    expect(app.documents.getSnapshot().docs).toHaveLength(0);
    expect(readFileSync(join(ws, "README.md"), "utf8")).toBe("# hi\n");
  });

  it("opening a folder that does not exist reports it", async () => {
    await app.workspace.openFolder(join(ws, "does-not-exist"));
    await waitFor(() => app.workspace.getSnapshot().error !== "", 5000, "error");
    expect(app.workspace.getSnapshot().error).toBe("The folder does not exist.");
    expect(app.workspace.isOpen).toBe(false);
  });

  it("an unreadable directory shows an inline error under it", async () => {
    mkdirSync(join(ws, "locked"));
    writeFileSync(join(ws, "locked", "secret.txt"), "x\n");
    chmodSync(join(ws, "locked"), 0o000);
    try {
      await app.workspace.openFolder(ws);
      app.workspace.setExpanded("locked", true);
      await waitFor(() => app.workspace.node("locked")?.error != null, 5000, "inline error");
      expect(app.workspace.node("locked")!.error).toBeTruthy();
    } finally {
      chmodSync(join(ws, "locked"), 0o755);
    }
  });

  it("opening a file instead of a folder reports an error", async () => {
    await app.workspace.openFolder(join(ws, "README.md"));
    await waitFor(() => app.workspace.getSnapshot().error !== "", 5000, "error banner");
    expect(app.workspace.isOpen).toBe(false);
  });

  it("refuses a non-UTF-8 file with a clear banner", async () => {
    writeFileSync(join(ws, "bin.dat"), Buffer.from([0xff, 0xfe, 0x00, 0x80]));
    await app.workspace.openFolder(ws);
    app.documents.open("bin.dat");
    await waitFor(() => app.documents.getSnapshot().banner !== "");
    expect(app.documents.getSnapshot().banner).toContain("not a UTF-8 text file");
  });

  it("runs a command, streams stdout/stderr distinctly and reports a failed exit", async () => {
    await app.workspace.openFolder(ws);
    app.tasks.run("echo out; echo err 1>&2; exit 3");
    await waitFor(() => !app.tasks.getSnapshot().running && app.tasks.getSnapshot().lastOutcome !== null, 5000);
    const rows = app.tasks.getSnapshot().rows;
    expect(rows.find((r) => r.text === "out")?.stream).toBe("stdout");
    expect(rows.find((r) => r.text === "err")?.stream).toBe("stderr");
    expect(rows.at(-1)!.text).toMatch(/Failed with exit code 3/);
    expect(app.tasks.getSnapshot().history[0]).toBe("echo out; echo err 1>&2; exit 3");
  });

  it("stops a long command (including its children) promptly", async () => {
    await app.workspace.openFolder(ws);
    app.tasks.run("sleep 30 & sleep 30; wait");
    await waitFor(() => app.tasks.getSnapshot().running);
    await new Promise((r) => setTimeout(r, 200));
    const t0 = Date.now();
    app.tasks.stop();
    await waitFor(() => app.tasks.getSnapshot().lastOutcome === "cancelled", 5000, "cancelled");
    expect(Date.now() - t0).toBeLessThan(5000);
  });

  it("clear empties the output, rerunLast repeats the last command, and a run is refused while running", async () => {
    await app.workspace.openFolder(ws);
    app.tasks.run("echo one");
    await waitFor(() => !app.tasks.getSnapshot().running && app.tasks.getSnapshot().rows.length > 0, 5000);
    expect(app.tasks.getSnapshot().history[0]).toBe("echo one");
    app.tasks.clear();
    expect(app.tasks.getSnapshot().rows).toHaveLength(0);
    expect(app.tasks.getSnapshot().history).toHaveLength(1); // history survives Clear
    app.tasks.rerunLast();
    await waitFor(() => app.tasks.getSnapshot().rows.some((r) => r.text === "one"), 5000, "rerun");
    app.tasks.run("sleep 5");
    await waitFor(() => app.tasks.getSnapshot().running);
    const historyWhileRunning = app.tasks.getSnapshot().history;
    app.tasks.rerunLast(); // refused: a task is already running
    expect(app.tasks.getSnapshot().history).toEqual(historyWhileRunning);
    app.tasks.stop();
    await waitFor(() => !app.tasks.getSnapshot().running, 8000, "stopped");
  }, 20_000);

  it("reports spawn_failed for a missing program", async () => {
    await app.workspace.openFolder(ws);
    app.tasks.runArgv(["definitely-not-a-program-axiom"]);
    await waitFor(() => app.tasks.getSnapshot().statusText === "Failed to start");
    expect(app.tasks.getSnapshot().rows[0]!.kind).toBe("error");
    expect(app.tasks.getSnapshot().rows[0]!.text).toContain("definitely-not-a-program-axiom");
  });

  it("unsaved text comes back from the host snapshot after a webview crash", async () => {
    // The host keeps the snapshots; a crash replaces the page but not the host.
    const hostSnapshots = new Map<string, string>();
    const recovery = {
      snapshotPut: (path: string, text: string) => void hostSnapshots.set(path, text),
      snapshotClear: (path: string) => void hostSnapshots.delete(path),
      state: async () => ({ recovered: true, snapshots: Object.fromEntries(hostSnapshots) }),
    };
    app.documents.recovery = recovery;
    await app.workspace.openFolder(ws);
    app.documents.open("README.md");
    await waitFor(() => editor.text("README.md") !== undefined);
    editor.type("README.md", "# unsaved work\n");
    await waitFor(() => app.documents.getSnapshot().docs[0]!.dirty);
    await app.documents.saveTabState();
    await waitFor(() => hostSnapshots.has("README.md"));

    // --- the content process dies: a new page with the same host and daemon ---
    const fresh = createApp(transport);
    fresh.documents.recovery = recovery;
    const freshEditor = new FakeEditor(fresh.documents);
    fresh.documents.attachEditor(freshEditor);
    fresh.documents.ready({ bridgeVersion: 1, monacoVersion: "test" });
    await fresh.client.connect(); // the reloaded page re-subscribes: state is replayed
    await waitFor(() => fresh.workspace.getSnapshot().rootLoaded, 8000, "workspace reopen");
    await waitFor(() => fresh.documents.getSnapshot().docs.length === 1, 3000, "recovered tab");
    const doc = fresh.documents.getSnapshot().docs[0]!;
    expect(doc.path).toBe("README.md");
    expect(doc.dirty).toBe(true);
    expect(freshEditor.text("README.md")).toBe("# unsaved work\n");
    expect(fresh.documents.getSnapshot().banner).toContain("reloaded");
    // Saving is still version-checked against the file on disk.
    fresh.documents.save("README.md");
    await waitFor(() => !fresh.documents.getSnapshot().docs[0]!.dirty, 3000, "saved");
    expect(readFileSync(join(ws, "README.md"), "utf8")).toBe("# unsaved work\n");
  }, 20_000);

  it("a clean document leaves nothing to recover", async () => {
    const hostSnapshots = new Map<string, string>();
    app.documents.recovery = {
      snapshotPut: (path, text) => void hostSnapshots.set(path, text),
      snapshotClear: (path) => void hostSnapshots.delete(path),
      state: async () => ({ recovered: true, snapshots: Object.fromEntries(hostSnapshots) }),
    };
    await app.workspace.openFolder(ws);
    app.documents.open("README.md");
    await waitFor(() => editor.text("README.md") !== undefined);
    editor.type("README.md", "dirty\n");
    await waitFor(() => app.documents.getSnapshot().docs[0]!.dirty);
    await waitFor(() => hostSnapshots.has("README.md"));
    app.documents.save("README.md");
    await waitFor(() => !app.documents.getSnapshot().docs[0]!.dirty, 3000, "clean");
    expect(hostSnapshots.size).toBe(0);
  }, 20_000);

  it("CRLF line endings survive a round trip", async () => {
    writeFileSync(join(ws, "crlf.txt"), "one\r\ntwo\r\n");
    await app.workspace.openFolder(ws);
    app.documents.open("crlf.txt");
    await waitFor(() => editor.text("crlf.txt") !== undefined);
    expect(editor.text("crlf.txt")).toBe("one\r\ntwo\r\n");
    editor.type("crlf.txt", "one\r\ntwo\r\nthree\r\n");
    await waitFor(() => app.documents.getSnapshot().docs[0]!.dirty);
    app.documents.save("crlf.txt");
    await waitFor(() => !app.documents.getSnapshot().docs[0]!.dirty, 3000, "saved");
    // The daemon never converts line endings, and neither does the shell.
    expect(readFileSync(join(ws, "crlf.txt"), "utf8")).toBe("one\r\ntwo\r\nthree\r\n");
  });

  it("a file over 16 MiB is refused with a clear banner", async () => {
    writeFileSync(join(ws, "huge.txt"), "a".repeat(16 * 1024 * 1024 + 1024));
    await app.workspace.openFolder(ws);
    app.documents.open("huge.txt");
    await waitFor(() => app.documents.getSnapshot().banner !== "");
    expect(app.documents.getSnapshot().banner).toContain("too large");
  });

  it("restoring tabs skips files that no longer exist", async () => {
    await app.workspace.openFolder(ws);
    const id = app.workspace.getSnapshot().id;
    await app.client.request("workspace/setState", {
      workspaceId: id,
      values: { "editor.openPaths": ["README.md", "gone.md"], "editor.activePath": "gone.md" },
    });
    // Re-open the workspace: the same root keeps its state, and restore runs again.
    await app.workspace.close();
    await app.workspace.openFolder(ws);
    await waitFor(() => app.documents.getSnapshot().docs.length > 0, 5000, "tabs restored");
    expect(app.documents.getSnapshot().docs.map((d) => d.path)).toEqual(["README.md"]);
    // The saved active tab is gone, so the first restored tab stays active
    // (the same outcome as the Qt model's restore).
    expect(app.documents.getSnapshot().activePath).toBe("README.md");
  });

  it("conflict → Reload shows the disk version as clean", async () => {
    await app.workspace.openFolder(ws);
    app.documents.open("README.md");
    await waitFor(() => editor.text("README.md") !== undefined);
    editor.type("README.md", "# mine\n");
    await waitFor(() => app.documents.getSnapshot().docs[0]!.dirty);
    writeFileSync(join(ws, "README.md"), "# theirs\n");
    await waitFor(() => app.documents.getSnapshot().docs[0]!.changedOnDisk, 3000, "changed on disk");
    app.documents.save("README.md");
    await waitFor(() => app.documents.getSnapshot().conflictName === "README.md", 3000, "conflict");
    app.documents.resolveConflict("reload");
    await waitFor(() => editor.text("README.md") === "# theirs\n", 3000, "disk version shown");
    expect(app.documents.getSnapshot().docs[0]!.dirty).toBe(false);
    expect(readFileSync(join(ws, "README.md"), "utf8")).toBe("# theirs\n");
  });

  it("conflict → Cancel keeps the unsaved text and answers the pending save", async () => {
    await app.workspace.openFolder(ws);
    app.documents.open("README.md");
    await waitFor(() => editor.text("README.md") !== undefined);
    editor.type("README.md", "# mine\n");
    await waitFor(() => app.documents.getSnapshot().docs[0]!.dirty);
    writeFileSync(join(ws, "README.md"), "# theirs\n");
    await waitFor(() => app.documents.getSnapshot().docs[0]!.changedOnDisk, 3000);
    app.documents.save("README.md");
    await waitFor(() => app.documents.getSnapshot().conflictName === "README.md", 3000);
    app.documents.resolveConflict("cancel");
    expect(app.documents.getSnapshot().conflictName).toBe(null);
    expect(app.documents.getSnapshot().docs[0]!.dirty).toBe(true);
    // The page can save again: a second Ctrl+S starts a new attempt.
    app.documents.save("README.md");
    await waitFor(() => app.documents.getSnapshot().conflictName === "README.md", 3000, "second conflict");
  });

  it("unsaved-work prompt: Save writes then proceeds; Cancel keeps the tab", async () => {
    await app.workspace.openFolder(ws);
    app.documents.open("README.md");
    await waitFor(() => editor.text("README.md") !== undefined);
    editor.type("README.md", "# keep me\n");
    await waitFor(() => app.documents.getSnapshot().docs[0]!.dirty);
    app.documents.close("README.md");
    expect(app.documents.getSnapshot().prompt?.kind).toBe("close");
    app.documents.resolvePrompt("cancel");
    expect(app.documents.getSnapshot().docs).toHaveLength(1);
    app.documents.close("README.md");
    expect(app.documents.getSnapshot().prompt?.kind).toBe("close");
    app.documents.resolvePrompt("save");
    await waitFor(() => app.documents.getSnapshot().docs.length === 0, 5000, "closed after save");
    expect(readFileSync(join(ws, "README.md"), "utf8")).toBe("# keep me\n");
  });

  it("a save requested for a path that is not open is ignored", async () => {
    await app.workspace.openFolder(ws);
    const before = readFileSync(join(ws, "README.md"), "utf8");
    editor.store.requestSave("not-open.md"); // the page asks; the shell must ignore it
    await new Promise((r) => setTimeout(r, 200));
    expect(app.documents.getSnapshot().docs).toHaveLength(0);
    expect(readFileSync(join(ws, "README.md"), "utf8")).toBe(before);
  });

  it("a page speaking the wrong bridge version is refused and gets no messages", async () => {
    const listener = createApp(transport);
    const port = new FakeEditor(listener.documents);
    listener.documents.attachEditor(port);
    const sent: string[] = [];
    const spy = new Proxy(port, {
      get: (target, prop) => {
        if (prop === "configure" || prop === "openDocument" || prop === "activateDocument") {
          return () => sent.push(String(prop));
        }
        return Reflect.get(target, prop);
      },
    });
    listener.documents.attachEditor(spy);
    // A page from a future build: the shell must refuse it, not speak to it.
    listener.documents.ready({ bridgeVersion: 2 as unknown as 1, monacoVersion: "test" });
    expect(listener.documents.getSnapshot().editorReady).toBe(false);
    expect(listener.documents.getSnapshot().banner).toContain("bridge version 2");
    expect(sent).toEqual([]);
  });

  it("switching workspaces closes the documents of the old one", async () => {
    const other = mkdtempSync(join(tmpdir(), "axiom-ws2-"));
    writeFileSync(join(other, "b.txt"), "b\n");
    await app.workspace.openFolder(ws);
    app.documents.open("README.md");
    await waitFor(() => app.documents.getSnapshot().docs.length === 1);
    await app.workspace.openFolder(other);
    expect(app.documents.getSnapshot().docs).toHaveLength(0);
    rmSync(other, { recursive: true, force: true });
  });

  it("settings change only when the daemon confirms, and invalid values are rejected", async () => {
    await app.settings.setValue("appearance.theme", "dark");
    await waitFor(() => app.settings.getSnapshot().theme === "dark", 2000, "theme");
    await app.settings.setValue("editor.fontSize", 99);
    await waitFor(() => app.settings.getSnapshot().error !== "");
    expect(app.settings.getSnapshot().editorFontSize).toBe(13);
  });

  it("a crashed daemon fails requests cleanly and restart recovers", async () => {
    await app.workspace.openFolder(ws);
    transport.kill();
    await waitFor(() => app.client.state.state === "failed", 3000, "failed");
    expect(app.workspace.isOpen).toBe(false);
    await app.client.restart();
    await waitFor(() => app.client.ready);
    await app.workspace.openFolder(ws);
    expect(app.workspace.getSnapshot().rootLoaded).toBe(true);
  });
});

describe("output assembly (pure)", () => {
  it("orders by seq, splits lines per stream, strips ANSI, honours CR", () => {
    const b = new OutputBuffer();
    b.append(1, "stdout", "b\nc");
    b.append(0, "stdout", "a\n");
    b.append(2, "stderr", "\x1b[31merr\x1b[0m\n");
    b.append(3, "stdout", "\r\nprogress 10%\rprogress 100%\n");
    b.finish();
    expect(b.take(100).map((r) => `${r.stream}:${r.text}`)).toEqual([
      "stdout:a", "stdout:b", "stderr:err", "stdout:c", "stdout:progress 100%",
    ]);
    expect(stripAnsi("\x1b]0;title\x07x")).toBe("x");
  });
});
