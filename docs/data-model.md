# Data model

Three tables, one SQLite file, schema v2 (`db::SCHEMA_VERSION`). No ORM —
every query lives in `src/db.rs`.

## `review_items`

One row per flagged snippet.

| Column | Type | Notes |
|---|---|---|
| `id` | `TEXT` primary key | A uuid v4, generated on import. This is gavel's own identity for the item — used by `list`/`show`/`review --id` (a full uuid or an unambiguous prefix, resolved via `id LIKE 'prefix%'` in `db::resolve_one`). |
| `external_id` | `TEXT`, nullable | Caller-supplied id, opaque to gavel. Echoed back verbatim on export. Never used for id resolution — that's what `id` is for. Exists so a consumer can join an exported verdict back to its own row (e.g. a `ground_truth` table keyed by an integer row id) without depending on import order surviving a review session, since items can be reviewed out of order in the TUI. `null` if the item was imported without one. |
| `title` | `TEXT` not null | Short human-readable summary. |
| `rule_id` | `TEXT` not null | CERT-C rule identifier, e.g. `ARR30-C`. |
| `rule_text` | `TEXT`, nullable | Optional rule name/description. |
| `language` | `TEXT` not null | e.g. `c`, `cpp`. |
| `file_path` | `TEXT`, nullable | Original source location, for display only — not used to re-read the file. |
| `start_line` | `INTEGER` not null | The original file's line number that snippet line 1 corresponds to. Used only for display in `show`/`review`. |
| `code` | `TEXT` not null | The snippet itself, multi-line. |
| `context` | `TEXT`, nullable | Freeform markdown: why this was flagged, what to focus on. |
| `status` | `TEXT` not null, `CHECK IN ('pending','in_review','adjudicated')` | See lifecycle below. |
| `created_at` | `TEXT` not null | ISO 8601, set at import time. |

## `line_comments`

Many per item.

| Column | Type | Notes |
|---|---|---|
| `id` | `TEXT` primary key | uuid v4. |
| `item_id` | `TEXT` not null, `REFERENCES review_items(id) ON DELETE CASCADE` | |
| `line_number` | `INTEGER` not null | **1-indexed and relative to the snippet**, not the original file. To map to the original file's line number, add `start_line - 1` from the parent `review_items` row. |
| `comment` | `TEXT` not null | |
| `created_at` | `TEXT` not null | ISO 8601. |

## `verdicts`

At most one per item — `item_id` is itself the primary key.

| Column | Type | Notes |
|---|---|---|
| `item_id` | `TEXT` primary key, `REFERENCES review_items(id) ON DELETE CASCADE` | |
| `decision` | `TEXT` not null, `CHECK IN ('compliant','violation','false_positive','needs_more_context','uncertain')` | See vocab notes below. |
| `rationale` | `TEXT`, nullable | Freeform reviewer notes. |
| `reviewer` | `TEXT`, nullable | Defaults to `$USER` if not passed to `gavel review --reviewer`. |
| `reviewed_at` | `TEXT` not null | ISO 8601. |

`db::upsert_verdict` does `INSERT ... ON CONFLICT(item_id) DO UPDATE`, so
re-reviewing an already-adjudicated item overwrites the previous verdict
rather than erroring or duplicating. There is no verdict history — only
the latest one is kept.

## Status lifecycle

```
pending ──(opened in gavel review)──▶ in_review ──(verdict saved)──▶ adjudicated
```

Driven entirely by the `review` TUI:
- Every imported item starts `pending`.
- Opening an item in the TUI (even without saving anything) flips it to
  `in_review` — see `App::ensure_in_review` in `review.rs`.
- Saving a verdict flips it to `adjudicated`. Re-reviewing an adjudicated
  item (e.g. via `gavel review --id`) re-runs `upsert_verdict` and leaves
  status at `adjudicated`.

There is no path back from `adjudicated` or `in_review` to `pending` — an
item that was opened and abandoned mid-review simply stays `in_review`
until someone finishes it or a verdict is forced some other way.

## Decision vocabulary and common consumer mappings

`decision` is a 5-way vocabulary:
`compliant | violation | false_positive | needs_more_context | uncertain`.

Some consumers track a narrower vocabulary of their own and don't need
every value gavel supports. One observed mapping, from a consumer whose
own ground-truth table uses `TP | FP | uncertain`:

| Consumer value | gavel `decision` |
|---|---|
| `TP` | `violation` |
| `FP` | `false_positive` |
| `uncertain` | `uncertain` |
| *(unused)* | `compliant` |
| *(unused)* | `needs_more_context` |

There's no requirement that a consumer use all five values — `compliant`
and `needs_more_context` exist for workflows that distinguish "reviewed and
found fine" from "reviewer needs more information," which not every
consumer's own vocabulary needs to carry.

That gap is a real trap in practice, though: a reviewer looking at the TUI
has no way to tell that `compliant` or `needs_more_context` is a dead end for
the consumer that will eventually read the export, and it's easy to reach for
"this looks fine to me" (`compliant`) when the consumer actually wanted
`false_positive` for that meaning. `gavel import --decisions <LIST>` closes
that gap: it narrows the decisions `gavel review` offers to a comma-separated
subset (e.g. `--decisions violation,false_positive,uncertain`), persisted in
`meta.allowed_decisions` for the whole database until a later import passes a
different list. The TUI's numbered decision keys are then assigned `1..N` in
the order given, so a value that isn't in the list is never offered as a
button at all — see `review::decision_keys` in `src/commands/review.rs`. This
is enforced only at the TUI/import layer: `verdicts.decision`'s `CHECK`
constraint still accepts any of the five values regardless, so it has no
schema-version implications.

## Id resolution

`db::resolve_one` accepts either a full uuid or an unambiguous prefix
(`id LIKE 'prefix%'`). If a prefix matches more than one item, the command
fails with a list of the matches (their short id and title) rather than
guessing. This applies to `show <ID>`, `review --id <ID>`, and nowhere
else — `external_id` plays no part in this lookup.

## Schema versioning

`db::open` runs `migrate()` on every open of an already-initialized
database, comparing the stored `meta.schema_version` against
`db::SCHEMA_VERSION` and applying any `migrate_vN_to_vN1` functions in
between. Opening a database with a schema version *newer* than the binary
supports is a hard error (upgrade the binary), not a silent downgrade.

Schema v1 → v2 added the nullable `external_id` column via a plain
`ALTER TABLE ... ADD COLUMN` (sufficient for an additive nullable column —
no `CHECK`-constraint change was involved, so the copy-via-new-table dance
wasn't needed). A future migration that does need to touch a `CHECK`
constraint should follow `../todo-sqlite-cli`'s `migrate_vN_to_vN1` pattern
of copying into a new table instead of editing `SCHEMA_SQL` in place.
