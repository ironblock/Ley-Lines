# Ley Lines

Ley Lines is a ledger of facts about one place: a repository, an organization, a pipeline.

It is written for a reader who was never in the room. That reader may be a new colleague. More often it is an agent that starts every session knowing nothing about the place.

Each fact is a row. A row says what a thing is and who said so, or it gives a command:

> If you run X, it will show you Y.

The row stores the command. It never stores what the command printed. So what a reader sees is the place as it is now, not as it was when the row was written. When a command stops showing what its row says, `ley check` reports the row.

## An example

[`examples/pipeline`](examples/pipeline) is a small invented place: a nightly billing run with three stages, a glossary, and two package manifests. Three rows of [its ledger](examples/pipeline/ley.toml):

```toml
[[row]]
name = "Quillon"
is = "the codename for the nightly billing run"
said = [{ by = "maintainer", on = 2026-11-10 }]

[[row]]
name = "Quillon"
run = "rg -n -F Quillon docs/glossary.md"
shows = "the glossary line that defines Quillon"

[[row]]
name = "stage"
run = "rg -n -F '[stage.' pipeline.toml"
shows = "the name of each stage of the billing run, in the order they run"
count = 3
```

A reader asks what the ledger holds, then asks about one name:

```console
$ cd examples/pipeline
$ ley show
Quillon          what it is, how to see it
Brimstone        what it is
stage            how to see it
platform         how to see it
QUILLON-409      how to see it
package manager  how to see it

$ ley show quillon
Quillon is the codename for the nightly billing run.
  Said by maintainer on 2026-11-10.

If you run
    rg -n -F Quillon docs/glossary.md
it will show you the glossary line that defines Quillon: 1 result.
```

Nothing was run. The reader now knows what Quillon is, who said so, and where to look. Whether to look is the reader's decision.

Whoever keeps the place runs the commands:

```console
$ cd examples/pipeline
$ ley check
8 rows: 6 hold, 2 have no command.
```

Then someone removes the `publish` stage from `pipeline.toml`, and tells nobody:

```text
$ ley check
ley.toml:20: differs: "stage": the row says 3 results, the command showed 2
    rg -n -F '[stage.' pipeline.toml
ley.toml:31: differs: "platform": the row says 3 results, the command showed 2
    rg -n '^platform' pipeline.toml
8 rows: 4 hold, 2 differ, 2 have no command.
```

A paragraph of documentation would still say three stages. A reader who runs the row's command sees two.

Moving a file changes nothing. The row that finds `interface Stage` searches every file under `services`, so it finds the interface wherever it goes.

`ley check` compares one number: how many results a command shows. So it catches a thing that was removed, renamed or duplicated. It does not catch a thing that changed inside. If a field of `Stage` changes its type, the row still holds, and its command shows the new type. A row points at a thing. It does not describe it.

A definition is different. No command stands behind "Quillon is the codename for the nightly billing run", so nothing can check it. That is why it carries its sources and their dates.

## The rules

A ledger follows ten rules. `ley` enforces the ones a program can enforce. Each of those has a fixture in [`fixtures`](fixtures): a small file that breaks the rule and says why.

1. **show, do not state.** A row gives a command that shows the thing, and says only what kind of thing the command shows. "The package manager this directory declares" stays true when the version changes. A sentence that names the version does not, and the row would go on holding. `ley check` reports a row whose `shows` repeats a value or a phrase that its command printed. It cannot find a paraphrase, a single digit, or a single plain word.
2. **no stored output.** Nothing a command printed is kept in a row: no value, no file position, no line number, no matched text. Those are the first things to go out of date.
3. **a command or a source.** Every sentence in a ledger is either what a command shows, or what a named source said on a date. Some facts have no command: what a codename means, why a choice was made. Those rows say who said so, and when.
4. **no grades.** A ledger records who said a thing and when. It does not rank its sources or assign a level of trust. The reader can count the sources and weigh them. An agent is a source like any other, and is named as one.
5. **no instructions.** A row states a fact and leaves the reader to decide whether to run anything. The author writes the parts of a row and `ley` writes the sentence, so a row cannot become "check this first".
6. **one field per line.** Each field of a row is on a line of its own. So a change to one field is one changed line, a review comment can sit on one field, and two people can change two fields of a row without a conflict.
7. **the same for every reader.** A command has no shell syntax and only relative paths, so it shows every reader the same thing. `rg Stage` breaks this rule: with no path, `rg` searches standard input when that is a pipe.
8. **exact name, wide search.** A command names the thing exactly and searches as much of the place as it can. Then it still finds the thing after a file moves.
9. **fail loud.** A row that could not be run is reported as such. It is never counted as holding, and never passed over.
10. **earliest known.** A row may carry the earliest date, and commit, at which it is known to have held. `ley since` finds them by walking back through history. With more history, the date can move earlier. It moves later in one case only: the history has the recorded commit, and shows a commit after it at which the row did not hold. A shallow clone or a squashed history does not move it. Neither does a command that could not run at an older commit.

