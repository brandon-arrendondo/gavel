use std::path::Path;

use crate::db;
use crate::error::CliResult;
use crate::model::ExportItem;

pub fn run(db_path: &Path, json_out: bool, id: &str) -> CliResult<()> {
    let conn = db::open(db_path)?;
    db::require_initialized(&conn)?;

    let item = db::resolve_one(&conn, id)?;
    let verdict = db::load_verdict(&conn, &item.id)?;
    let comments = db::load_comments(&conn, &item.id)?;

    if json_out {
        let out = ExportItem::from_parts(item, verdict, comments);
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
        return Ok(());
    }

    println!("{}", item.title);
    println!("id:       {}", item.id);
    if let Some(ext) = &item.external_id {
        println!("external: {ext}");
    }
    println!("rule:     {}", item.rule_id);
    if let Some(rt) = &item.rule_text {
        println!("rule_text: {rt}");
    }
    println!("language: {}", item.language);
    if let Some(fp) = &item.file_path {
        println!("file:     {fp}:{}", item.start_line);
    }
    println!("status:   {}", item.status);
    println!();
    println!("--- code ---");
    for (i, line) in item.code.lines().enumerate() {
        let lineno = item.start_line + i as i64;
        println!("{lineno:>6} | {line}");
    }
    if let Some(ctx) = &item.context {
        println!();
        println!("--- context ---");
        println!("{ctx}");
    }
    if !comments.is_empty() {
        println!();
        println!("--- comments ---");
        for c in &comments {
            let file_line = item.start_line + c.line_number - 1;
            println!("  line {file_line}: {}", c.comment);
        }
    }
    if let Some(v) = &verdict {
        println!();
        println!("--- verdict ---");
        println!("decision:    {}", v.decision);
        if let Some(r) = &v.rationale {
            println!("rationale:   {r}");
        }
        if let Some(r) = &v.reviewer {
            println!("reviewer:    {r}");
        }
        println!("reviewed_at: {}", v.reviewed_at);
    }
    Ok(())
}
