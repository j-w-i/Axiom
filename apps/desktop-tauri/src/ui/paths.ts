// Display forms of paths (no file system access).

/** Home directory shortened to ~ (Linux, macOS, Windows user folders). */
export function shortenHome(path: string): string {
  return path.replace(/^(\/home\/[^/]+|\/Users\/[^/]+|[A-Za-z]:\\Users\\[^\\]+)(?=[/\\]|$)/, "~");
}
