use std::path::Path;

use serde_json::json;

use crate::db::{self, STATUSES};
use crate::error::{user, CliResult};

pub fn run(db_path: &Path, json_out: bool, status: Option<&str>) -> CliResult<()> {
    let conn = db::open(db_path)?;
    db::require_initialized(&conn)?;

    if let Some(s) = status {
        if !STATUSES.contains(&s) {
            return Err(user(format!(
                "invalid --status '{s}' (expected one of: {})",
                STATUSES.join(", ")
            )));
        }
    }

    let items = db::list_items(&conn, status)?;

    if json_out {
        let arr: Vec<_> = items
            .iter()
            .map(|it| {
                json!({
                    "id": it.id,
                    "title": it.title,
                    "rule_id": it.rule_id,
                    "status": it.status,
                })
            })
            .collect();
        println!("{}", serde_json::to_string(&json!({"items": arr})).unwrap());
        return Ok(());
    }

    if items.is_empty() {
        println!("(no items)");
        return Ok(());
    }

    println!("{:<10} {:<12} {:<40} STATUS", "ID", "RULE", "TITLE");
    for it in &items {
        let short_id = &it.id[..8.min(it.id.len())];
        let title: String = if it.title.chars().count() > 40 {
            let mut t: String = it.title.chars().take(39).collect();
            t.push('…');
            t
        } else {
            it.title.clone()
        };
        println!(
            "{:<10} {:<12} {:<40} {}",
            short_id, it.rule_id, title, it.status
        );
    }
    Ok(())
}
