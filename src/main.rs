mod cli;
mod commands;
mod db;
mod error;
mod model;
mod resolve;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::{Cli, Command};
use crate::error::CliResult;

fn main() -> ExitCode {
    let cli = Cli::parse();
    match dispatch(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(e.exit_code() as u8)
        }
    }
}

fn dispatch(cli: Cli) -> CliResult<()> {
    let db_flag = cli.db.as_deref();
    let json = cli.json;

    match cli.command {
        Command::Init { dir } => commands::init::run(db_flag, dir.as_deref(), json),
        other => {
            let db_path = resolve::resolve_db_path(db_flag)?;
            run_command(other, &db_path, json)
        }
    }
}

fn run_command(cmd: Command, db_path: &std::path::Path, json: bool) -> CliResult<()> {
    match cmd {
        Command::Init { .. } => unreachable!("Init handled upstream"),
        Command::Import { file } => commands::import::run(db_path, json, &file),
        Command::List { status } => commands::list::run(db_path, json, status.as_deref()),
        Command::Show { id } => commands::show::run(db_path, json, &id),
        Command::Review { id, reviewer } => {
            commands::review::run(db_path, id.as_deref(), reviewer.as_deref())
        }
        Command::Export { status, output } => {
            commands::export::run(db_path, &status, output.as_deref())
        }
        Command::Stats => commands::stats::run(db_path, json),
    }
}
