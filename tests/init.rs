mod common;

use assert_cmd::Command;
use common::Sandbox;

#[test]
fn init_creates_db() {
    let sb = Sandbox::raw();
    let mut cmd = Command::cargo_bin("gavel").unwrap();
    cmd.arg("--db").arg(&sb.db).arg("init").assert().success();
    assert!(sb.db.exists(), "db should exist after init");
}

#[test]
fn init_refuses_to_clobber_existing_db() {
    let sb = Sandbox::new(); // already initialized
    let mut cmd = Command::cargo_bin("gavel").unwrap();
    cmd.arg("--db").arg(&sb.db).arg("init");
    cmd.assert().failure().code(1);
}

#[test]
fn init_creates_gavel_dir_in_cwd_by_default() {
    let sb = Sandbox::raw();
    let mut cmd = Command::cargo_bin("gavel").unwrap();
    cmd.env_remove("GAVEL_DB")
        .current_dir(sb.path())
        .arg("init");
    cmd.assert().success();
    let db = sb.path().join(".gavel").join("gavel.db");
    assert!(db.is_file(), "expected .gavel/gavel.db in cwd");
}

#[test]
fn list_before_init_fails() {
    let sb = Sandbox::raw();
    let mut cmd = Command::cargo_bin("gavel").unwrap();
    cmd.arg("--db").arg(&sb.db).arg("list");
    // sqlite happily creates an empty file at --db, but it has no tables
    // yet, so this is a user error (run `gavel init` first), not a system one.
    cmd.assert().failure().code(1);
}
