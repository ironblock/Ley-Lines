# Rows for a working tree

Written by Claude Opus 5.5 on 2026-11-24. A proposal for the maintainer to read before any of it is built. Nothing here exists yet but measurements of how large the rows would be, and `sizes.py`, which made them. Whether rows like these help a reader is not measured here.

## What is asked

A model works on a tree, loses its context, and must go on. What it needs back first is what it changed. So, given a working tree and an earlier state of it, rows say:

- what changed;
- the shape of what changed;
- which tests cover it, and whether they passed.

Four conditions come with it. No model writes the rows or checks them. They are cheap enough to make at every clearing of the context. They need no commit history. And their size is measured, because the reader's context pays for it.

This is "run this to see that" for code being written now, where the rest of Ley Lines is for a place as it stands.

## The proposal in one paragraph

`ley mark` saves the working tree as it is now, as a git tree, through an index of its own. `ley changes` compares a mark with the working tree and prints one row for each file and each declaration that was added, changed or is gone. Each row says what kind of thing changed, and gives a command that shows the thing as it is now. How much of each thing `ley changes` prints under its row, its name, its first line or its lines, is what Kung Fu's patch run will measure, once its tries are sent. The table below says which of its answers picks which. A test row names the test files that name a changed declaration. Whether they passed is recorded from a test run, with the tree it ran on. `ley` never runs a test.

## The mark

A mark is the earlier state.

- **What it is.** `git add -A` into an index of `ley`'s own, then `git write-tree`. The result is a tree: every file git would track, as it is in the working tree, and nothing it ignores.
- **What it needs.** A git repository. It needs no commit. It works in a repository whose history is empty, cut short or squashed. That was tried: a copy of a 5,714-file tree in a new repository with no commits.
- **What it leaves alone.** Your index, your HEAD, your branches and your files. It adds objects to the repository's object store, as `git add` would. `ley since` already works this way, through an index of its own.
- **How it is kept.** By a ref, `refs/ley/marks/<n>`, so that git's garbage collection keeps its objects. Question A below asks whether that is the place.
- **What it costs.** The first mark hashes every file. Later ones reuse the index and hash only what changed.

| Tree | Files | First mark | A later mark, one file changed | What changed between the two marks |
| --- | ---: | ---: | ---: | ---: |
| Kung Fu, a clone | 57,337 | 1.0 s | 0.26 s | |
| Babylon Lite, in a repository with no commits | 5,714 | 4.3 s | 0.03 s | listed in 0.004 s |

The Kung Fu clone shares its objects with another clone, so its first mark wrote few objects. The Babylon Lite copy had none, and its first mark wrote 116 MB.

## What `ley changes` prints

It marks the working tree first, so that both sides are trees, and then compares them. For each path added, changed or gone, it prints a file row. For each declaration in a changed file, it prints a declaration row, if the declaration was added, changed, moved, or is gone. A declaration is what `ast-grep outline` reports: a top-level item, or a member of one. A declaration is changed when its text is. A member's change does not also mark the item that holds it.

A row in the example place, after the interface `Stage` gained a field:

```text
Since mark 1, 1 file changed.

services/quillon/stage.ts changed: 1 line added.
  The interface Stage changed. If you run
      ast-grep outline --match '^Stage$' --view expanded services/quillon/stage.ts
  it will show you its first line and the first line of each of its members: 1 result.
```

The sentence is `ley`'s, as with `ley show`. The command is one `ley check` would run today. A declaration that is gone gets the same command, and its row ends "it will show you the interface Stage: 0 results", as a row with `count = 0` does today.

**How much is printed under a row.** At the level of names, nothing: the reader runs the command if it wants to. At the level of first lines, `ley` runs the command and prints what it shows. At the level of lines, `ley` prints the declaration's lines from the file, with their numbers. The numbers and the lines are read at the moment of printing. Nothing that was printed is stored. `sizes.py` calls these three levels `names`, `shape` and `region`.

**Order and size.** Files in the order of their paths, and declarations in the order of the file. A budget in tokens, `--budget`, caps what is printed under the rows. Past it, rows are printed with names only, and past that `ley` says how many rows it did not print. It never drops a row in silence.

## How much a row carries: what the patch run decides

