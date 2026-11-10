# For agents working in this repository

## Read in first

This repository keeps a ledger of its own words, rules and places, in `ley.toml`.

```sh
cargo build
target/debug/ley show            # every name
target/debug/ley show shape      # what one name means, and where it lives
target/debug/ley rules           # the rules a ledger follows
```

`README.md` says what the project is for. `LATER.md` says what is parked, and why.

## Before you commit

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
target/debug/ley check --strict
```

The tests start `rg`, `ast-grep` and `git`. A test that needs a missing program skips itself and says so. Set `LEY_TEST_REQUIRE_TOOLS=1` to make that a failure.

## When you change something

- **A rule.** Add its name to `RULES`. Add a file to `fixtures/invalid` that breaks only that rule. Its first line is `# breaks: <rule>`. Its second line says why, in plain words. If a reader should know the rule, add it to `ley.toml` and to the list in `README.md`. A test compares the two lists. `ley rules` prints the rows of `ley.toml` whose `is` starts with "the rule that".
- **A flag or a shape.** Add it to `src/shapes.toml` and add a case to `fixtures/commands.toml`. Leave out any flag that writes files, runs another program, or changes what one result means.
- **What `ley` prints.** `README.md` quotes real output, and a test compares it. Change the README in the same commit.
- **Where something lives.** If you move or rename a thing that a row in `ley.toml` points at, `ley check` will say so. Change the row.

## How to write here

- Write for a reader who was never in the room. Use short sentences, one idea in each, and plain words. Use one term for one thing.
- Examples use invented names. Nothing from a real organization goes into this repository: no codenames, no stage names, no error strings.
- A row says only what its command can show, or it names who said it. If you are the one saying it, name yourself in `said`.
- Do not store what a command printed, anywhere. Store the command.
