mod common;

use common::Sandbox;
use rusqlite::{params, Connection};

const ONE_ITEM: &str = r#"[{"title": "off-by-one in loop", "rule_id": "ARR30-C", "language": "c", "start_line": 10, "code": "for (i = 0; i <= n; i++) {}", "context": "check bound"}]"#;

const ONE_ITEM_WITH_EXTERNAL_ID: &str = r#"[{"external_id": "gt-42", "title": "off-by-one in loop", "rule_id": "ARR30-C", "language": "c", "start_line": 10, "code": "for (i = 0; i <= n; i++) {}", "context": "check bound"}]"#;

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

#[test]
fn export_echoes_external_id_verbatim() {
    let sb = Sandbox::new();
    sb.import_and_list_ids(ONE_ITEM_WITH_EXTERNAL_ID);

    let out = sb
        .cmd()
        .args(["export", "--status", "all"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["external_id"], "gt-42");
}

#[test]
fn export_external_id_is_null_when_absent() {
    let sb = Sandbox::new();
    sb.import_and_list_ids(ONE_ITEM);

    let out = sb
        .cmd()
        .args(["export", "--status", "all"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert!(arr[0]["external_id"].is_null());
}

/// A database created under schema v1 (no `external_id` column) must open
/// and migrate cleanly, with old rows reading back with a null external_id.
#[test]
fn v1_database_migrates_and_gains_null_external_id() {
    let sb = Sandbox::raw();
    let conn = Connection::open(&sb.db).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE review_items (
            id         TEXT PRIMARY KEY,
            title      TEXT NOT NULL,
            rule_id    TEXT NOT NULL,
            rule_text  TEXT,
            language   TEXT NOT NULL,
            file_path  TEXT,
            start_line INTEGER NOT NULL,
            code       TEXT NOT NULL,
            context    TEXT,
            status     TEXT NOT NULL CHECK(status IN ('pending','in_review','adjudicated')),
            created_at TEXT NOT NULL
        );
        CREATE TABLE line_comments (
            id          TEXT PRIMARY KEY,
            item_id     TEXT NOT NULL REFERENCES review_items(id) ON DELETE CASCADE,
            line_number INTEGER NOT NULL,
            comment     TEXT NOT NULL,
            created_at  TEXT NOT NULL
        );
        CREATE TABLE verdicts (
            item_id     TEXT PRIMARY KEY REFERENCES review_items(id) ON DELETE CASCADE,
            decision    TEXT NOT NULL CHECK(decision IN ('compliant','violation','false_positive','needs_more_context','uncertain')),
            rationale   TEXT,
            reviewer    TEXT,
            reviewed_at TEXT NOT NULL
        );
        CREATE TABLE meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        INSERT INTO meta(key, value) VALUES('schema_version', '1');
        INSERT INTO review_items(id, title, rule_id, language, start_line, code, status, created_at)
            VALUES('11111111-1111-1111-1111-111111111111', 'pre-migration item', 'ARR30-C', 'c', 1, 'code', 'pending', '2026-01-01T00:00:00Z');
        "#,
    )
    .unwrap();
    drop(conn);

    let out = sb
        .cmd()
        .args(["export", "--status", "all"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], "11111111-1111-1111-1111-111111111111");
    assert!(arr[0]["external_id"].is_null());

    let conn = Connection::open(&sb.db).unwrap();
    let schema_version: String = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(schema_version, "2");
}
