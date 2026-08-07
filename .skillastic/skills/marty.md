---
name: marty
description: >-
  Use Marty, ViCo's filesystem scout, to explore directories, discover relevant
  existing projects, and gather source context before a build task. Marty is
  backed by the `marty` binary at `~/.local/bin/marty`; the ViCo tool wrapper
  shells out to the binary for a subset of commands.
---

# Marty

Marty is a command-line companion that learns your habits to make directory
navigation faster. Exposed in two ways:

1. **Standalone CLI** at `~/.local/bin/marty` — full commands:
   `scout`, `visit`, `hotspots`, `beliefs`, `tag`, `trace`, `tui`, `server`.
2. **ViCo tool** (`tool_registry/builtins/marty.rs`) — wrapper that shells out
   to the binary for `scout`, `hotspots`, `beliefs`, and `trace` only. CLI JSON
   output is returned in the tool result's `stdout` field.

Use Marty when the target directory is unknown or ambiguous, when user
navigation habits should influence the choice, or when you need a tree plus
source snapshot in one call.

## Quick decision flow

```
Need a project tree + source snapshot?  → marty scout <path>
Need to choose a directory?
  ├─ user just worked on it?             → marty trace
  ├─ match by habit/energy?              → marty hotspots
  └─ match by tag/nickname?              → marty beliefs
Exact file path already known?           → use Read or bound directly
```

## When to call Marty

Call before expensive build planning when:

- The user mentions a task but not a concrete directory.
- Multiple project roots could match the request.
- Navigation history (`hotspots`, `trace`) should influence the choice.
- You need `project_type`, a directory tree, and a source snapshot in one call.

**Do not call Marty** when the user already gave an exact file path (use
`Read` directly), you only need a single known file, or you are already editing
inside a known working directory.

## Prefer Marty over bart/bound directly

| Situation | Preferred tool | Why |
|-----------|----------------|-----|
| Target directory unknown or ambiguous | `marty` | Combines discovery with habit ranking |
| Ranked list of where user works | `marty hotspots` | Learns from `marty visit` history |
| Recent navigation history | `marty trace` | Cheaper than re-scanning the filesystem |
| Tree + source snapshot of one root | `marty scout` | Single call; wraps `bound` |
| Exact path known; only need files | `bound` directly | Avoids Marty overhead |
| Global filename search | `bart` directly | Lower-level; no habit data needed |
| Single file content | `Read` | Fastest |

## Task selection heuristics

### `scout` — Inspect one project root

Use when you have a candidate directory and need `project_type` detection, a
tree, and a bundled source snapshot (`bound`) for relevant files.

**Default ViCo call:**

```text
marty::{"task": "scout", "path": "<dir>", "depth": 3, "token_limit": 8000}
```

- `path` accepts `~` and is expanded. Omit only if the current open project
  root is correct; use `"reference": true` to prefer the reference root.
- `depth` default is `3`. Increase for deeply nested projects; decrease to save
  tokens when the tree is large.
- `token_limit` default is `8000` **per file**, not total. Reduce when you only
  need filenames or a small context window.
- If only a project name is given, run `hotspots` or `beliefs` first, then
  `scout` the best match.

### `hotspots` — Rank habit-weighted directories

Use when no explicit path is given, you need to break ties, or you want to
align with where the user spends time.

```text
marty::{"task": "hotspots"}
```

Interpret as an energy-ranked list. Higher `energy` = more visited and more
recently active. Prefer the top entry whose name/type matches the task. If
empty or irrelevant, fall back to `beliefs` or ask the user.

### `beliefs` — Directory relationships and tags

Use when the user refers to a project by nickname, tag, or relationship, or
when `hotspots` did not surface the right root.

```text
marty::{"task": "beliefs"}
```

### `trace` — Recent navigation

Use when the task likely relates to something the user just opened, or when you
need temporal context (last worked-on directory).

```text
marty::{"task": "trace"}
```

## Path and hotspot reasoning workflow

1. **Extract explicit path** from the prompt.
   - Present and valid → `scout` directly.
   - Ambiguous (e.g., just a project name) → confirm with `hotspots`/`beliefs`.
2. **No explicit path** → `marty hotspots`.
   - Match top entries against task keywords (language, framework, project name).
   - Clear match → `scout` that root.
   - Unclear → `marty beliefs` for tags/relationships.
3. **Still ambiguous** → `scout` the most likely root and state the assumption.
4. **After selecting a root** → `marty visit /path/to/project` so future hotspot
   rankings learn from it.

## ViCo tool interface

The tool accepts JSON and invokes the matching `marty` subcommand with `--json`.
The raw CLI JSON is returned inside `result.stdout`.

```text
marty::{"task": "scout", "path": "<dir>", "depth": 3, "token_limit": 8000}
  → runs: marty scout --json --depth 3 --token-limit 8000 <dir>
  → stdout: { path, project_type, snapshot, tree }

marty::{"task": "hotspots"}
  → runs: marty hotspots --json
  → stdout: [ { path, energy, last_ts }, ... ]

marty::{"task": "beliefs"}
  → runs: marty beliefs --json
  → stdout: { "<path>": { tags?: [...], related?: [...], ... }, ... }

marty::{"task": "trace"}
  → runs: marty trace --json
  → stdout: [ { "Visit": { path, ts } }, ... ]
```

