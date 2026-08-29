mod common;

use common::Sandbox;

const ONE_ITEM: &str = r#"[{"title": "off-by-one in loop", "rule_id": "ARR30-C", "language": "c", "start_line": 10, "code": "for (i = 0; i <= n; i++) {}", "context": "check bound"}]"#;

const TWO_ITEMS_JSONL: &str = "{\"title\": \"a\", \"rule_id\": \"EXP34-C\", \"language\": \"c\", \"start_line\": 1, \"code\": \"x;\"}\n{\"title\": \"b\", \"rule_id\": \"MEM31-C\", \"language\": \"c\", \"start_line\": 2, \"code\": \"y;\"}\n";

#[test]
fn import_json_array_reports_count() {
    let sb = Sandbox::new();
    let file = sb.write_file("items.json", ONE_ITEM);
    let out = sb
        .cmd()
        .args(["import", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success());
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("imported 1"), "unexpected output: {s}");
}

#[test]
fn import_jsonl_imports_all_lines() {
    let sb = Sandbox::new();
    let ids = sb.import_and_list_ids(TWO_ITEMS_JSONL);
    assert_eq!(ids.len(), 2);
}

#[test]
fn imported_items_start_pending_and_get_fresh_uuids() {
    let sb = Sandbox::new();
    let ids = sb.import_and_list_ids(ONE_ITEM);
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0].len(), 36, "expected a uuid, got {}", ids[0]);

    let out = sb
        .cmd()
        .args(["list", "--status", "pending", "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["items"].as_array().unwrap().len(), 1);
}

#[test]
fn list_filters_by_status() {
    let sb = Sandbox::new();
    sb.import_and_list_ids(ONE_ITEM);

    let out = sb
        .cmd()
        .args(["list", "--status", "adjudicated", "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["items"].as_array().unwrap().len(), 0);
}

#[test]
fn list_rejects_invalid_status() {
    let sb = Sandbox::new();
    sb.cmd()
        .args(["list", "--status", "bogus"])
        .assert()
        .failure()
        .code(1);
}

#[test]
fn import_invalid_json_is_user_error() {
    let sb = Sandbox::new();
    let file = sb.write_file("bad.json", "not json at all");
    sb.cmd()
        .args(["import", file.to_str().unwrap()])
        .assert()
        .failure()
        .code(1);
}
