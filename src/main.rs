use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use ley_lines::ledger::{self, Ledger, LoadError};
use ley_lines::since::{Change, Found, Stopped};
use ley_lines::text::{count, shown};
use ley_lines::{check, rules, run, shape, show, since};

const THE_FILE: &str = "\
A row is a block of ley.toml, with one field on each line:

  [[row]]
  name = \"…\"
  is = \"…\"
  said = [{ by = \"…\", on = 2026-11-10 }]

  [[row]]
  name = \"…\"
  run = \"…\"
  shows = \"…\"
  count = 3

  name    the term a reader would look up
  is      what the thing is; it needs `said`
  said    who said so, and when
  run     a command; `ley shapes` lists the commands `ley check` will run
  shows   what kind of thing the command shows, not the thing itself;
          it completes the sentence \"it will show you …\"
  count   how many results the command shows: 1 when left out, 0 when the
          command shows nothing
  since, sha    written by `ley since`

A row has `is`, or `run`, or both. `ley rules` prints the rules a ledger follows.";

/// A ledger of facts about one place, each with a command that shows it.
///
/// The ledger is a file named ley.toml, made of rows. A row says
/// what a thing is and who said so, or it gives a command: "If you run X, it
/// will show you Y." The row stores the command and never its output, so a
/// reader sees the place as it is now. `ley check` runs the commands and
/// reports every row that no longer shows what it says.
#[derive(Parser)]
#[command(name = "ley", version, max_term_width = 100, after_long_help = THE_FILE)]
struct Cli {
    /// The ledger to read. Without this, `ley` looks for ley.toml in the current
    /// directory, then in each directory above it.
    #[arg(long, global = true, value_name = "FILE")]
    ledger: Option<PathBuf>,

    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print what the ledger says about a name
    ///
    /// With no name, lists every name in the ledger. With a name, prints each
    /// row that has that name. If no row has exactly that name, prints each row
    /// whose name contains it. Letter case is ignored. Nothing is run.
    Show {
        /// The names to look up
        names: Vec<String>,
    },
    /// Run each row's command and compare what it shows with what the row says
    ///
    /// Each row with a command ends in one of five states.
    ///
    /// "holds": the command showed as many results as the row says.
    ///
    /// "differs": it showed a different number.
    ///
    /// "restates": it showed the right number, and the row's `shows` repeats
    /// part of what the command printed. A row says what kind of thing its
    /// command shows. It does not say the thing.
    ///
    /// "could not run": the program is missing, too old, or said it failed.
    ///
    /// "not checked": the command fits no shape, so `ley` did not run it. See
    /// `ley shapes`.
    ///
    /// A row with no command has nothing to run. It is counted, and it never
    /// fails. A row is judged by what its command shows, never by the
    /// command's exit status.
    ///
    /// Commands run in the ledger's directory, with no shell. `ley check` exits
    /// with 0 when every command that ran holds.
    Check {
        /// Also fail when a row is not checked
        #[arg(long)]
        strict: bool,
        /// Also list the rows that hold
        #[arg(short, long)]
        verbose: bool,
    },
    /// Record the earliest commit at which each row held
    ///
    /// Walks back through git history from the current commit, running each
    /// row's command as it is written now, until the row stops holding. Writes
    /// the date and commit of the oldest commit at which it held into the
    /// ledger, as `since` and `sha`. The date is the commit's day in UTC.
    ///
    /// Each commit's files are put into a temporary directory. Nothing is
    /// written into the repository, none of its hooks run, and your own files
    /// are not touched, apart from the ledger.
    ///
    /// A recorded date moves earlier when more history is read. It moves later
    /// only when the history has the recorded commit and shows a commit after
    /// it at which the row did not hold. A shallow clone or a squashed history
    /// does not move it. A row that does not hold at the current commit loses
    /// its date.
    Since {
        /// Print what would be recorded, and write nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// List the commands `check` will run
    Shapes,
    /// Print the rules a ledger follows
    Rules,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(message) => {
            // A message may quote a file name. None of it is printed raw.
            let lines: Vec<String> = message.lines().map(shown).collect();
            eprintln!("ley: {}", lines.join("\n     "));
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, String> {
    match cli.command {
        Cmd::Shapes => {
            print!("{}", shape::describe());
            return Ok(ExitCode::SUCCESS);
        }
        Cmd::Rules => {
            print!("{}", rules::describe());
            return Ok(ExitCode::SUCCESS);
        }
        _ => {}
    }

    let path = locate(cli.ledger)?;
    let ledger = match ledger::load(&path) {
        Ok(ledger) => ledger,
        Err(LoadError::Unreadable(why)) => return Err(why),
        Err(LoadError::Problems(problems)) => {
            for problem in &problems {
                println!("{}:{}: {}", path.display(), problem.line, problem.message);
            }
            let n = problems.len();
            println!(
                "{} in the ledger. Nothing was run.",
                count(n as u64, "problem", "problems")
            );
            return Ok(ExitCode::FAILURE);
        }
    };

    match cli.command {
        Cmd::Shapes | Cmd::Rules => unreachable!("handled above"),
        Cmd::Show { names } => Ok(show_rows(&ledger, &names)),
        Cmd::Check { strict, verbose } => {
            let (lines, tally) = check::check(&ledger, verbose, run::LIMIT);
            for line in lines {
                println!("{line}");
            }
            println!("{}", tally.sentence());
            Ok(if tally.ok(strict) {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
        Cmd::Since { dry_run } => record_since(&ledger, dry_run),
    }
}

/// The ledger named on the command line, or the nearest ley.toml.
fn locate(named: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(path) = named {
        return Ok(path);
    }
    let here = std::env::current_dir().map_err(|e| format!("no current directory: {e}"))?;
    let found = ledger::find(&here).ok_or_else(|| {
        format!(
            "no {} in {} or any directory above it",
            ledger::FILE_NAME,
            here.display()
        )
    })?;
    // Name the file the way the reader would: relative to where they are.
    Ok(match found.strip_prefix(&here) {
        Ok(relative) => relative.to_path_buf(),
        Err(_) => found,
    })
}

fn show_rows(ledger: &Ledger, names: &[String]) -> ExitCode {
    if names.is_empty() {
        print!("{}", show::index(ledger));
        return ExitCode::SUCCESS;
    }
    let mut code = ExitCode::SUCCESS;
    let mut first = true;
    for name in names {
        let rows = show::select(ledger, name);
        if rows.is_empty() {
            eprintln!("ley: no row has a name containing \"{}\"", shown(name));
            code = ExitCode::FAILURE;
            continue;
        }
        if !first {
            println!();
        }
        first = false;
        print!("{}", show::render(&rows));
    }
    code
}

fn record_since(ledger: &Ledger, dry_run: bool) -> Result<ExitCode, String> {
    let walk = since::since(ledger, run::LIMIT)?;
    let file = ledger.path.display();
    let mut updates = Vec::new();
    let mut could_not_run = 0;
    for entry in &walk.entries {
        let at = format!("{file}:{}: \"{}\"", entry.line, entry.name);
        let recorded = ledger.rows[entry.index]
            .pointer()
            .and_then(|pointer| pointer.since.as_ref());
        let change = since::decide(recorded, &entry.found, walk.shallow);
        match &entry.found {
            Found::NotChecked(why) => println!("{at}: not checked: {why}"),
            Found::CouldNotRun(why) => {
                could_not_run += 1;
                println!("{at}: could not run: {why}");
            }
            Found::NotNow(why) => {
                let then = match change {
                    Change::Removed => "the recorded date is removed",
                    _ => "nothing recorded",
                };
                println!("{at}: does not hold at the current commit ({why}): {then}");
            }
            Found::Since {
                since,
                commits,
                stopped,
                ..
            } => {
                let sha = since.sha.as_deref().unwrap_or("");
                let reach = match stopped {
                    Stopped::NoOlderCommit => "the oldest commit here".to_string(),
                    _ => format!("held at {}", count(*commits as u64, "commit", "commits")),
                };
                let found = format!("true since at least {} (commit {sha}, {reach})", since.date);
                match &change {
                    Change::New(_) => println!("{at}: {found}: new"),
                    Change::Earlier(_) => println!("{at}: {found}: moved earlier"),
                    Change::Later { broke, .. } => println!(
                        "{at}: {found}: moved later, because the row did not hold at commit {}",
                        broke.sha
                    ),
                    // The date a row keeps may be older than the commit beside it.
                    Change::Unchanged => println!(
                        "{at}: true since at least {} (commit {sha}, {reach}): unchanged",
                        recorded.map_or(since.date, |recorded| recorded.date)
                    ),
                    Change::Commit(kept) => println!(
                        "{at}: true since at least {}: the date is kept, and the commit is now {sha}, the earliest one here at which the row is known to hold",
                        kept.date
                    ),
                    Change::Kept => println!(
                        "{at}: the recorded date is kept: {}",
                        match stopped {
                            Stopped::NoOlderCommit => "this clone holds only part of the history",
                            Stopped::Broke(_) =>
                                "the row is not seen to break after the commit it records",
                            Stopped::CouldNotRun { .. } => "the walk could not go further back",
                        }
                    ),
                    Change::Removed | Change::Nothing => {}
                }
                if let Stopped::CouldNotRun { at: commit, why } = stopped {
                    could_not_run += 1;
                    println!(
                        "    the walk stopped at commit {}, where the command could not run: {why}",
                        commit.sha
                    );
                }
            }
        }
        if let Some(update) = change.update() {
            updates.push((entry.index, update));
        }
    }
    if updates.is_empty() {
        println!("Nothing to record.");
    } else if dry_run {
        println!(
            "{} would change.",
            count(updates.len() as u64, "row", "rows")
        );
    } else {
        write(ledger, &updates)?;
        println!(
            "Recorded {} in {file}.",
            count(updates.len() as u64, "row", "rows")
        );
    }
    Ok(if could_not_run == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn write(ledger: &Ledger, updates: &[(usize, Option<ledger::Since>)]) -> Result<(), String> {
    let path = &ledger.path;
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    // The updates name rows by their position in the ledger as it was when the walk began.
    if text != ledger.text {
        return Err(format!(
            "{} changed while `ley since` was reading history; nothing was written. Run it again",
            path.display()
        ));
    }
    let text = ledger::record_since(&text, updates)?;
    // Never write a ledger that `ley` itself would refuse.
    if let Err(problems) = ledger::parse(&text) {
        return Err(format!(
            "recording would break the ledger ({}); nothing was written",
            problems[0].message
        ));
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}
