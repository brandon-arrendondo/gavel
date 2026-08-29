# Architecture

`gavel` is a small Rust CLI plus a Python MCP wrapper around it. There is no
server process, no daemon, and no network I/O anywhere in the binary —
everything is a single SQLite file on disk, opened fresh by each invocation.

## Process shape

```
agent (Claude, or another tool) ──JSON──▶ gavel import ──▶ SQLite db
                                                              │
human ──terminal──▶ gavel review (TUI) ◀─────────────────────┘
                                                              │
agent ◀──JSON── gavel export ◀───────────────────────────────┘
```

`gavel` has no static-analysis logic of its own. It is purely the
human-in-the-loop adjudication step between "an agent flagged this" and "a
human confirmed or rejected it."

## Module map

| Path | Responsibility |
|------|----------------|
| `src/main.rs` | Entry point. `dispatch()` special-cases `Init` (it must run before database resolution, since it's what creates the database); every other command goes through `resolve::resolve_db_path` first, then `run_command`. |
| `src/cli.rs` | `clap` derive `Cli` / `Command` enum — the entire argument surface. Doc comments here become `--help` text and should stay in sync with `man/gavel.1`. |
| `src/resolve.rs` | Database path resolution: `--db` flag → `$GAVEL_DB` → walk up from cwd for `.gavel/gavel.db` → error. Mirrors the same pattern in `../todo-sqlite-cli`, simplified (no marker file — the DB's own directory name is the marker). |
| `src/db.rs` | The schema (`SCHEMA_SQL`), `SCHEMA_VERSION` and migration functions, the `ReviewItem` / `LineComment` / `Verdict` structs, and every SQL query in the codebase. All database access goes through this module — no other module writes raw SQL. |
| `src/error.rs` | `CliError::{User, System}` — no `anyhow`. `User` exits 1 (bad input, not found, missing db); `System` exits 2 (I/O/SQL failures indicating something is actually broken). |
| `src/model.rs` | The JSON interchange contract: `ImportItem` (what `import` accepts) and `ExportItem` (what `show`/`export` emit). This is the file an external integrator cares about most — it's the wire format. |
| `src/commands/*.rs` | One module per subcommand, each a thin `run()` that opens a connection, calls into `db::`, and formats output (text or `--json`). |
| `src/commands/review.rs` | The one command with real complexity — see below. |
| `mcp_server/server.py` | A FastMCP server that shells out to the compiled `gavel` binary and returns its stdout, for driving `gavel` from an agent instead of a human-typed terminal. It does no validation or transformation of its own; whatever `gavel --json` emits is what callers get back. |

## The review TUI

`src/commands/review.rs` is a single-file `ratatui` + `crossterm`
application — no separate `tui/` module, since the surface is small.

- `App` holds all in-memory session state. `Mode` (`Normal`, `Comment`,
  `Rationale(decision)`) gates which key handler is active.
- **Nothing is buffered for a later "save."** Every comment and verdict is
  written to SQLite synchronously, the moment `Enter` confirms it. There is
  no draft state that quitting (`q`) could lose — `q` only tears down the
  terminal.
- `goto()` is the single chokepoint for moving between items: it resets
  per-item UI state, reloads comments from the database, and flips
  `pending` → `in_review` the first time an item is opened
  (`ensure_in_review`).
- Advancing past the last item in the queue ends the session; it does not
  wrap back to the first item.

No syntax highlighting, no undo/redo — out of scope by design, not an
oversight. The TUI is built for working through a stack of ~150 flagged
snippets quickly, not for pretty rendering.

### Blind review is a design guarantee

The schema has no field for a "prior" or "reference" verdict on a
`review_item`, and the TUI has no code path that could render one. This is
intentional: at least one consumer (measuring whether a fresh human
verdict agrees with an existing one) depends on the reviewer never seeing
that existing verdict during review — showing it would make the agreement
measurement meaningless.

If a future feature needs to attach an existing verdict to an item (for
some other consumer's workflow), that field must default to **not**
rendering in the TUI — gate it behind an explicit opt-in, never show it
unconditionally. See the note in `CLAUDE.md`'s review-TUI section before
touching this.

## Error handling and exit codes

Every command returns `CliResult<T> = Result<T, CliError>`. `main.rs`
converts `Err` into a printed `error: ...` line on stderr and an exit code
(1 for `User`, 2 for `System`); there is no panic-based control flow in
`src/` (test code under `tests/` is exempt and may `unwrap()`/`expect()`
freely).

## Why no server/daemon

Each `gavel` invocation opens its own SQLite connection
(`journal_mode=WAL`, `foreign_keys=ON`) and closes it on exit. This keeps
the tool trivially embeddable — a script or agent can shell out to it
repeatedly without managing a long-lived process — at the cost of one
connection-open per command, which is not meaningful overhead for a
few-hundred-row review queue.
