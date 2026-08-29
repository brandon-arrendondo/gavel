# gavel

A terminal review workstation for adjudicating CERT-C secure-coding findings
that an AI agent (Claude) flagged in C/C++ source.

The workflow: an agent statically reviews a codebase against CERT-C rules and
produces a batch of flagged snippets as JSON. A human imports that batch into
`gavel`, works through each item in a terminal UI — confirming a violation,
marking a false positive, or flagging "needs more context" — and exports the
adjudicated verdicts back out as JSON for the agent to consume.

Not a security scanner. `gavel` has no static analysis of its own; it is
purely the human-in-the-loop review step for output some other tool (or
agent) already produced.

## Install

```sh
cargo build --release
# binary at target/release/gavel
```

## Quick start

```sh
gavel init
gavel import examples/sample-items.jsonl
gavel list
gavel review
gavel export --status adjudicated -o verdicts.json
```

## Commands

| Command | Purpose |
|---|---|
| `gavel init [--dir PATH]` | Create `.gavel/gavel.db` in the target directory (cwd by default) |
| `gavel import <FILE>` | Load review items from a JSON array or JSONL file |
| `gavel list [--status S]` | Table of id / rule / title / status |
| `gavel show <ID>` | Full detail for one item — snippet, rule, context, comments, verdict |
| `gavel review [--id ID] [--reviewer NAME]` | Launch the TUI |
| `gavel export [--status S] [-o FILE]` | Dump items (with verdicts + comments) as JSON |
| `gavel stats` | Counts by status and by verdict decision |

Every command accepts `--db PATH` to point at a specific database file,
overriding the usual lookup (`--db` flag → `$GAVEL_DB` → walk up from cwd
looking for `.gavel/gavel.db` → error).

Full reference: `man gavel` (see `man/gavel.1`).

## Import / export JSON shape

Import accepts a JSON array or JSONL (one object per line) of:

```json
{
  "title": "Pointer arithmetic past array bound in checksum loop",
  "rule_id": "ARR30-C",
  "rule_text": "Do not form or use out-of-bounds pointers or array subscripts",
  "language": "c",
  "file_path": "src/checksum.c",
  "start_line": 42,
  "code": "uint8_t compute_checksum(const uint8_t *buf, size_t len) {\n    ...\n}",
  "context": "Why this was flagged, what to focus on. Freeform markdown, optional."
}
```

`rule_text`, `file_path`, and `context` are optional. Every imported item
starts life with status `pending`.

Export produces a JSON array where each item additionally carries its
`status`, `verdict` (`{decision, rationale, reviewer, reviewed_at}` or
`null`), and `line_comments` (`[{line_number, comment, created_at}, ...]`).
By default only `adjudicated` items are exported — pass `--status all` (or
`pending` / `in_review`) to see others.

## The review TUI

`gavel review` works through every `pending`/`in_review` item as a queue (in
import order), or opens a single item directly with `--id`.

Layout: header (title, rule, progress) — code snippet with real line numbers
on the left, rule text + context on the right — line comments and an input
line along the bottom — a keybinding bar at the very bottom. No syntax
highlighting; this is a plain-monospace MVP built for working through a
stack of ~150 snippets quickly, not for pretty rendering.

Keys:

- `j` / `k` (or arrows) — move the selected line in the code pane
- `c` — add a comment on the selected line (type text, `Enter` to save, `Esc` to cancel)
- `1` compliant · `2` violation · `3` false_positive · `4` needs_more_context · `5` uncertain
  — sets the verdict, then prompts for an optional rationale (`Enter` saves
  and auto-advances to the next item; `Esc` cancels and stays put)
- `n` — skip to the next item without a verdict
- `p` — go back to the previous item
- `PageUp` / `PageDown` — scroll the rule/context pane
- `q` — quit

Comments and verdicts are written to the database as soon as they're
entered — there's no separate "save" step and no unsaved-draft state to
lose on quit. An item left without a verdict just stays `in_review` for the
next session.

## MCP server

`mcp_server/` is a thin Python MCP server (FastMCP) that shells out to the
`gavel` binary, for driving import/list/show/export/stats from an agent
directly instead of through a human-typed CLI. See `mcp_server/server.py`.

```sh
cd mcp_server
pip install -e .
gavel-mcp-server
```

Resolves the `gavel` binary via `$GAVEL_BIN` (falls back to `gavel` on
`PATH`) and the database the same way the CLI does (`$GAVEL_DB`, or a
`.gavel/gavel.db` walked up from wherever the server process's cwd is).

## Development

```sh
invoke build          # cargo build
invoke test           # cargo test
invoke lint           # cargo clippy -D warnings
invoke fmt             # cargo fmt --all
invoke check           # pre-commit run --all-files
```

See `CLAUDE.md` for repository conventions and data-model details aimed at
a future Claude session working in this repo.

## License

MIT — see `LICENSE`.
