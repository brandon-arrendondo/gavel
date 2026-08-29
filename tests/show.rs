mod common;

use common::Sandbox;

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
