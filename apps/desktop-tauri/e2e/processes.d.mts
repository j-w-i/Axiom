// Types for e2e/processes.mjs (the module is plain ESM so Node can run it
// directly; TypeScript and Vitest consume these declarations).

export interface Proc {
  pid: number;
  ppid: number;
  /** Executable file name (not the truncated `ps comm`). */
  comm: string;
  /** Full command line; empty when unknown. */
  args: string;
}

export declare function executableName(args: string): string;
export declare function parsePsTable(text: string): Proc[];
export declare function parsePsTableWithArgs(text: string): Proc[];
export declare function parseWindowsTable(text: string): Proc[];
export declare function readProcessTable(): Proc[];
export declare function findDescendants(table: Proc[], appPid: number, match: (p: Proc) => boolean): number[];
export declare function findDescendantsByName(table: Proc[], appPid: number, name: RegExp): number[];
export declare function isWebContentProcess(p: Proc): boolean;
export declare function findWebContentProcesses(table: Proc[], appPid: number): number[];
export declare function killNow(pid: number): void;
