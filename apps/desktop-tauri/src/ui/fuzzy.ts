// Fuzzy matching for Go to File (M4-UX-001): the query's characters in order (a subsequence),
// scored for how a person types paths. Bonuses for a character at the start of a path segment
// or a word (after / _ - . or a space), at a camelCase hump, for runs of consecutive characters,
// and for matches in the file name. Penalties for gaps and for long paths. The best alignment
// is found by dynamic programming (query × path), after a cheap subsequence check. Pure.

export interface FuzzyMatch {
  score: number;
  /** Matched character ranges [start, end) of the text (merged). */
  ranges: [number, number][];
}

const SEPARATORS = new Set(["/", "\\", "_", "-", ".", " "]);
const BONUS_SEGMENT = 10;
const BONUS_WORD = 7;
const BONUS_CAMEL = 6;
const BONUS_CONSECUTIVE = 5;
const BONUS_NAME = 3;
const GAP = 0.6;
const GAP_LEADING = 0.05;

function isSubsequence(q: string, t: string): boolean {
  let i = 0;
  for (let j = 0; j < t.length && i < q.length; j++) if (t[j] === q[i]) i++;
  return i === q.length;
}

/** The match of `query` in `text`, or null. Case-insensitive; an upper-case query character
 * prefers the same case (camelCase queries). */
export function fuzzyMatch(query: string, text: string): FuzzyMatch | null {
  if (!query) return { score: 0, ranges: [] };
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  if (!isSubsequence(q, t)) return null;
  const n = q.length;
  const m = t.length;
  const nameStart = Math.max(text.lastIndexOf("/"), text.lastIndexOf("\\")) + 1;
  const charBonus = (j: number): number => {
    const prev = j > 0 ? text[j - 1]! : "/";
    let b = 0;
    if (prev === "/" || prev === "\\") b = BONUS_SEGMENT;
    else if (SEPARATORS.has(prev)) b = BONUS_WORD;
    else if (/[a-z0-9]/.test(prev) && /[A-Z]/.test(text[j]!)) b = BONUS_CAMEL;
    if (j >= nameStart) b += BONUS_NAME;
    return b;
  };
  const NEG = -1e9;
  // score[i][j]: best score with q[i] matched at t[j]; from[i][j]: where q[i-1] was matched.
  const score: Float64Array[] = [];
  const from: Int32Array[] = [];
  for (let i = 0; i < n; i++) {
    const row = new Float64Array(m).fill(NEG);
    const back = new Int32Array(m).fill(-1);
    const prev = score[i - 1];
    // run: the best of prev[k] - GAP·(j-1-k) over k < j-1, kept as j moves.
    let run = NEG;
    let runAt = -1;
    for (let j = 0; j < m; j++) {
      if (i > 0 && j >= 2) {
        const cand = prev![j - 2]! - GAP;
        if (run - GAP > cand) run -= GAP;
        else {
          run = cand;
          runAt = j - 2;
        }
      }
      if (t[j] !== q[i]) continue;
      const caseBonus = query[i] !== q[i] && text[j] === query[i] ? 1 : 0;
      const own = 1 + charBonus(j) + caseBonus;
      if (i === 0) {
        row[j] = own - GAP_LEADING * j;
        continue;
      }
      const adjacent = j > 0 && prev![j - 1]! > NEG / 2 ? prev![j - 1]! + BONUS_CONSECUTIVE : NEG;
      if (adjacent >= run && adjacent > NEG / 2) {
        row[j] = own + adjacent;
        back[j] = j - 1;
      } else if (run > NEG / 2) {
        row[j] = own + run;
        back[j] = runAt;
      }
    }
    score.push(row);
    from.push(back);
  }
  const last = score[n - 1]!;
  let bestJ = -1;
  for (let j = 0; j < m; j++) if (bestJ < 0 || last[j]! > last[bestJ]!) bestJ = j;
  if (bestJ < 0 || last[bestJ]! <= NEG / 2) return null;
  // Positions, by walking back.
  const pos: number[] = [];
  for (let i = n - 1, j = bestJ; i >= 0 && j >= 0; i--) {
    pos.push(j);
    j = from[i]![j]!;
  }
  pos.reverse();
  const ranges: [number, number][] = [];
  for (const p of pos) {
    const r = ranges[ranges.length - 1];
    if (r && r[1] === p) r[1] = p + 1;
    else ranges.push([p, p + 1]);
  }
  return { score: last[bestJ]! - text.length * 0.02, ranges };
}

/** `items` ranked by their match against `query` (ties: shorter, then alphabetical), at most
 * `limit`. */
export function rankFuzzy<T>(items: readonly T[], query: string, text: (t: T) => string, limit = 200): { item: T; match: FuzzyMatch }[] {
  const out: { item: T; match: FuzzyMatch }[] = [];
  for (const item of items) {
    const match = fuzzyMatch(query, text(item));
    if (match) out.push({ item, match });
  }
  out.sort((a, b) => b.match.score - a.match.score || text(a.item).length - text(b.item).length || text(a.item).localeCompare(text(b.item)));
  return out.slice(0, limit);
}
