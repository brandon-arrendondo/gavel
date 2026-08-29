//! JSON interchange shapes for `gavel import` / `gavel export`.

use serde::{Deserialize, Serialize};

use crate::db::{LineComment, ReviewItem, Verdict};

/// One item as accepted by `gavel import`. A JSON array of these, or JSONL
/// (one object per line), both work.
#[derive(Debug, Clone, Deserialize)]
pub struct ImportItem {
    pub title: String,
    pub rule_id: String,
    #[serde(default)]
    pub rule_text: Option<String>,
    pub language: String,
    #[serde(default)]
    pub file_path: Option<String>,
    pub start_line: i64,
    pub code: String,
    #[serde(default)]
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerdictOut {
    pub decision: String,
    pub rationale: Option<String>,
    pub reviewer: Option<String>,
    pub reviewed_at: String,
}

impl From<Verdict> for VerdictOut {
    fn from(v: Verdict) -> Self {
        VerdictOut {
            decision: v.decision,
            rationale: v.rationale,
            reviewer: v.reviewer,
            reviewed_at: v.reviewed_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LineCommentOut {
    pub line_number: i64,
    pub comment: String,
    pub created_at: String,
}

impl From<LineComment> for LineCommentOut {
    fn from(c: LineComment) -> Self {
        LineCommentOut {
            line_number: c.line_number,
            comment: c.comment,
            created_at: c.created_at,
        }
    }
}

/// One item as emitted by `gavel export`.
#[derive(Debug, Clone, Serialize)]
pub struct ExportItem {
    pub id: String,
    pub title: String,
    pub rule_id: String,
    pub rule_text: Option<String>,
    pub language: String,
    pub file_path: Option<String>,
    pub start_line: i64,
    pub code: String,
    pub context: Option<String>,
    pub status: String,
    pub verdict: Option<VerdictOut>,
    pub line_comments: Vec<LineCommentOut>,
}

impl ExportItem {
    pub fn from_parts(
        item: ReviewItem,
        verdict: Option<Verdict>,
        comments: Vec<LineComment>,
    ) -> Self {
        ExportItem {
            id: item.id,
            title: item.title,
            rule_id: item.rule_id,
            rule_text: item.rule_text,
            language: item.language,
            file_path: item.file_path,
            start_line: item.start_line,
            code: item.code,
            context: item.context,
            status: item.status,
            verdict: verdict.map(VerdictOut::from),
            line_comments: comments.into_iter().map(LineCommentOut::from).collect(),
        }
    }
}
