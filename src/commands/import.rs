use std::fs;
use std::path::Path;

use serde_json::json;

use crate::db::{self, ReviewItem};
use crate::error::{user, CliResult};
use crate::model::ImportItem;

pub fn run(db_path: &Path, json_out: bool, file: &Path, decisions: Option<&str>) -> CliResult<()> {
    let conn = db::open(db_path)?;
    db::require_initialized(&conn)?;

    // Validate and persist before touching review_items, so a bad --decisions
    // value fails the whole import rather than leaving items imported against
    // a vocabulary restriction that never got set.
    let allowed_decisions = match decisions {
        Some(raw) => {
            let parsed = db::parse_allowed_decisions(raw)?;
            db::set_allowed_decisions(&conn, &parsed)?;
            Some(parsed)
        }
        None => None,
    };

    let content = fs::read_to_string(file)
        .map_err(|e| user(format!("cannot read {}: {e}", file.display())))?;
    let items = parse_input(&content)?;

    let n = items.len();
    for it in items {
        let review_item = ReviewItem {
            id: uuid::Uuid::new_v4().to_string(),
            external_id: it.external_id,
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
            serde_json::to_string(&json!({"imported": n, "allowed_decisions": allowed_decisions}))
                .unwrap()
        );
    } else {
        println!("imported {n} review item(s)");
        if let Some(d) = &allowed_decisions {
            println!("review decisions restricted to: {}", d.join(", "));
        }
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
