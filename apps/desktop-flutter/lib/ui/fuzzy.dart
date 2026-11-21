// Fuzzy matching for the palette and Go to File (ported from the Tauri shell's ui/fuzzy.ts, the
// same scoring): the query's characters in order (a subsequence), scored for how a person types
// paths. Bonuses for a character at the start of a path segment or a word, at a camelCase hump,
// for runs of consecutive characters, and for matches in the file name; penalties for gaps and
// long paths. The best alignment is found by dynamic programming after a cheap subsequence
// check. Pure.

import 'dart:typed_data';

final class FuzzyMatch {
  const FuzzyMatch(this.score, this.ranges);
  final double score;

  /// Matched character ranges `[start, end)` of the text, merged.
  final List<(int, int)> ranges;
}

const _separators = {'/', r'\', '_', '-', '.', ' '};
const _bonusSegment = 10.0;
const _bonusWord = 7.0;
const _bonusCamel = 6.0;
const _bonusConsecutive = 5.0;
const _bonusName = 3.0;
const _gap = 0.6;
const _gapLeading = 0.05;
const _neg = -1e9;

final _lowerOrDigit = RegExp(r'[a-z0-9]');
final _upper = RegExp(r'[A-Z]');

bool _isSubsequence(String q, String t) {
  var i = 0;
  for (var j = 0; j < t.length && i < q.length; j++) {
    if (t[j] == q[i]) i++;
  }
  return i == q.length;
}

/// The match of [query] in [text], or null. Case-insensitive; an upper-case query character
/// prefers the same case (camelCase queries).
FuzzyMatch? fuzzyMatch(String query, String text) {
  if (query.isEmpty) return const FuzzyMatch(0, []);
  final q = query.toLowerCase();
  final t = text.toLowerCase();
  if (!_isSubsequence(q, t)) return null;
  final n = q.length;
  final m = t.length;
  final nameStart =
      (text.lastIndexOf('/') > text.lastIndexOf(r'\')
          ? text.lastIndexOf('/')
          : text.lastIndexOf(r'\')) +
      1;
  double charBonus(int j) {
    final prev = j > 0 ? text[j - 1] : '/';
    var b = 0.0;
    if (prev == '/' || prev == r'\') {
      b = _bonusSegment;
    } else if (_separators.contains(prev)) {
      b = _bonusWord;
    } else if (_lowerOrDigit.hasMatch(prev) && _upper.hasMatch(text[j])) {
      b = _bonusCamel;
    }
    if (j >= nameStart) b += _bonusName;
    return b;
  }

  final score = <Float64List>[];
  final from = <Int32List>[];
  for (var i = 0; i < n; i++) {
    final row = Float64List(m)..fillRange(0, m, _neg);
    final back = Int32List(m)..fillRange(0, m, -1);
    final prev = i > 0 ? score[i - 1] : null;
    var run = _neg;
    var runAt = -1;
    for (var j = 0; j < m; j++) {
      if (i > 0 && j >= 2) {
        final cand = prev![j - 2] - _gap;
        if (run - _gap > cand) {
          run -= _gap;
        } else {
          run = cand;
          runAt = j - 2;
        }
      }
      if (t[j] != q[i]) continue;
      final caseBonus = query[i] != q[i] && text[j] == query[i] ? 1.0 : 0.0;
      final own = 1 + charBonus(j) + caseBonus;
      if (i == 0) {
        row[j] = own - _gapLeading * j;
        continue;
      }
      final adjacent = j > 0 && prev![j - 1] > _neg / 2 ? prev[j - 1] + _bonusConsecutive : _neg;
      if (adjacent >= run && adjacent > _neg / 2) {
        row[j] = own + adjacent;
        back[j] = j - 1;
      } else if (run > _neg / 2) {
        row[j] = own + run;
        back[j] = runAt;
      }
    }
    score.add(row);
    from.add(back);
  }
  final last = score[n - 1];
  var bestJ = -1;
  for (var j = 0; j < m; j++) {
    if (bestJ < 0 || last[j] > last[bestJ]) bestJ = j;
  }
  if (bestJ < 0 || last[bestJ] <= _neg / 2) return null;
  final pos = <int>[];
  for (var i = n - 1, j = bestJ; i >= 0 && j >= 0; i--) {
    pos.add(j);
    j = from[i][j];
  }
  final ranges = <(int, int)>[];
  for (final p in pos.reversed) {
    if (ranges.isNotEmpty && ranges.last.$2 == p) {
      ranges[ranges.length - 1] = (ranges.last.$1, p + 1);
    } else {
      ranges.add((p, p + 1));
    }
  }
  return FuzzyMatch(last[bestJ] - text.length * 0.02, ranges);
}

/// [items] ranked by their match against [query] (ties: shorter, then alphabetical), at most
/// [limit].
List<({T item, FuzzyMatch match})> rankFuzzy<T>(
  Iterable<T> items,
  String query,
  String Function(T) text, {
  int limit = 200,
}) {
  final out = <({T item, FuzzyMatch match})>[];
  for (final item in items) {
    final m = fuzzyMatch(query, text(item));
    if (m != null) out.add((item: item, match: m));
  }
  out.sort((a, b) {
    final s = b.match.score.compareTo(a.match.score);
    if (s != 0) return s;
    final l = text(a.item).length.compareTo(text(b.item).length);
    return l != 0 ? l : text(a.item).compareTo(text(b.item));
  });
  return out.length > limit ? out.sublist(0, limit) : out;
}
