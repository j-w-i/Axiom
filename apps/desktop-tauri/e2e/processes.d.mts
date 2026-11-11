// Types for e2e/processes.mjs (the module is plain ESM so Node can run it
// directly; TypeScript and Vitest consume these declarations).

export interface Proc {
  pid: number;
  ppid: number;
  comm: string;
}

export declare const CONTENT_PROCESS: RegExp;
export declare function parsePsTable(text: string): Proc[];
export declare function parseWindowsTable(text: string): Proc[];
export declare function readProcessTable(): Proc[];
export declare function findDescendantsByName(table: Proc[], appPid: number, name: RegExp): number[];
export declare function findWebContentProcesses(table: Proc[], appPid: number): number[];
