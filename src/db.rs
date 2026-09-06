use std::path::Path;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;

use crate::error::{system, user, CliResult};

pub const SCHEMA_VERSION: i64 = 2;

const SCHEMA_SQL: &str = r#"
CREATE TABLE review_items (
    id          TEXT PRIMARY KEY,
    external_id TEXT,
    title       TEXT NOT NULL,
    rule_id     TEXT NOT NULL,
    rule_text   TEXT,
    language    TEXT NOT NULL,
    file_path   TEXT,
    start_line  INTEGER NOT NULL,
    code        TEXT NOT NULL,
    context     TEXT,
    status      TEXT NOT NULL CHECK(status IN ('pending','in_review','adjudicated')),
    created_at  TEXT NOT NULL
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

CREATE INDEX idx_review_items_status ON review_items(status, created_at);
CREATE INDEX idx_review_items_external_id ON review_items(external_id);
CREATE INDEX idx_line_comments_item ON line_comments(item_id, line_number);
"#;

#[derive(Debug, Clone, Serialize)]
pub struct ReviewItem {
    pub id: String,
    pub external_id: Option<String>,
    pub title: String,
    pub rule_id: String,
    pub rule_text: Option<String>,
    pub language: String,
    pub file_path: Option<String>,
    pub start_line: i64,
    pub code: String,
    pub context: Option<String>,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LineComment {
    pub id: String,
    pub item_id: String,
    pub line_number: i64,
    pub comment: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Verdict {
    pub item_id: String,
    pub decision: String,
    pub rationale: Option<String>,
    pub reviewer: Option<String>,
    pub reviewed_at: String,
}

pub const STATUSES: &[&str] = &["pending", "in_review", "adjudicated"];

/// The full decision vocabulary, in gavel's canonical order. `verdicts.decision`'s
/// CHECK constraint (above) is the source of truth this must stay in sync with.
pub const DECISIONS: &[&str] = &[
    "compliant",
    "violation",
    "false_positive",
    "needs_more_context",
    "uncertain",
];

const ALLOWED_DECISIONS_META_KEY: &str = "allowed_decisions";

pub fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn open(path: &Path) -> CliResult<Connection> {
    let conn = Connection::open(path)
        .map_err(|e| system(format!("cannot open database {}: {e}", path.display())))?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| system(format!("pragma journal_mode failed: {e}")))?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|e| system(format!("pragma foreign_keys failed: {e}")))?;
    if is_initialized(&conn) {
        migrate(&conn)?;
    }
    Ok(conn)
}

fn read_schema_version(conn: &Connection) -> CliResult<i64> {
    let v: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| system(format!("meta read failed: {e}")))?;
    match v {
        Some(s) => s
            .parse()
            .map_err(|e| system(format!("schema_version parse failed: {e}"))),
        None => Ok(1),
    }
}

fn migrate(conn: &Connection) -> CliResult<()> {
    let current = read_schema_version(conn)?;
    if current == SCHEMA_VERSION {
        return Ok(());
    }
    if current > SCHEMA_VERSION {
        return Err(system(format!(
            "database schema version {current} is newer than this binary supports ({SCHEMA_VERSION}); upgrade gavel"
        )));
    }
    if current <= 1 {
        migrate_v1_to_v2(conn)?;
    }
    Ok(())
}

/// Add the nullable `external_id` column so callers can round-trip their own
/// row key (e.g. a `ground_truth.gt_id`) through import -> export without
/// relying on import order surviving a review session. A plain
/// `ALTER TABLE ... ADD COLUMN` is sufficient here (unlike a CHECK-constraint
/// change) since SQLite supports adding nullable columns in place.
fn migrate_v1_to_v2(conn: &Connection) -> CliResult<()> {
    conn.execute_batch(
        r#"
        BEGIN;
        ALTER TABLE review_items ADD COLUMN external_id TEXT;
        CREATE INDEX IF NOT EXISTS idx_review_items_external_id ON review_items(external_id);
        UPDATE meta SET value = '2' WHERE key = 'schema_version';
        COMMIT;
        "#,
    )
    .map_err(|e| system(format!("v1->v2 migration failed: {e}")))?;
    Ok(())
}

pub fn create_schema(conn: &Connection) -> CliResult<()> {
    conn.execute_batch(SCHEMA_SQL)
        .map_err(|e| system(format!("schema create failed: {e}")))?;
    conn.execute(
        "INSERT INTO meta(key, value) VALUES('schema_version', ?1)",
        params![SCHEMA_VERSION.to_string()],
    )
    .map_err(|e| system(format!("meta insert failed: {e}")))?;
    Ok(())
}

pub fn is_initialized(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name='review_items'",
        [],
        |_| Ok(()),
    )
    .optional()
    .ok()
    .flatten()
    .is_some()
}