Kung Fu's patch run asks how much of a file a reader must be given to patch it right. Its note is [`bench/runs/2026-11-20-patch/README.md`](https://github.com/ironblock/Kung-Fu/blob/claude/gracious-gates-1xlptv/bench/runs/2026-11-20-patch/README.md) in Kung Fu, and its five questions are there. Its arms hand the reader the file's path (`name`), its outline (`outline`), its outline and the lines of each declaration the ask names (`region`), or the whole file (`whole`). Rows should carry no more than its answer says is needed, and no less. So the choice is written down here, before its tries come back:

| If the patch run finds | Then a row for a changed file carries |
| --- | --- |
| An outline is as good as the whole file for adding code (patch question 2) | the file's outline, so that code can be added beside what is there |
| The outline is not as good as the whole file for adding code | the outline, and a pointer to the whole file, which the reader opens if it has tools |
| `region` is as good as the whole file for changing a declaration (patch question 3) | for each changed declaration, its lines |
| `region` is not as good as the whole file for changing a declaration | the whole file |
| `region` falls short when a change leans on another body (patch question 4) | also, for each changed declaration, the names it calls, each with the command that shows it |
| `region` does better than the outline alone (patch question 5) | lines, and not first lines alone, for each changed declaration |
| The rule cannot tell | the larger of the two, until a run can |

The patch run's outline is the whole file's outline. A row at the level of first lines holds only the changed declarations' first lines, which is less. That is why the table speaks of the file's outline.

A hint, not a result: the patch run's ceiling, a stronger model with one try a task, passed every task with the whole file. With the outline it passed every task that adds code. With `region` it passed every task that changes one declaration, and 4 of 16 that lean on another body.

## Tests

**Which tests cover a change.** For each added or changed declaration, a row runs `rg -l -w -F <name>` over the places that hold tests. It shows the test files that name it. This is a command `ley check` runs today. Its limits: a name shorter than four characters is left out, as in rule 1, because it is in too many files; and a test that reaches the code under another name is missed. The places that hold tests would be named once, in `ley.toml` (question B).

**Whether they passed.** `ley` does not run tests. A test runs the repository's code. `ley` runs only commands that fit a shape, and that is what makes running `ley check` no more trusting than running `rg` yourself. So whoever runs the tests records the run:

```console
$ ley tested --exit 0 -- cargo test
```

`ley` keeps the command, its exit status, the time, and a mark of the tree at that moment. Then a test row says, for each test file that names a changed declaration, whether a recorded run passed, at which mark, and whether that file or what it names has changed since. A recorded run is a source, with a date, as rule 3 asks. `LATER.md` parks "commands that pass or fail"; this brings it back in a narrow form, since `ley` records the result and does not run the command. Kung Fu has tried the other form. In its run `bench/runs/2026-11-19-notes-at-pins`, 67 notes on a library each carry a test that passes or fails, and `bench/notes.py check` sorts them into `ley check`'s own states, holds, differs and could not run, at seven commits of the library.

## The rules, one by one

1. **show, do not state.** Kept. Each row says what kind of thing changed, and gives the command that shows it.
2. **no stored output.** Kept. A mark is the files themselves, not what a command printed. What `ley changes` prints is read when it prints. A recorded test run keeps an exit status, not the test's output.
3. **a command or a source.** Kept. That a thing changed is found by `ley` from two trees, and the row names the mark and its time. A test result names the recorded run.
4. **no grades.** Kept. `ley` does not judge which change matters. It orders by path. The budget drops rows by place, not by worth, and says how many it dropped.
5. **no instructions.** Kept. `ley` writes every sentence.
6. **one field per line.** It applies if the rows are written as TOML (question F).
7. **the same for every reader.** Not kept, and it cannot be: a working tree belongs to one clone. So these rows never go into `ley.toml`. They are printed for the session that asked, and the marks live in the clone, not in history.
8. **exact name, wide search.** Kept for test rows: the declaration's exact name, searched as a word over every place that holds tests.
9. **fail loud.** Kept. A mark that is missing, a file that is not text, a file the outline reports nothing for, and every row the budget leaves out are each said.
10. **earliest known.** Not used.

## What it costs a reader

Four changes in real trees, measured by `sizes.py` from two exported commits each. Tokens are bytes divided by 3.5, an estimate.

