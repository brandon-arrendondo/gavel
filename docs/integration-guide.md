# Integration guide

For an agent or other automated caller driving `gavel` as the human-review
step in a larger pipeline — either by shelling out to the `gavel` binary
directly, or through `mcp_server/server.py`.

## The basic loop

1. **Import** a batch of flagged findings as JSON or JSONL:
   `gavel import findings.jsonl` (or `import_items([...])` over MCP).
2. **Wait** for a human to work through them with `gavel review`. Poll
   `gavel stats` or `gavel list --status pending` / `--status in_review` to
   see progress; there is no push/webhook mechanism, only pull.
3. **Export** adjudicated verdicts once ready:
   `gavel export --status adjudicated -o verdicts.json` (or
   `export_items(status="adjudicated")` over MCP).

Every command opens its own SQLite connection and exits — there is no
server process to keep running, and no state kept anywhere but the
database file itself.

## Carrying your own id through: `external_id`

If your system already has its own identity for each finding (a row in
your own tracking table, an issue number, anything), set `external_id` on
import:

```json
{"external_id": "gt-42", "title": "...", "rule_id": "ARR30-C", "language": "c", "start_line": 42, "code": "..."}
```

`external_id` is optional, opaque to gavel (never validated, never used
for lookups), and echoed back verbatim on `show` and `export` — `null` if
the item was imported without one.

This matters because **gavel's own id (a uuid) is not predictable from
import order**: items in a review queue can be opened and adjudicated in
any order, and `gavel export` returns items in the order `list_items`
does, not necessarily the order you imported them in. If you need to
match an exported verdict back to a specific row in your own system,
don't rely on import order — read `external_id` back off the export and
join on that.

If you don't need this, just omit the field; every downstream JSON shape
still has it, as `null`.

## Mapping gavel's decision vocabulary to your own

`gavel`'s `verdict.decision` is one of `compliant`, `violation`,
`false_positive`, `needs_more_context`, `uncertain`. If your own system
uses a narrower vocabulary, map onto the subset that applies and simply
don't produce the values you don't need. See
[`data-model.md`](data-model.md#decision-vocabulary-and-common-consumer-mappings)
for one observed mapping (a `TP`/`FP`/`uncertain` ground-truth workflow).

Don't stop at mapping on the way *out*, though — also narrow what
`gavel review` offers on the way *in*, with `gavel import --decisions
<LIST>`:

```
gavel import findings.jsonl --decisions violation,false_positive,uncertain
```

Without this, a reviewer can (and, in practice, will) pick `compliant` or
`needs_more_context` meaning "no issue here" when your mapping only has
`false_positive` for that — the export then has a decision your import
script has no mapping for, which it's right to skip rather than guess at,
but every skip is a wasted round-trip. `--decisions` fixes this at the
source: it restricts the TUI's numbered decision keys to exactly the values
in your mapping, in the order given, so a value you have no use for is never
offered as a button in the first place. It persists for the whole database
(`meta.allowed_decisions`) until a later import passes a different list, so
you only need to pass it once per review session.

## What gavel does *not* give you: no reference verdict during review

If your use case involves comparing a fresh human verdict against an
existing one you already have (e.g. measuring agreement between an AI
adjudicator and a human, or between two independent human reviewers),
**do not attempt to store your existing verdict anywhere gavel would
render it during review.** There is no schema field for this, and that's
intentional — see
[`architecture.md`](architecture.md#blind-review-is-a-design-guarantee).
Keep your reference verdicts in your own system, joined by `external_id`
after export, never fed into gavel's database before the human reviews
the item.

## Providing a code excerpt

`code` is required on import — gavel does not read your source tree
itself, so if your findings are currently just `file_path:line` pointers
without an embedded snippet, you'll need a preprocessing step that slices
out a window of source around the flagged line before importing. How much
context to include (leading/trailing lines) is a judgment call for your
own pipeline; gavel just displays whatever you send as `code`, with real
line numbers computed from `start_line`.

## Programmatic import without hand-writing JSONL

If you're driving `gavel` through `mcp_server/server.py`, `import_items`
already accepts a plain list of Python dicts — you don't need to write a
JSONL file yourself; the server does that under the hood and cleans up
the temp file afterward. If you're shelling out to the binary directly
instead, `gavel import` accepts either a JSON array or JSONL, whichever is
more convenient to produce.

## Quick reference: JSON shapes

Import item (only `title`, `rule_id`, `language`, `start_line`, and `code`
are required):

```json
{
  "external_id": "gt-42",
  "title": "Pointer arithmetic past array bound in checksum loop",
  "rule_id": "ARR30-C",
  "rule_text": "Do not form or use out-of-bounds pointers or array subscripts",
  "language": "c",
  "file_path": "src/checksum.c",
  "start_line": 42,
  "code": "uint8_t compute_checksum(const uint8_t *buf, size_t len) {\n    ...\n}",
  "context": "Why this was flagged, what to focus on."
}
```

Exported item (every import field, plus):

```json
{
  "id": "…uuid…",
  "status": "adjudicated",
  "verdict": {
    "decision": "violation",
    "rationale": "confirmed OOB read on malformed input",
    "reviewer": "brandon",
    "reviewed_at": "2026-08-29T23:00:00Z"
  },
  "line_comments": [
    {"line_number": 3, "comment": "this is the actual OOB access", "created_at": "2026-08-29T22:55:00Z"}
  ]
}
```

`verdict` is `null` until adjudicated; `line_comments` is `[]` if none
were left.