pub fn require_initialized(conn: &Connection) -> CliResult<()> {
    if !is_initialized(conn) {
        return Err(user("database is not initialized; run `gavel init` first"));
    }
    Ok(())
}

pub const ITEM_COLUMNS: &str =
    "id, external_id, title, rule_id, rule_text, language, file_path, start_line, code, context, status, created_at";

pub fn row_to_item(row: &Row) -> rusqlite::Result<ReviewItem> {
    Ok(ReviewItem {
        id: row.get(0)?,
        external_id: row.get(1)?,
        title: row.get(2)?,
        rule_id: row.get(3)?,
        rule_text: row.get(4)?,
        language: row.get(5)?,
        file_path: row.get(6)?,
        start_line: row.get(7)?,
        code: row.get(8)?,
        context: row.get(9)?,
        status: row.get(10)?,
        created_at: row.get(11)?,
    })
}

pub fn insert_item(conn: &Connection, item: &ReviewItem) -> CliResult<()> {
    conn.execute(
        &format!(
            "INSERT INTO review_items({ITEM_COLUMNS}) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"
        ),
        params![
            item.id,
            item.external_id,
            item.title,
            item.rule_id,
            item.rule_text,
            item.language,
            item.file_path,
            item.start_line,
            item.code,
            item.context,
            item.status,
            item.created_at,
        ],
    )
    .map_err(|e| system(format!("insert review_items failed: {e}")))?;
    Ok(())
}

/// Resolve a user-supplied `<ID>` argument to exactly one item. Accepts a
/// full uuid or an unambiguous prefix of one (the "short id" shown by
/// `list`).
pub fn resolve_one(conn: &Connection, raw: &str) -> CliResult<ReviewItem> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {ITEM_COLUMNS} FROM review_items WHERE id = ?1 OR id LIKE ?2 ORDER BY id"
        ))
        .map_err(|e| system(format!("prepare failed: {e}")))?;
    let like = format!("{raw}%");
    let rows = stmt
        .query_map(params![raw, like], row_to_item)
        .map_err(|e| system(format!("query failed: {e}")))?;
    let mut matches = Vec::new();
    for r in rows {
        matches.push(r.map_err(|e| system(format!("row read failed: {e}")))?);
    }
    drop(stmt);
    match matches.len() {
        0 => Err(user(format!("item {raw} not found"))),
        1 => Ok(matches.into_iter().next().unwrap()),
        n => {
            let mut msg = format!("item id '{raw}' is ambiguous ({n} matches):\n");
            for it in &matches {
                msg.push_str(&format!("  {} {}\n", &it.id[..8], it.title));
            }
            Err(user(msg))
        }
    }
}

pub fn list_items(conn: &Connection, status: Option<&str>) -> CliResult<Vec<ReviewItem>> {
    let sql = match status {
        Some(_) => {
            format!("SELECT {ITEM_COLUMNS} FROM review_items WHERE status = ?1 ORDER BY created_at")
        }
        None => format!("SELECT {ITEM_COLUMNS} FROM review_items ORDER BY created_at"),
    };
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| system(format!("prepare failed: {e}")))?;
    let rows = if let Some(s) = status {
        stmt.query_map(params![s], row_to_item)
    } else {
        stmt.query_map([], row_to_item)
    }
    .map_err(|e| system(format!("query failed: {e}")))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| system(format!("row read failed: {e}")))?);
    }
    Ok(out)
}

pub fn set_status(conn: &Connection, item_id: &str, status: &str) -> CliResult<()> {
    conn.execute(
        "UPDATE review_items SET status = ?1 WHERE id = ?2",
        params![status, item_id],
    )
    .map_err(|e| system(format!("update status failed: {e}")))?;
    Ok(())
}

pub fn load_comments(conn: &Connection, item_id: &str) -> CliResult<Vec<LineComment>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, item_id, line_number, comment, created_at FROM line_comments \
             WHERE item_id = ?1 ORDER BY line_number, created_at",
        )
        .map_err(|e| system(format!("prepare failed: {e}")))?;
    let rows = stmt
        .query_map(params![item_id], |r| {
            Ok(LineComment {
                id: r.get(0)?,
                item_id: r.get(1)?,
                line_number: r.get(2)?,
                comment: r.get(3)?,
                created_at: r.get(4)?,
            })
        })
        .map_err(|e| system(format!("query failed: {e}")))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| system(format!("row read failed: {e}")))?);
    }
    Ok(out)
}

pub fn add_comment(
    conn: &Connection,
    item_id: &str,
    line_number: i64,
    comment: &str,
) -> CliResult<()> {
    conn.execute(
        "INSERT INTO line_comments(id, item_id, line_number, comment, created_at) VALUES(?1,?2,?3,?4,?5)",
        params![
            uuid::Uuid::new_v4().to_string(),
            item_id,
            line_number,
            comment,
            now_iso(),
        ],
    )
    .map_err(|e| system(format!("insert line_comments failed: {e}")))?;
    Ok(())
}

