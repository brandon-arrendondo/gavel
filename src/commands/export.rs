use std::fs;
use std::path::Path;

use crate::db;
use crate::error::{user, CliResult};
use crate::model::ExportItem;

pub fn run(db_path: &Path, status: &str, output: Option<&Path>) -> CliResult<()> {
    let conn = db::open(db_path)?;
    db::require_initialized(&conn)?;

    let filter = match status {
        "all" => None,
        s if db::STATUSES.contains(&s) => Some(s),
        s => {
            return Err(user(format!(
                "invalid --status '{s}' (expected one of: all, {})",
                db::STATUSES.join(", ")
            )))
        }
    };

    let items = db::list_items(&conn, filter)?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let verdict = db::load_verdict(&conn, &item.id)?;
        let comments = db::load_comments(&conn, &item.id)?;
        out.push(ExportItem::from_parts(item, verdict, comments));
    }

    let text = serde_json::to_string_pretty(&out).unwrap();
    match output {
        Some(path) => {
            fs::write(path, format!("{text}\n"))
                .map_err(|e| user(format!("cannot write {}: {e}", path.display())))?;
        }
        None => println!("{text}"),
    }
    Ok(())
}
