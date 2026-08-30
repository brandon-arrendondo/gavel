mod common;

use common::Sandbox;
use rusqlite::{params, Connection};

const ONE_ITEM: &str = r#"[{"title": "off-by-one in loop", "rule_id": "ARR30-C", "rule_text": "no OOB access", "language": "c", "file_path": "src/x.c", "start_line": 10, "code": "for (i = 0; i <= n; i++) {}", "context": "check bound"}]"#;

#[test]
fn show_by_full_id_returns_item_json() {
    let sb = Sandbox::new();
    let ids = sb.import_and_list_ids(ONE_ITEM);
    let out = sb.cmd().args(["show", &ids[0], "--json"]).output().unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["id"], ids[0]);
    assert_eq!(v["rule_id"], "ARR30-C");
    assert_eq!(v["status"], "pending");
    assert!(v["verdict"].is_null());
    assert_eq!(v["line_comments"].as_array().unwrap().len(), 0);
}

#[test]
fn show_by_short_id_prefix_resolves() {
    let sb = Sandbox::new();
    let ids = sb.import_and_list_ids(ONE_ITEM);
    let short = &ids[0][..8];
    let out = sb.cmd().args(["show", short, "--json"]).output().unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["id"], ids[0]);
}

#[test]
fn show_unknown_id_is_user_error() {
    let sb = Sandbox::new();
    sb.cmd()
        .args(["show", "does-not-exist"])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn show_text_output_includes_line_numbers() {
    let sb = Sandbox::new();
    let ids = sb.import_and_list_ids(ONE_ITEM);
    let out = sb.cmd().args(["show", &ids[0]]).output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("10 |"), "expected line-numbered code, got:\n{s}");
    assert!(s.contains("ARR30-C"));
}

/// A comment's `line_number` is stored 1-indexed and relative to the
/// snippet (see docs/data-model.md), but `show`'s text output must
/// translate it back to the original file's line number using the item's
/// `start_line` — showing the raw relative index here is meaningless to a
/// human reviewer looking at the source file.
#[test]
fn show_text_output_translates_comment_line_to_file_line() {
    let sb = Sandbox::new();
    let ids = sb.import_and_list_ids(ONE_ITEM);
    let id = &ids[0];

    // start_line is 10; a comment on snippet line 3 is file line 12.
    let conn = Connection::open(&sb.db).unwrap();
    conn.execute(
        "INSERT INTO line_comments(id, item_id, line_number, comment, created_at) \
         VALUES('c1', ?1, 3, 'here', '2026-01-01T00:00:00Z')",
        params![id],
    )
    .unwrap();
    drop(conn);

    let out = sb.cmd().args(["show", id]).output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(
        s.contains("line 12: here"),
        "expected comment translated to file line 12, got:\n{s}"
    );
    assert!(
        !s.contains("line 3: here"),
        "comment should not show the raw snippet-relative line number, got:\n{s}"
    );
}
