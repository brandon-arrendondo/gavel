use std::path::PathBuf;

use clap::{Parser, Subcommand};

const LONG_ABOUT: &str = "\
Terminal review workstation for adjudicating CERT-C secure-coding findings
that an AI agent (Claude) flagged in C/C++ source. Import review items as
JSON/JSONL, work through them with `gavel review` (a ratatui TUI), and
export adjudicated verdicts back out as JSON for the agent to consume.

Database resolution (first match wins):
  1. --db PATH flag
  2. GAVEL_DB environment variable
  3. Walk up from cwd looking for a .gavel/gavel.db file (like git finds .git)
  4. Otherwise exit 1 with a hint to run `gavel init`.

Exit codes: 0 success, 1 user error, 2 system error.";

#[derive(Parser, Debug)]
#[command(
    name = "gavel",
    version,
    about = "Human review workstation for AI-flagged CERT-C findings",
    long_about = LONG_ABOUT
)]
pub struct Cli {
    /// Path to the SQLite database. Overrides $GAVEL_DB and the .gavel/gavel.db lookup.
    #[arg(long, global = true, value_name = "PATH")]
    pub db: Option<PathBuf>,

    /// Emit machine-readable JSON output where the command supports it.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Initialize a new database at .gavel/gavel.db in the target directory.
    Init {
        /// Directory in which to create .gavel/ (defaults to cwd). Ignored when --db is passed.
        #[arg(long, value_name = "PATH")]
        dir: Option<PathBuf>,
    },

    /// Import review items from a JSON array or JSONL file. Prints the count imported.
    Import {
        /// Path to the JSON (array) or JSONL (one object per line) input file.
        file: PathBuf,

        /// Restrict the decisions `gavel review` offers to this comma-separated
        /// subset, e.g. "violation,false_positive,uncertain". Values must come
        /// from the full vocabulary: compliant, violation, false_positive,
        /// needs_more_context, uncertain. The TUI's numbered decision keys are
        /// assigned 1..N in the order given here. Persists for the whole
        /// database until a later import passes a different list — omit to
        /// leave (or keep) the full 5-value vocabulary offered.
        #[arg(long, value_name = "LIST")]
        decisions: Option<String>,
    },

    /// List review items.
    List {
        /// Filter by status: pending | in_review | adjudicated.
        #[arg(long)]
        status: Option<String>,
    },

    /// Show full detail for one item: snippet with line numbers, rule, context, comments, verdict.
    Show {
        /// Item id (or unambiguous prefix, as printed by `list`).
        id: String,
    },

    /// Launch the interactive TUI to adjudicate items.
    Review {
        /// Review one specific item and exit the queue afterward.
        #[arg(long)]
        id: Option<String>,
        /// Reviewer name recorded on verdicts (defaults to $USER).
        #[arg(long)]
        reviewer: Option<String>,
    },

    /// Export items as a JSON array. Defaults to only adjudicated items.
    Export {
        /// Filter by status: pending | in_review | adjudicated | all. Defaults to adjudicated.
        #[arg(long, default_value = "adjudicated")]
        status: String,
        /// Write to this file instead of stdout.
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
    },

    /// Print counts by status and by verdict decision.
    Stats,
}