| Change | Files changed | Declarations | Names | + first lines | + lines | A plain diff | Whole files | Time |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Ley Lines, pull request 2 | 9 | 13 | 106 | 397 | 5,938 | 5,235 | 52,727 | 1.9 s |
| Kung Fu, one commit of bench tools | 7 | 38 | 91 | 803 | 12,025 | 11,216 | 47,916 | 1.8 s |
| Babylon Lite, one commit | 92 | 491 | 1,946 | 18,795 | 128,554 | 181,261 | 502,007 | 4.9 s |
| Babylon Lite, 17 commits | 624 | 2,020 | 12,865 | 82,399 | 470,557 | 943,096 | 2,194,266 | 34 s |

A plain diff is the names and the changed lines with three lines round each, as `diff -u` gives them. For a change the size of one commit it is a little smaller than the lines of each changed declaration; for the large changes it is larger. A row adds what a diff does not: the name of each declaration, and a command that shows it as it is now.

The test rows, the test files that name each changed declaration, came to 201 tokens for the Ley Lines change and 24,578 for the one Babylon Lite commit.

- **A change the size of one commit of these two projects costs a reader 100 to 800 tokens at names and first lines.** Lines cost 6,000 to 12,000. The whole files cost 48,000 to 53,000.
- **A large change in a large tree does not fit at any level** without a budget: 13,000 tokens for names alone, and 82,000 with first lines.
- **Most of the time goes on running `ast-grep outline` over the changed files,** twice each. `sizes.py` also reads every file of both trees to find what changed. With marks, finding what changed is a comparison of two git trees, which took 0.004 s on the 5,714-file tree. So I expect the time of `ley changes` to grow with the change and hardly with the tree. That is a guess until it is built.
- The counts leave out what `sizes.py` cannot see: hidden files, and files that are not text. Git counts 8, 94 and 691 files changed where `sizes.py` counts 7, 92 and 624.

## What it does not do

- It does not choose what matters. No model, and no ranking.
- It does not see a file that git ignores, or the files of a submodule.
- It does not follow a change across repositories.
- It does not deliver itself. Something must call `ley changes` at each clearing of the context. That is the harness's part, and `LATER.md`'s "delivering rows into an agent's session".

## Questions for the maintainer

A. **Where a mark lives.** A ref under `refs/ley/marks/` keeps its objects from garbage collection, and `git push --mirror` would send it. A file under `.git/ley/` sends nothing, but git may prune its objects after two weeks.
B. **Tests.** Recorded by `ley tested`, as above, or run by `ley` from a command the ledger names? And are the places that hold tests named in `ley.toml`?
C. **Which earlier state.** The first mark of a session, so that the rows hold everything the session changed, or the last, so that they hold only what changed since the last clearing? I would print the first by default, and the last with `--since-last`.
D. **The budget.** Its default, and what goes first when it binds: changed before added, or the smallest first.
E. **A tree that is not a git repository.** Refuse, or keep a store of `ley`'s own?
F. **The form.** Sentences, as `ley show` prints, or rows as TOML, or both.

## How to see the numbers

The numbers above are what these commands printed on 2026-11-24. Run them in an empty directory outside the repository, with `LEY` set to this repository's path. Each change is two commits, exported and compared:

```sh
mkdir before after
git -C <a clone> archive <before> | tar -x -C before
git -C <a clone> archive <after> | tar -x -C after
python3 "$LEY/design/sizes.py" before after --show none
```

| Change | Repository | Before | After |
| --- | --- | --- | --- |
| Ley Lines, pull request 2 | this one | `38b09a1` | `70eb035` |
| Kung Fu, one commit of bench tools | `ironblock/Kung-Fu` | `2cd1e8a` | `f58fcdb` |
| Babylon Lite, one commit | `BabylonJS/babylon-lite` | `bc317f91~1` | `bc317f91` |
| Babylon Lite, 17 commits | `BabylonJS/babylon-lite` | `315c7e99` | `124eafa5` |

For the test rows, add `--tests tests` for Ley Lines, and `--tests tests packages` for Babylon Lite.

A mark, timed, in a copy of a tree in a repository with no commits:

```sh
cp -r after nohist && cd nohist && git init -q
time GIT_INDEX_FILE=../mark.index git add -A
GIT_INDEX_FILE=../mark.index git write-tree
```