## The file

A ledger is a TOML file named `ley.toml`. Its commands run in the directory that holds it.

```toml
[[row]]
name = "…"
is = "…"
said = [{ by = "…", on = 2026-11-10 }]

[[row]]
name = "…"
run = "…"
shows = "…"
```

| key | what it holds |
|---|---|
| `name` | The term a reader would look up. Several rows may share a name. |
| `is` | What the thing is. `ley` prints "*name* is *is*." It needs `said`. |
| `said` | Who said so, and when. |
| `run` | A command. |
| `shows` | What kind of thing the command shows, not the thing itself. `ley` prints "If you run *run*, it will show you *shows*." |
| `count` | How many results the command shows. It is 1 when left out. It may be 0: the command then shows that something is not there. |
| `since`, `sha` | The earliest known date and commit at which the row held. `ley since` writes them. The date is the commit's day in UTC. |

A row has `is`, or `run`, or both. What kind of row it is follows from the keys it carries.

A row is a `[[row]]` block, and every field of the row sits inside it, on a line of its own. TOML has other ways to write the same rows: all the fields on one line, or the sources in a `[[row.said]]` block of their own. `ley` refuses both, so a row is written one way only. The `=` signs may be lined up or not: `ley` does not mind, and `ley since` writes its two lines to match the block.

Text is one line. It holds no control character, no space but the plain one, and no character that a reader cannot see. Some of those a person does not see and a model reads as letters. So a row cannot print as two rows, a command cannot read as another command, and a sentence cannot carry a second message. A mark that a script needs, such as the joiner in Persian, is allowed where that script puts it. Letters that look alike across scripts are not found.

A result is one line for `rg`, one match for `ast-grep run` and `ast-grep scan`, and one symbol for `ast-grep outline`.

## The commands

| command | what it does |
|---|---|
| `ley show` | Lists every name in the ledger. |
| `ley show <name>` | Prints the rows with that name. A name of several words needs no quotes. Runs nothing. |
| `ley check` | Runs each row's command and compares what it shows with what the row says. |
| `ley since` | Walks back through git history and records the earliest commit at which each row held. |
| `ley shapes` | Lists the commands `ley check` will run. |
| `ley rules` | Prints the rules a ledger follows. |

`ley --help` describes the file. So a reader who has only the program can still learn the format.

`ley check` leaves each row with a command in one of five states:

| state | what it means |
|---|---|
| holds | The command showed as many results as the row says. |
| differs | The command showed a different number. |
| restates | The command showed the right number, and the row's `shows` repeats part of what it printed. |
| could not run | The program is missing, or too old, or said it failed. Or a path the row names is not there. |
| not checked | The command fits no shape, so `ley` did not run it. |

It exits with 0 when every command that ran holds. With `--strict`, a row that was not checked also counts against it. A row with no command has nothing to run, and never fails.

`ley since` reads the repository and writes one file, the ledger. It puts each commit's files into a temporary directory, through an index of its own. It registers no working tree and runs none of the repository's hooks. Your files, your index and your HEAD stay as they were. If it is stopped half way, what remains is that temporary directory.

Git checks those files out the way it would for you. So it runs the filters that your git settings name for checking files out. It also fetches what a commit needs and the clone does not have: large files kept by git-lfs, and files that a partial clone left out. Those are the only things a walk adds to the repository.

