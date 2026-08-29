# gavel — developer guide for Claude

`gavel` is a human review workstation for adjudicating CERT-C secure-coding
findings that an AI agent (Claude, typically) flagged in C/C++ source. An
agent produces a batch of ~150+ flagged snippets as JSON; a human imports
them, works through each one in a terminal UI, and exports the verdicts
back out as JSON for the agent to consume. `gavel` has no static-analysis
logic of its own — it is purely the human-in-the-loop step.

Modeled directly on `../todo-sqlite-cli` (clap derive CLI, rusqlite with
`bundled`, per-command modules, anyhow-free hand-rolled error type, serde
interchange, a Python MCP server wrapping the binary). Read that repo's
source if a convention here seems unclear — it's the closest analog and the
patterns were copied intentionally, not reinvented.

## Repository layout

| Path | Purpose |
|------|---------|
| `src/main.rs` | Entry point; dispatches `Init` before DB resolution (it creates the DB), everything else after |
| `src/cli.rs` | clap derive `Cli`/`Command` — the argument surface |
| `src/db.rs` | Schema (`SCHEMA_SQL`), `ReviewItem`/`LineComment`/`Verdict` structs, all rusqlite queries |
| `src/error.rs` | `CliError::{User,System}` — exit code 1 vs 2, no `anyhow` |
| `src/resolve.rs` | `--db` / `$GAVEL_DB` / walk-up-for-`.gavel/gavel.db` resolution, mirrors todo-sqlite-cli's marker-file resolver but simpler (fixed relative path, no marker file needed) |
| `src/model.rs` | `ImportItem` (import JSON shape) and `ExportItem` (export JSON shape) — the interchange contract with the agent side |
| `src/commands/*.rs` | One module per subcommand: `init`, `import`, `list`, `show`, `review`, `export`, `stats` |
| `src/commands/review.rs` | The ratatui + crossterm TUI — the only command with meaningful complexity |
| `mcp_server/` | Python FastMCP server wrapping the binary (`import_items`, `list_items`, `show_item`, `stats`, `export_items`) |
| `examples/sample-items.jsonl` | 4 realistic sample findings (ARR30-C, EXP34-C, MEM31-C, STR31-C) for trying the pipeline end to end |
| `man/gavel.1` | Man page — the canonical command reference; keep in sync with `cli.rs` doc comments |
| `docs/` | Deeper reference docs for external readers: `architecture.md` (module map, TUI structure, the blind-review guarantee), `data-model.md` (schema, status lifecycle, decision vocab), `integration-guide.md` (how a consumer/agent should use gavel end to end) |
| `tasks.py` | invoke tasks: `build`, `test`, `lint`, `fmt`, `check`, `bump-version` |

## Data model

Three tables. Schema v2 as of the `external_id` column (see
`db::SCHEMA_VERSION` and `db::migrate_v1_to_v2` in `src/db.rs`) — a plain
`ALTER TABLE ADD COLUMN` was sufficient there since it's an additive
nullable column, not a `CHECK`-constraint change. For a breaking change,
follow todo-sqlite-cli's `migrate_vN_to_vN1` pattern of copy-via-new-table
instead of editing `SCHEMA_SQL` in place for existing databases.

- **`review_items`** — one row per flagged snippet. `id` is a uuid v4
  (primary key — there is no separate integer display id like
  todo-sqlite-cli's `id`/`uuid` split; `list`/`show`/`review --id` accept
  either the full uuid or an unambiguous prefix via `id LIKE 'prefix%'` in
  `db::resolve_one`). `status` is `pending` → `in_review` → `adjudicated`,
  driven entirely by the `review` TUI (opening an item flips
  `pending`→`in_review`; saving a verdict flips to `adjudicated`). Also
  carries `external_id` (nullable) — a caller-supplied id echoed verbatim
  on export, never interpreted or used for lookups by gavel itself; that's
  what `id`/`resolve_one` are for.
