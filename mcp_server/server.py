"""MCP server wrapping gavel.

Database resolution (first match wins):
  1. GAVEL_DB environment variable
  2. Walk-up from cwd looking for a .gavel/gavel.db file
  3. Exit 1

Binary resolution:
  1. GAVEL_BIN environment variable
  2. `gavel` on PATH
"""

import json
import os
import subprocess
import tempfile
from typing import Any

from mcp.server.fastmcp import FastMCP

BIN = os.environ.get("GAVEL_BIN", "gavel")

mcp = FastMCP("gavel")


def _run(*args: str) -> str:
    """Run the CLI, raise RuntimeError on non-zero exit, return stdout."""
    result = subprocess.run([BIN, *args], capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or f"CLI exited {result.returncode}")
    return result.stdout.strip()


# ---------------------------------------------------------------------------
# Import
# ---------------------------------------------------------------------------


@mcp.tool()
def import_items(items: list[dict[str, Any]]) -> str:
    """Import a batch of review items. Returns a confirmation JSON string.

    Each item has the shape:
      {title, rule_id, rule_text?, language, file_path?, start_line, code, context?}

    - title: short human-readable summary of what's being reviewed
    - rule_id: CERT-C rule identifier, e.g. "EXP34-C"
    - rule_text: optional rule name/description
    - language: e.g. "c", "cpp"
    - file_path: optional original source location, for reference only
    - start_line: line number in the original file that snippet line 1 corresponds to
    - code: the snippet itself, multi-line
    - context: optional freeform markdown — why this was flagged, what to focus on

    Every imported item starts with status "pending".
    """
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".json", delete=False
    ) as f:
        json.dump(items, f)
        path = f.name
    try:
        out = _run("import", "--json", path)
    finally:
        os.unlink(path)
    return out


# ---------------------------------------------------------------------------
# Read
# ---------------------------------------------------------------------------


@mcp.tool()
def list_items(status: str | None = None) -> str:
    """List review items as JSON.

    status: filter by pending | in_review | adjudicated (omit for all).

    Returns {"items": [{"id", "title", "rule_id", "status"}, ...]}.
    """
    args = ["list", "--json"]
    if status:
        args += ["--status", status]
    return _run(*args)


@mcp.tool()
def show_item(id: str) -> str:
    """Show full detail for one item as JSON: snippet, rule, context,
    line comments, and verdict (or null if not yet adjudicated).

    id: full item id or an unambiguous short-id prefix (as shown by list_items).
    """
    return _run("show", id, "--json")


@mcp.tool()
def stats() -> str:
    """Return counts of items by status and of verdicts by decision, as JSON."""
    return _run("stats", "--json")


# ---------------------------------------------------------------------------
# Export
# ---------------------------------------------------------------------------


@mcp.tool()
def export_items(status: str = "adjudicated") -> str:
    """Export items as a JSON array, each carrying its verdict and line comments.

    status: pending | in_review | adjudicated | all (default: adjudicated,
        since that's what an agent typically wants back after a human review
        session).

    Returns the JSON array as a string (parse it to get a list of objects
    shaped {id, title, rule_id, rule_text, language, file_path, start_line,
    code, context, status, verdict, line_comments}).
    """
    return _run("export", "--status", status)


if __name__ == "__main__":
    mcp.run()
