# Later

Things that are deliberately not here yet. Each has a condition that brings it back.

- **Delivering rows into an agent's session.** A row could arrive when its name appears in what the agent is reading. *Returns when* a ledger has been in daily use, and recorded sessions show what a trigger on names would have delivered, and when.
- **Finding rows by meaning.** `ley show` matches names only. *Returns when* readers are seen asking for a thing by words that are in no name.
- **A command that runs a row.** Today a reader copies the command. *Returns when* that copying is seen to cost more than it teaches.
- **A shape for a tool describing itself.** `<tool> --help`, `<tool> --version`, and a pipe into `rg`. It needs a way to approve which programs a ledger may start. *Returns with* the first ledger about a tool, which is Kung Fu's subject.
- **Shapes for structured files.** A value in JSON or YAML, read with `jq` or `yq`. *Returns when* a row needs a value that `rg` cannot show cleanly.
- **Commands that pass or fail.** Today a row is judged only by how many results it shows. *Returns when* a fact is found that no count can express.
- **One ledger across several repositories.** The motivating case: follow one value from the screen that shows it, back through the service, the store and the pipeline, to the place that first produced it. *Returns with* the first ledger that needs two repositories.
- **A sandbox in place of shapes.** Run any command, read-only and with no network. It would end the need to list flags. *Returns when* listing flags becomes the larger cost.
- **A faster `ley since`.** It runs every row at every commit until the row stops holding. *Returns when* a real ledger makes that slow.
- **A walk that reads only what its rows name.** `ley since` puts every file of every commit it walks into its temporary directory, through the checkout filters git is set up with. *Returns when* a repository is large enough, or keeps enough large files, for that to cost.
- **Output for a program to read.** `ley show` prints sentences. A name or a source is free text, so only its place in the sentence says where it ends. *Returns when* a reader is seen to misread one.
- **Reading the ledger once.** Values are read by one TOML parser and positions by another, and `ley` checks that the two agree. One reading would need no such check. *Returns when* the rules of the file next change.
- **Rows from session logs.** Every search an agent ran that found something is a candidate row. Logs can also show how often the same thing is searched for again. *Returns when* there is a second ledger to compare against.
- **Measuring whether a ledger helps.** Does a reader with a ledger reach the right place in fewer steps than a reader without one, and trust the right things? *Returns when* there is one real ledger besides this repository's own. The first measurements should need no model: how long rows keep holding across real history, and how many of a place's facts can carry a command at all.
