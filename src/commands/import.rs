use std::fs;
use std::path::Path;

use serde_json::json;

use crate::db::{self, ReviewItem};
use crate::error::{user, CliResult};
use crate::model::ImportItem;

pub fn run(db_path: &Path, json_out: bool, file: &Path) -> CliResult<()> {
    let conn = db::open(db_path)?;
    db::require_initialized(&conn)?;

    let content = fs::read_to_string(file)
        .map_err(|e| user(format!("cannot read {}: {e}", file.display())))?;
    let items = parse_input(&content)?;

    let n = items.len();
    for it in items {
        let review_item = ReviewItem {
            id: uuid::Uuid::new_v4().to_string(),
            title: it.title,
            rule_id: it.rule_id,
            rule_text: it.rule_text,
            language: it.language,
            file_path: it.file_path,
            start_line: it.start_line,
            code: it.code,
            context: it.context,
            status: "pending".to_string(),
            created_at: db::now_iso(),
        };
        db::insert_item(&conn, &review_item)?;
    }

    if json_out {
        println!(
            "{}",
            serde_json::to_string(&json!({"imported": n})).unwrap()
        );
    } else {
        println!("imported {n} review item(s)");
    }
    Ok(())
}

/// Parse either a JSON array of items, or JSONL (one object per non-blank line).
fn parse_input(content: &str) -> CliResult<Vec<ImportItem>> {
    let trimmed = content.trim_start();
    if trimmed.starts_with('[') {
        return serde_json::from_str(content)
            .map_err(|e| user(format!("invalid JSON array of review items: {e}")));
    }
    let mut out = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let item: ImportItem = serde_json::from_str(line)
            .map_err(|e| user(format!("invalid JSONL on line {}: {e}", i + 1)))?;
        out.push(item);
    }
    if out.is_empty() {
        return Err(user("no review items found in input file"));
    }
    Ok(out)
}
