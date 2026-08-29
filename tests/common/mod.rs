#![allow(dead_code)]

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;

pub struct Sandbox {
    pub dir: TempDir,
    pub db: PathBuf,
}

impl Sandbox {
    /// New sandbox with an initialized DB.
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("gavel.db");
        let mut cmd = Command::cargo_bin("gavel").unwrap();
        cmd.arg("--db").arg(&db).arg("init");
        cmd.assert().success();
        Sandbox { dir, db }
    }

    /// Sandbox without initializing — caller exercises init itself.
    pub fn raw() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("gavel.db");
        Sandbox { dir, db }
    }

    pub fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("gavel").unwrap();
        c.arg("--db").arg(&self.db);
        c.env_remove("GAVEL_DB");
        c
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Write `content` to a file named `name` inside the sandbox dir and
    /// return its path.
    pub fn write_file(&self, name: &str, content: &str) -> PathBuf {
        let p = self.dir.path().join(name);
        std::fs::write(&p, content).unwrap();
        p
    }

    /// Import the given JSON/JSONL content and return the ids of every
    /// item now in the database (in list order), by parsing `list --json`.
    pub fn import_and_list_ids(&self, content: &str) -> Vec<String> {
        let file = self.write_file("items.jsonl", content);
        self.cmd()
            .args(["import", file.to_str().unwrap()])
            .assert()
            .success();
        let out = self.cmd().args(["list", "--json"]).output().unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|it| it["id"].as_str().unwrap().to_string())
            .collect()
    }
}
