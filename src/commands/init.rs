use std::env;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::db;
use crate::error::{user, CliResult};
use crate::resolve;

pub fn run(db_flag: Option<&Path>, dir: Option<&Path>, json: bool) -> CliResult<()> {
    let db_path = resolve_init_path(db_flag, dir)?;

    if db_path.exists() {
        return Err(user(format!(
            "database already exists at {}; refusing to clobber",
            db_path.display()
        )));
    }
    if let Some(parent) = db_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| {
                user(format!(
                    "cannot create parent directory {}: {e}",
                    parent.display()
                ))
            })?;
        }
    }

    let conn = db::open(&db_path)?;
    db::create_schema(&conn)?;

    if json {
        let out = json!({
            "db": db_path.display().to_string(),
            "schema_version": db::SCHEMA_VERSION,
        });
        println!("{}", serde_json::to_string(&out).unwrap());
    } else {
        println!("initialized {}", db_path.display());
    }
    Ok(())
}

fn resolve_init_path(db_flag: Option<&Path>, dir: Option<&Path>) -> CliResult<PathBuf> {
    if let Some(p) = db_flag {
        return Ok(p.to_path_buf());
    }
    let target_dir = match dir {
        Some(d) => d.to_path_buf(),
        None => {
            env::current_dir().map_err(|e| user(format!("cannot read current directory: {e}")))?
        }
    };
    Ok(resolve::default_db_path(&target_dir))
}