- **`line_comments`** — many per item, `line_number` is 1-indexed and
  **relative to the snippet**, not the original file (`start_line` is the
  original file's line 1 offset, used only for display in `show`/`review`).
- **`verdicts`** — at most one per item (`item_id` is itself the primary
  key); `db::upsert_verdict` does an `INSERT ... ON CONFLICT DO UPDATE`, so
  re-reviewing an already-adjudicated item (e.g. via `review --id`)
  overwrites the previous verdict rather than erroring or duplicating.

## Conventions carried over from todo-sqlite-cli

- Every DB-touching command opens its own connection via `db::open`, which
  auto-sets `journal_mode=WAL` and `foreign_keys=ON` and refuses to open a
  database with a schema version newer than the binary supports.
- `CliError::User` → exit 1, `CliError::System` → exit 2. Use `user(...)`
  for anything caused by bad input/missing db/not-found; `system(...)` for
  I/O and SQL failures that indicate something is actually broken.
- `--json` is a **global** clap flag (works both before and after the
  subcommand — confirmed empirically, see git history / just try
  `gavel init --json`), but only `init`, `import`, `list`, `show`, and
  `stats` honor it. `export` always emits JSON (that's its whole purpose);
  `review` is a TUI and ignores it.
- No `anyhow` in `src/` — everything threads through `CliResult<T> =
  Result<T, CliError>`, same as todo-sqlite-cli. Test code (`tests/`) may
  use `unwrap()`/`expect()` freely.

## The review TUI (`src/commands/review.rs`)

Single-file, no separate `tui/` module — it's small enough. Key structural
points if you're extending it:

- `App` holds the whole in-memory session state; `Mode` (`Normal`,
  `Comment`, `Rationale(decision)`) gates key handling in
  `handle_{normal,comment,rationale}_key`.
- **Nothing is buffered for a later "save"** — `db::add_comment` and
  `db::upsert_verdict` are called synchronously the moment `Enter` confirms
  an input, so `q` never needs to persist anything; it just tears down the
  terminal. If you add new stateful TUI behavior, keep this property —
  don't introduce a draft state that can be lost.
- `goto()` is the single chokepoint for moving to a different item: it
  resets `selected_line`/`context_scroll`/`mode`/`input`, reloads comments
  from the DB, and calls `ensure_in_review` (which flips `pending` →
  `in_review` the first time an item is displayed). Advancing past the last
  item sets `should_quit` rather than wrapping.
- Terminal setup/teardown (`setup_terminal`/`teardown_terminal`) always run
  in `run()` regardless of how `run_app` exits (including on error) — don't
  add an early return elsewhere in `run()` that skips `teardown_terminal`,
  or a panic/error mid-review leaves the user's terminal in raw
  alternate-screen mode.
- No syntax highlighting, no undo/redo — intentionally out of scope (see
  the original build brief this repo was built from, if you're looking for
  it, it's not persisted anywhere in-repo — this file and the man page are
  the record of what "MVP" was scoped to mean).
- **Blind review is an intentional design guarantee, not an oversight.**
  The schema has no slot for a "prior verdict" or "reference verdict" on a
  `review_item` — this is deliberate. At least one downstream consumer
  (measuring agreement between a fresh human verdict and an existing
  ground-truth verdict) depends on gavel never surfacing that existing
  verdict during review; if it were visible, the measurement would be
  meaningless. If you ever add a field like that (e.g. to support a
  different consumer's workflow), it **must** default to not rendering in
  `review.rs` — gate it behind an explicit opt-in flag/command, never show
  it unconditionally in the TUI. Don't remove this note without checking
  whether that guarantee is still relied upon.

## Testing

`tests/` follows todo-sqlite-cli's `assert_cmd` + `tempfile` + a shared
`tests/common/mod.rs` `Sandbox` helper style. Add new integration tests as
new files under `tests/`, not inside `src/`. The TUI itself
(`commands::review`) is not covered by integration tests (raw-mode
terminal I/O isn't practical to drive from `assert_cmd`); its logic is kept
thin enough that the surrounding `db` functions it calls carry the real
test coverage.

## Adding a subcommand

1. Add the variant to `Command` in `src/cli.rs` with doc comments (these
   become the `--help` text).
2. Add a module under `src/commands/`, wire it into `src/commands/mod.rs`.
3. Add a match arm in `run_command` (or, if the command needs to run before
   DB resolution like `init` does, in `dispatch`) in `src/main.rs`.
4. Update `man/gavel.1` and `README.md`'s command table.
5. If the agent side would plausibly want to call it, add a matching tool
   to `mcp_server/server.py`.
