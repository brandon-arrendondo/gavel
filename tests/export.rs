mod common;

use common::Sandbox;
use rusqlite::{params, Connection};

const ONE_ITEM: &str = r#"[{"title": "off-by-one in loop", "rule_id": "ARR30-C", "language": "c", "start_line": 10, "code": "for (i = 0; i <= n; i++) {}", "context": "check bound"}]"#;

#[test]
fn export_default_status_adjudicated_is_empty_with_no_verdicts() {
    let sb = Sandbox::new();
    sb.import_and_list_ids(ONE_ITEM);

    // Default --status is "adjudicated"; nothing has a verdict yet.
    let out = sb.cmd().args(["export"]).output().unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v.as_array().unwrap().len(),
        0,
        "expected empty export, got: {v}"
    );
}

#[test]
fn export_status_all_includes_pending_items() {
    let sb = Sandbox::new();
    let ids = sb.import_and_list_ids(ONE_ITEM);

    let out = sb
        .cmd()
        .args(["export", "--status", "all"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], ids[0]);
    assert_eq!(arr[0]["status"], "pending");
    assert!(arr[0]["verdict"].is_null());
}

#[test]
fn export_round_trip_with_verdict_and_comment() {
    let sb = Sandbox::new();
    let ids = sb.import_and_list_ids(ONE_ITEM);
    let id = &ids[0];

    // Simulate what `gavel review` would have written: a line comment and a
    // verdict, inserted directly since there's no non-interactive CLI path
    // to set a verdict (the TUI is the only writer).
    let conn = Connection::open(&sb.db).unwrap();
    conn.execute(
        "INSERT INTO line_comments(id, item_id, line_number, comment, created_at) \
         VALUES('c1', ?1, 1, 'off-by-one confirmed', '2026-01-01T00:00:00Z')",
        params![id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO verdicts(item_id, decision, rationale, reviewer, reviewed_at) \
         VALUES(?1, 'violation', 'confirmed OOB read', 'brandon', '2026-01-01T00:00:00Z')",
        params![id],
    )
    .unwrap();
    conn.execute(
        "UPDATE review_items SET status = 'adjudicated' WHERE id = ?1",
        params![id],
    )
    .unwrap();
    drop(conn);

    // Default export (adjudicated only) should now include it.
    let out = sb.cmd().args(["export"]).output().unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], *id);
    assert_eq!(arr[0]["status"], "adjudicated");
    assert_eq!(arr[0]["verdict"]["decision"], "violation");
    assert_eq!(arr[0]["verdict"]["reviewer"], "brandon");
    let comments = arr[0]["line_comments"].as_array().unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0]["line_number"], 1);
    assert_eq!(comments[0]["comment"], "off-by-one confirmed");
}

#[test]
fn export_to_file_writes_valid_json() {
    let sb = Sandbox::new();
    sb.import_and_list_ids(ONE_ITEM);
    let out_path = sb.path().join("out.json");

    sb.cmd()
        .args([
            "export",
            "--status",
            "all",
            "-o",
            out_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    let contents = std::fs::read_to_string(&out_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&contents).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 1);
}
