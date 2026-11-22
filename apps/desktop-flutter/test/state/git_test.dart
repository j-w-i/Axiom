// Git in the UI (FL-GIT-001): marks, Source Control groups, the line diff and the gutter marks.

import 'package:axiom_desktop/daemon/protocol.g.dart';
import 'package:axiom_desktop/state/git.dart';
import 'package:flutter_test/flutter_test.dart';

GitFile f(String path, GitFileState index, GitFileState worktree) =>
    GitFile(path: path, index_: index, worktree: worktree);

void main() {
  test('a file is marked by its conflict, then its working tree, then its index', () {
    expect(markOf(f('a', GitFileState.unmodified, GitFileState.modified))!.letter, 'M');
    final staged = markOf(f('b', GitFileState.added, GitFileState.unmodified))!;
    expect((staged.letter, staged.staged), ('A', true));
    expect(markOf(f('c', GitFileState.untracked, GitFileState.untracked))!.letter, 'U');
    expect(markOf(f('d', GitFileState.modified, GitFileState.conflicted))!.letter, 'C');
    expect(markOf(f('e', GitFileState.unmodified, GitFileState.unmodified)), isNull);
  });

  test('Source Control groups', () {
    final g = gitGroups(
      GitStatus(
        repo: true,
        files: [
          f('z.rs', GitFileState.modified, GitFileState.modified),
          f('new.rs', GitFileState.untracked, GitFileState.untracked),
          f('a.rs', GitFileState.unmodified, GitFileState.modified),
        ],
      ),
    );
    expect(g.staged.map((x) => x.path), ['z.rs']);
    expect(g.changes.map((x) => x.path), ['a.rs', 'z.rs']);
    expect(g.untracked.map((x) => x.path), ['new.rs']);
  });

  test('the line diff aligns what is common', () {
    final d = diffLines(['a', 'b', 'c', 'd'], ['a', 'x', 'c', 'd', 'e']);
    expect(
      [for (final l in d) '${l.op.name[0]}${l.left ?? '-'}${l.right ?? '-'}${l.text}'],
      ['s11a', 'r2-b', 'a-2x', 's33c', 's44d', 'a-5e'],
    );
  });

  test('gutter marks: added, modified, deleted', () {
    const head = 'one\ntwo\nthree\nfour\n';
    expect(gutterMarks(head, 'one\ntwo\nthree\nfour\n'), isEmpty);
    expect(gutterMarks(head, 'one\nTWO\nthree\nfour\n'), {1: GutterMark.modified});
    expect(gutterMarks(head, 'one\ntwo\nnew\nthree\nfour\n'), {2: GutterMark.added});
    expect(gutterMarks(head, 'one\nthree\nfour\n'), {1: GutterMark.deleted});
  });
}
