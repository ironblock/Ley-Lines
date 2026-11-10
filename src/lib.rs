//! Ley Lines: a ledger of facts about one place, each with a command that shows it.
//!
//! - [`ledger`]: the file format and the rules a row must follow.
//! - [`command`]: split a row's command into words, with no shell.
//! - [`shape`]: the commands `ley check` is willing to run.
//! - [`run`]: run one, and count what it shows.
//! - [`check`], [`show`], [`since`]: the three things `ley` does with a ledger.
//! - [`rules`]: the rules a ledger follows, read from this project's own ledger.

pub mod check;
pub mod command;
pub mod ledger;
pub mod rules;
pub mod run;
pub mod shape;
pub mod show;
pub mod since;
