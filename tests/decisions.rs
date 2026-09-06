mod common;

use common::Sandbox;
use rusqlite::Connection;

const ONE_ITEM: &str = r#"[{"title": "off-by-one in loop", "rule_id": "ARR30-C", "language": "c", "start_line": 10, "code": "for (i = 0; i <= n; i++) {}", "context": "check bound"}]"#;

fn meta_value(db: &std::path::Path, key: &str) -> Option<String> {
    let conn = Connection::open(db).unwrap();
    conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
        .ok()
}

#[test]
fn import_without_decisions_leaves_full_vocabulary() {
    let sb = Sandbox::new();
    sb.import_and_list_ids(ONE_ITEM);
    assert_eq!(meta_value(&sb.db, "allowed_decisions"), None);
}

#[test]
fn import_decisions_flag_persists_in_given_order() {
    let sb = Sandbox::new();
    let file = sb.write_file("items.json", ONE_ITEM);
    let out = sb
        .cmd()
        .args([
            "import",
            file.to_str().unwrap(),
            "--decisions",
            "violation,false_positive,uncertain",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["allowed_decisions"],
        serde_json::json!(["violation", "false_positive", "uncertain"])
    );
    assert_eq!(
        meta_value(&sb.db, "allowed_decisions"),
        Some("violation,false_positive,uncertain".to_string())
    );
}

#[test]
fn import_decisions_rejects_unknown_value() {
    let sb = Sandbox::new();
    let file = sb.write_file("items.json", ONE_ITEM);
    sb.cmd()
        .args([
            "import",
            file.to_str().unwrap(),
            "--decisions",
            "violation,not_a_real_decision",
        ])
        .assert()
        .failure()
        .code(1);

    // Rejected before any item was inserted -- not a partial import.
    let out = sb.cmd().args(["list", "--json"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["items"].as_array().unwrap().len(), 0);
}

#[test]
fn import_decisions_rejects_empty_list() {
    let sb = Sandbox::new();
    let file = sb.write_file("items.json", ONE_ITEM);
    sb.cmd()
        .args(["import", file.to_str().unwrap(), "--decisions", " , ,"])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn later_import_without_decisions_keeps_earlier_restriction() {
    let sb = Sandbox::new();
    let file = sb.write_file("items.json", ONE_ITEM);
    sb.cmd()
        .args([
            "import",
            file.to_str().unwrap(),
            "--decisions",
            "violation,false_positive",
        ])
        .assert()
        .success();
    sb.cmd()
        .args(["import", file.to_str().unwrap()])
        .assert()
        .success();

    assert_eq!(
        meta_value(&sb.db, "allowed_decisions"),
        Some("violation,false_positive".to_string())
    );
}

#[test]
fn later_import_with_decisions_overwrites_earlier_restriction() {
    let sb = Sandbox::new();
    let file = sb.write_file("items.json", ONE_ITEM);
    sb.cmd()
        .args([
            "import",
            file.to_str().unwrap(),
            "--decisions",
            "violation,false_positive",
        ])
        .assert()
        .success();
    sb.cmd()
        .args(["import", file.to_str().unwrap(), "--decisions", "compliant"])
        .assert()
        .success();

    assert_eq!(
        meta_value(&sb.db, "allowed_decisions"),
        Some("compliant".to_string())
    );
}
