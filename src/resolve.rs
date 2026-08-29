use std::env;
use std::path::{Path, PathBuf};

use crate::error::{user, CliResult};

pub const GAVEL_DIR: &str = ".gavel";
pub const DB_FILENAME: &str = "gavel.db";
pub const ENV_VAR: &str = "GAVEL_DB";

/// Resolve the database path.
///
/// Resolution order (first match wins):
///   1. `--db PATH` flag
///   2. `$GAVEL_DB` environment variable
///   3. Walk up from cwd looking for a `.gavel/gavel.db` file (like git
///      finds `.git`)
///   4. Error, hinting at `gavel init`
pub fn resolve_db_path(flag: Option<&Path>) -> CliResult<PathBuf> {
    if let Some(p) = flag {
        return Ok(p.to_path_buf());
    }
    if let Ok(val) = env::var(ENV_VAR) {
        if !val.is_empty() {
            return Ok(PathBuf::from(val));
        }
    }
    let cwd =
        env::current_dir().map_err(|e| user(format!("cannot read current directory: {e}")))?;
    if let Some(db) = find_db(&cwd) {
        return Ok(db);
    }
    Err(user(format!(
        "no database found. Set --db, ${ENV_VAR}, or run `gavel init` to create one."
    )))
}

fn find_db(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start.to_path_buf());
    while let Some(d) = dir {
        let candidate = d.join(GAVEL_DIR).join(DB_FILENAME);
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = d.parent().map(|p| p.to_path_buf());
    }
    None
}

/// Default database path for `gavel init`: `<dir>/.gavel/gavel.db`.
pub fn default_db_path(dir: &Path) -> PathBuf {
    dir.join(GAVEL_DIR).join(DB_FILENAME)
}
