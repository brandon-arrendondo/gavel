use std::path::Path;

use serde_json::json;

use crate::db;
use crate::error::CliResult;

pub fn run(db_path: &Path, json_out: bool) -> CliResult<()> {
    let conn = db::open(db_path)?;
    db::require_initialized(&conn)?;

    let by_status = db::count_by_status(&conn)?;
    let by_decision = db::count_by_decision(&conn)?;

    if json_out {
        let status_obj: serde_json::Map<_, _> = by_status
            .iter()
            .map(|(k, v)| (k.clone(), json!(v)))
            .collect();
        let decision_obj: serde_json::Map<_, _> = by_decision
            .iter()
            .map(|(k, v)| (k.clone(), json!(v)))
            .collect();
        let out = json!({"by_status": status_obj, "by_decision": decision_obj});
        println!("{}", serde_json::to_string(&out).unwrap());
        return Ok(());
    }

    println!("by status:");
    if by_status.is_empty() {
        println!("  (none)");
    }
    for (status, count) in &by_status {
        println!("  {status:<16} {count}");
    }
    println!();
    println!("by verdict:");
    if by_decision.is_empty() {
        println!("  (none)");
    }
    for (decision, count) in &by_decision {
        println!("  {decision:<20} {count}");
    }
    Ok(())
}