pub fn load_verdict(conn: &Connection, item_id: &str) -> CliResult<Option<Verdict>> {
    conn.query_row(
        "SELECT item_id, decision, rationale, reviewer, reviewed_at FROM verdicts WHERE item_id = ?1",
        params![item_id],
        |r| {
            Ok(Verdict {
                item_id: r.get(0)?,
                decision: r.get(1)?,
                rationale: r.get(2)?,
                reviewer: r.get(3)?,
                reviewed_at: r.get(4)?,
            })
        },
    )
    .optional()
    .map_err(|e| system(format!("query failed: {e}")))
}

/// Insert or replace the verdict for an item (one verdict per item; a
/// re-review overwrites the previous one), then mark the item adjudicated.
pub fn upsert_verdict(
    conn: &Connection,
    item_id: &str,
    decision: &str,
    rationale: Option<&str>,
    reviewer: Option<&str>,
) -> CliResult<()> {
    conn.execute(
        "INSERT INTO verdicts(item_id, decision, rationale, reviewer, reviewed_at) VALUES(?1,?2,?3,?4,?5)
         ON CONFLICT(item_id) DO UPDATE SET decision = excluded.decision, rationale = excluded.rationale,
             reviewer = excluded.reviewer, reviewed_at = excluded.reviewed_at",
        params![item_id, decision, rationale, reviewer, now_iso()],
    )
    .map_err(|e| system(format!("upsert verdicts failed: {e}")))?;
    set_status(conn, item_id, "adjudicated")?;
    Ok(())
}

pub fn count_by_status(conn: &Connection) -> CliResult<Vec<(String, i64)>> {
    let mut stmt = conn
        .prepare("SELECT status, COUNT(*) FROM review_items GROUP BY status ORDER BY status")
        .map_err(|e| system(format!("prepare failed: {e}")))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .map_err(|e| system(format!("query failed: {e}")))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| system(format!("row read failed: {e}")))?);
    }
    Ok(out)
}

pub fn get_meta(conn: &Connection, key: &str) -> CliResult<Option<String>> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| {
        r.get(0)
    })
    .optional()
    .map_err(|e| system(format!("meta read failed: {e}")))
}

pub fn set_meta(conn: &Connection, key: &str, value: &str) -> CliResult<()> {
    conn.execute(
        "INSERT INTO meta(key, value) VALUES(?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(|e| system(format!("meta write failed: {e}")))?;
    Ok(())
}

/// Parse and validate a `--decisions` flag value (comma-separated, e.g.
/// `"violation,false_positive,uncertain"`) against `DECISIONS`. Order is
/// preserved as given — `review`'s TUI assigns numbered keys 1..N in this
/// order, so the caller controls which key maps to which decision. Rejects
/// unknown values and an empty list.
pub fn parse_allowed_decisions(raw: &str) -> CliResult<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for part in raw.split(',') {
        let d = part.trim();
        if d.is_empty() {
            continue;
        }
        if !DECISIONS.contains(&d) {
            return Err(user(format!(
                "unknown decision '{d}' in --decisions; must be one of: {}",
                DECISIONS.join(", ")
            )));
        }
        if !out.iter().any(|existing| existing == d) {
            out.push(d.to_string());
        }
    }
    if out.is_empty() {
        return Err(user("--decisions must name at least one decision"));
    }
    Ok(out)
}

/// The decision subset `review`'s TUI should offer, or `None` for the full
/// vocabulary (the default when no import has ever set one).
pub fn get_allowed_decisions(conn: &Connection) -> CliResult<Option<Vec<String>>> {
    let raw = get_meta(conn, ALLOWED_DECISIONS_META_KEY)?;
    Ok(raw.map(|s| s.split(',').map(str::to_string).collect()))
}

/// Persist the decision subset `review` should offer from now on, until a
/// later import overwrites it. Does not affect items already adjudicated
/// under a wider vocabulary — `verdicts.decision` still accepts any of
/// `DECISIONS` regardless of this setting.
pub fn set_allowed_decisions(conn: &Connection, decisions: &[String]) -> CliResult<()> {
    set_meta(conn, ALLOWED_DECISIONS_META_KEY, &decisions.join(","))
}

pub fn count_by_decision(conn: &Connection) -> CliResult<Vec<(String, i64)>> {
    let mut stmt = conn
        .prepare("SELECT decision, COUNT(*) FROM verdicts GROUP BY decision ORDER BY decision")
        .map_err(|e| system(format!("prepare failed: {e}")))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .map_err(|e| system(format!("query failed: {e}")))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| system(format!("row read failed: {e}")))?);
    }
    Ok(out)
}
