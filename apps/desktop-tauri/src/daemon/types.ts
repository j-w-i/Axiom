// Wire types for the Axiom protocol 0.1 as the shell uses them.
//
// Payload types are GENERATED (ADR-0005): `src/daemon/protocol.gen.ts` is built
// from `schemas/ipc/protocol.schema.json`, which is built from `crates/protocol`.
// Regenerate with `pnpm run gen:types`; `pnpm run check:types` (and CI) fail when
// the committed file is stale.
//
// This module adds only what the generated file cannot express:
// - the host's own event types (they are Tauri-side, not protocol types);
// - `RpcError` as an Error subclass, so a failed call can be thrown (the
//   generated `RpcError` interface describes the wire shape: see `RpcErrorWire`).

import type { ErrorData } from "./protocol.gen";

export type {
  Command,
  DirEntry,
  DidChangeParams,
  EntryKind,
  ErrorData,
  ErrorKind,
  ExitedParams,
  FileChange,
  FileVersion,
  OpenResult,
  OutputParams,
  ReadFileResult,
  RecentWorkspace,
  StartParams,
  Stream,
  TaskId,
  WorkspaceId,
  WorkspacePath,
  WriteFileParams,
} from "./protocol.gen";

/** The wire shape of an error, as generated from `crates/protocol`. */
export type RpcErrorWire = import("./protocol.gen").RpcError;

/** Thrown by `DaemonClient.request`; carries the protocol error *kind*. */
export class RpcError extends Error {
  readonly kind: import("./protocol.gen").ErrorKind;
  readonly details: Record<string, unknown>;
  readonly retryable: boolean;

  constructor(e: RpcErrorWire) {
    super(e.message);
    this.kind = e.data?.kind ?? "unknown";
    this.details = (e.data?.details as Record<string, unknown> | null) ?? {};
    this.retryable = e.data?.retryable ?? false;
  }

  static stopped(): RpcError {
    return new RpcError({
      code: -32603,
      message: "Backend stopped",
      data: { kind: "internal" } as ErrorData,
    });
  }
}

/** Connection state as the host reports it. */
export type DaemonState =
  | { state: "stopped" }
  | { state: "starting" }
  | { state: "ready"; serverVersion: string; protocolVersion: string; methods: string[] }
  | { state: "failed"; message: string };

/** Everything the host sends on its one ordered channel (Tauri → UI). */
export type BridgeEvent =
  | { type: "state"; generation: number; state: DaemonState }
  | { type: "notification"; method: string; params: unknown }
  | { type: "response"; key: number; result?: unknown; error?: RpcErrorWire }
  | { type: "log"; line: string };

/** Items of that channel, including the window's own requests. */
export type HostEvent = { kind: "bridge"; event: BridgeEvent } | { kind: "closeRequested" };