**Tool schema notes:**

- `task` is required. Supported values: `scout`, `hotspots`, `beliefs`, `trace`.
- `path` accepts `~` and is expanded. Required for `scout` if no project root is
  open; ignored otherwise.
- `reference` (boolean, `scout` only): if `true` and no `path` is given, use the
  reference project root instead of the active one.
- `depth` applies only to `scout`. Default: `3`.
- `token_limit` applies only to `scout`. Default: `8000` per file.
- `visit`, `tag`, `tui`, and `server` are **not** exposed through the ViCo tool.

## CLI commands

Global options:

```text
  -s, --state <PATH>   Persisted state file (default: ~/.marty/state.json)
  -h, --help           Print help
  -V, --version        Print version
```

| Command | Purpose | Key flags |
|---------|---------|-----------|
| `scout <PATH>` | Tree + source snapshot | `--json`, `-d/--depth`, `-t/--token-limit` |
| `hotspots` | Top N hotspots | `--json`, `-t/--top` (default 5) |
| `beliefs` | Directory relationships | `--json` |
| `trace` | Recent visits | `--json`, `-l/--last` (default 10) |
| `tag <PATH> <TAG>` | Tag a directory | — |
| `visit <PATH>` | Reinforce a hotspot | — |
| `tui` | Interactive UI | — |
| `server` | HTTP dashboard | — |

**Note:** `-t/--top` for `hotspots` and `-l/--last` for `trace` are CLI-only
options and are **not** passed through the ViCo tool wrapper.

### JSON schemas

`scout`:

```json
{
  "path": "/path/to/project",
  "project_type": "rust",
  "snapshot": "<bundled source text with per-file metadata>",
  "tree": "<JSON-stringified directory tree>"
}
```

- `tree` is a JSON string, not a nested object. Parse it with
  `JSON.parse(result.stdout.tree)` if you need the structure.
- `snapshot` is produced by `bound`; it contains file metadata plus content and
  is capped per file by `token_limit`.

`hotspots`:

```json
[
  { "path": "/home/user/work/my-api", "energy": 7.0, "last_ts": 1782845079 }
]
```

`trace`:

```json
[
  { "Visit": { "path": "/home/user/work/my-api", "ts": 1782845079 } }
]
```

`beliefs`:

Object mapping paths to belief data. When empty: `{}`. May include `tags`,
`related`, and inferred relationships.

## Error handling

- **Missing path / no open project:** provide an explicit `path` or open a
  project root first.
- **Empty `hotspots` / `trace`:** no learned navigation yet; ask for a path or
  scan candidates with `scout`.
- **Marty timeout (25s):** reduce `depth` or `token_limit`; for huge trees use
  `bound` directly with a narrow filter.
- **Non-zero exit / stderr:** read before retrying; common causes are missing or
  unreadable paths.
- **Malformed JSON:** `tree` is a JSON string and needs an extra `JSON.parse`.

## Agent workflows

### Discover and rank candidates

For open-ended requests like "add auth to my Rust project":

```text
marty::{"task": "hotspots"}
marty::{"task": "trace"}
```

Cross-reference with keywords (e.g., `Cargo.toml`, `.rs`, project name), then
shortlist the top 2–3 roots that match.

### Scout a root before planning

```text
marty::{"task": "scout", "path": "~/work/my-api", "depth": 3, "token_limit": 8000}
```

Feed `project_type`, `tree`, and `snapshot` into the planner so the plan
references real files and conventions.

### Reinforce after landing on a root

```bash
marty visit /path/to/project
```

Call explicitly when you discover a root during manual exploration. ViCo also
records visits automatically on project open.

### Tag projects for thematic lookup

```bash
marty tag /path/to/project rust-backend
marty tag /path/to/project active-2025
```

Prefer stable-but-specific tags. Avoid one-off tags.

## Anti-patterns

- **Calling `scout` on `~`:** the home directory is too large; narrow to a
  project root first using `hotspots`/`beliefs`.
- **Ignoring empty hotspots:** if energy data is missing, ask rather than guess.
- **Using `token_limit` as a total budget:** it is per-file. A huge tree can
  still exceed your context window.
- **Parsing `tree` as a nested object:** it is a JSON string.
- **Forgetting `visit`:** scouting without visiting leaves habit ranking stale.
- **Mixing CLI-only flags into the ViCo tool:** `--top` and `--last` are not
  forwarded by the wrapper; call the CLI directly if you need them.

## Safety

- Marty skips hidden/build directories (`node_modules`, `target`, `.git`,
  `.cache`, etc.) when scanning.
- `bart` results and `bound` snapshots are cached server-side to avoid repeated
  expensive scans; temp snapshot files are deleted after use.
- `server` is an explicit subcommand; the binary does not start an HTTP server
  for any other command.
- State is persisted to `~/.marty/state.json` by default, written atomically
  with `0o600` permissions and capped (100 hotspots, 500 trace entries, 1000
  beliefs).