When a command could not run at an older commit, the walk stops there for that row and says so. That is not a commit at which the row did not hold, so it moves no date.

## What `ley check` will run

`ley check` runs commands that it reads from a file. So it runs only the commands that fit a *shape*: a program, and the flags a row may pass to it. The shapes are data, in [`src/shapes.toml`](src/shapes.toml).

```console
$ ley shapes
rg
    search text; each line printed is one result
    needs a path
    `ley` adds: --no-config --no-ignore-global --no-ignore-exclude
    flags: -n --line-number -N --no-line-number -i --ignore-case -s
        --case-sensitive -S --smart-case -w --word-regexp -x --line-regexp -F
        --fixed-strings -o --only-matching -l --files-with-matches --files
    options, each with a value: -e --regexp -g --glob -t --type -T --type-not

ast-grep run
    match a code pattern; each match is one result
    `ley` adds: --no-ignore global --no-ignore exclude --json=stream
    needs one of: -p --pattern -k --kind
    options, each with a value: -p --pattern -k --kind -l --lang --selector
        --strictness --globs

ast-grep scan
    match a rule; each match is one result
    `ley` adds: --no-ignore global --no-ignore exclude --json=stream
    needs one of: --inline-rules -r --rule
    options, each with a value: --inline-rules -r --rule --globs

ast-grep outline
    list the symbols in files; each top-level symbol is one result
    `ley` adds: --no-ignore global --no-ignore exclude --json=stream
    options, each with a value: -l --lang --items --type --match --view --globs

Paths are relative and stay inside the ledger's directory.
A command that fits no shape is shown to readers but never run.
```

A list of allowed programs would not be enough. `rg --pre` makes `rg` run another program on every file. So a shape lists flags, and any flag it does not list is refused. `--hidden` and `--no-ignore` are refused for a different reason: they search `.git` and build output, which differ from one clone to the next.

A command that fits no shape is still a valid row. Readers see it, and `ley check` reports it as "not checked". [`fixtures/commands.toml`](fixtures/commands.toml) lists commands that run and commands that are refused, each with its reason.

`ley` starts each program directly, with no shell, in the ledger's directory, with nothing on standard input. It adds flags to each program so that a count depends on the repository and not on the machine: no personal ripgrep configuration file, and no personal list of ignored files. The row itself never carries those flags. One gap is known: a personal `.ignore` file in a directory above the repository still applies. A test compares how `ley` and a real `sh` split each fixture command into words. They must agree.

Before it starts a program, `ley` looks at each path the row names. A path that is not there is reported. So is a path that a link leads out of the ledger's directory: what is there is not the same for every reader. A path inside `.git` is refused for the same reason.

Running `ley check` in a repository trusts that repository as much as running `rg` or `ast-grep` there yourself does. It does not trust it more.

## What belongs in a ledger

A fact belongs in a ledger if it is about one particular place and would be false, or mean nothing, anywhere else: what a codename refers to, where an interface is defined, which stages a pipeline has, which line raises an error.

How to use a tool is a different kind of fact. It is true wherever the tool is installed. That is the subject of a sibling project, Kung Fu.

## Status

This is a first version. It has the file format, the rules, and five commands.

It does not yet deliver rows into an agent's session, choose which rows a reader needs, or follow one fact across several repositories. [`LATER.md`](LATER.md) lists what is parked, and what would bring each item back.

Nothing here has been measured yet. The ledgers so far are this repository's [own](ley.toml) and the example. Whether a ledger makes a reader faster, or right more often, is an open question.

## Building

```sh
cargo install --path .
```

This needs Rust 1.85 or later, and installs one program, `ley`.

`ley check` and `ley since` start the programs that the rows name. So they need [ripgrep](https://github.com/BurntSushi/ripgrep) and [ast-grep](https://ast-grep.github.io). A row that uses `ast-grep outline` needs ast-grep 0.44 or later; with an older one, the row is reported as "could not run". The tests here ran with ripgrep 14.1 and ast-grep 0.51.

## License

Apache-2.0. See [LICENSE](LICENSE).
