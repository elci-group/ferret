---
name: bart
description: >-
  Use the `bart` temporal filesystem profiler to inspect directory trees,
  measure disk bloat, find heavy files, and clean disposable artefacts.
  Reach for bart whenever ViCo needs to understand project layout, count
  files, identify large binaries, or prepare a contextual tree view for an
  agent. Prefer `--json` output when the result will be parsed by code.
---

# bart

`bart` is a temporal filesystem profiler with interactive filtering, semantic
code grouping, and emoji-based terminal output. Use it to get human- or
machine-readable trees, find heavy files (`--top`, `--explain`), compare scans
(`--diff`), clean disposable directories (`--clean`, `--apply`), and score
bloat (`--insights`).

## Quick decision flow

Use this flow to pick the right command without over-thinking:

1. **First look at a project:** `bart -d 3 .`
2. **Machine-readable tree for code/agents:** `bart --json -d 3 -n 0 .`
3. **Find the biggest files globally:** `bart --top 20 .`
4. **Understand why one directory is large:** `bart --explain <dir>`
5. **Preview safe cleanup:** `bart --clean`
6. **Actually clean:** `bart --clean --apply` (only after reviewing the dry-run)
7. **Compare against a prior scan:** `bart --diff`

## When to call bart

Call bart before making assumptions about directory structure or size.

- **First look at a project:** `bart -d 3 .` to see the top-level layout.
- **Find what's consuming space:** `bart --explain <dir>` or `bart --top 20 .`.
- **Prepare a tree for another agent or context cache:**
  `bart --json -d 3 -n 0 .`.
- **Check whether a build/dependency directory is the bloat source:**
  `bart --explain ./target` or `bart --explain ./node_modules`.
- **Clean up safely:** `bart --clean` first, then `bart --clean --apply` only
  after reviewing the dry-run list.

## Decision heuristics

1. **Default to `bart -d 3 .`** for an initial survey.
   - Depth 1 is often too shallow; depth 3 shows project structure without
     drowning in leaves.
   - Increase depth (`-d 5` or `-d 10`) only when you need deeper detail.
2. **Default sort is `size`.** Keep it unless you specifically need alphabetical.
3. **Always prefer `--json` when code or another agent consumes the output.**
   Human-readable output includes emoji, progress spinners, and cached state;
   parse JSON instead.
4. **Use `--no-ignore` only when ignored directories are suspects.**
   Normally let bart respect `.gitignore`; it keeps noise out.
5. **Do not use `-w/--watch` or `-f/--filter`.** These launch interactive TUIs
   and will hang in an agent session. Filter JSON output with `jq` instead.
6. **Use `--insights` for a one-line bloat score and auto-generated tips.**
7. **Watch out for the `.toon` cache.** bart writes a `.toon` file in the
   scanned directory. If results look stale or `--no-ignore` output does not
   match expectations, delete `.toon` and re-run.

## Token and depth defaults

- **Overview:** `-d 3` balances breadth and detail.
- **Source dive:** `-d 5` is usually enough for nested source trees.
- **Deep search:** `-d 10` or higher only when hunting for specific deep files.
- **JSON exports:** always use `-n 0` to avoid silently dropping entries.
- **Human tree per-directory cap:** `-n 10` or `-n 20` keeps long directories
  readable; omit or use `0` when you need everything.

## Output format guidance

| Goal | Command |
|------|---------|
| Human-readable overview | `bart -d 3 .` |
| Machine-readable tree | `bart --json -d 3 -n 0 .` |
| Spreadsheet import | `bart --csv -d 3 .` |
| Find biggest files globally | `bart --top 20 .` |
| Understand why one directory is large | `bart --explain <dir>` |
| Get a bloat score + tips | `bart --insights .` |
| Compare against previous scan | `bart --diff` |
| Safe cleanup preview | `bart --clean` |
| Actually delete disposable heavyweights | `bart --clean --apply` |

## Important flags

| Flag | Meaning | Default |
|------|---------|---------|
| `-d, --depth <N>` | Maximum depth to display | `1` |
| `-n, --limit <N>` | Top N entries per directory; `0` for all | `0` |
| `-s, --sort <size\|name>` | Sort order | `size` |
| `-w, --watch` | Watch directory for live updates | — |
| `-f, --filter` | Launch interactive TUI to filter by file format and perform actions | — |
| `--no-ignore` | Include `.git`, `node_modules`, `target`, etc. | — |
| `--json` | Export the entire directory structure to JSON | — |
| `--csv` | Export the entire directory structure to CSV | — |
| `--diff` | Compare current directory against the previous scan | — |
| `--explain` | Deep semantic breakdown of why a directory is large | — |
| `--top <N>` | Show top N largest individual files globally | — |
| `--insights` | Calculate and display bloat score and auto-insights | — |
| `--clean` | Identify disposable heavyweights (dry-run) | — |
| `--apply` | Use with `--clean` to actually delete | — |

## Subcommands

Agents rarely need these; focus on the scan flags above.

| Command | Purpose |
|---------|---------|
| `index` | Manage tracked directories for the daemon |
| `daemon` | Start/status the background observability daemon |
| `help` | Print help for bart or a given subcommand |

## Common commands

```bash
# Pretty tree of current directory (depth 2)
bart -d 2

# JSON tree for programmatic consumption (unlimited entries)
bart --json -d 3 -n 0 .

# Explain why a directory is large
bart --explain ./src-tauri/target

# Show top 20 largest files anywhere under the path
bart --top 20 .

# Diff against the previous scan
bart --diff

# Calculate bloat score and auto-insights
bart --insights .

# Dry-run cleanup of known disposable directories
bart --clean

# Actually delete the directories identified by --clean
bart --clean --apply
```

## JSON schema

Every node in `--json` output has the same shape:

| Field | Type | Meaning |
|-------|------|---------|
| `path` | string | Absolute or relative path |
| `is_dir` | boolean | `true` if the entry is a directory |
| `size` | integer | Size in bytes |
| `file_count` | integer | Number of files under this node; a file has `1` |
| `children` | array | Child nodes (empty for files and empty directories) |
| `depth` | integer | Depth of the node relative to the scan root |
| `modified` | integer | Unix timestamp (seconds) of last modification |

```bash
# Find every directory larger than 50 MB
bart --json -d 5 -n 0 . | jq '.. | objects | select(.is_dir and .size > 52428800) | {path, size}'

# Cache a project tree for later agent context
mkdir -p .agents/bart_cache
bart --json -d 3 -n 0 . > .agents/bart_cache/$(basename $(pwd))-tree.json
```

## Agent workflows

### 1. Scout a new project

When you open a repository for the first time, get a compact overview and a
deep view of the source tree.

```bash
# Quick orientation: top-level layout
bart -d 2

# See the whole source tree, sorted by size
bart -d 4 -n 10 ./src

# Count files and find the heaviest directories
bart --insights .
```

Typical follow-ups:

- If a directory looks unexpectedly large, run `bart --explain <dir>`.
- If build artefacts dominate, run `bart --clean`.

### 2. Bloat analysis

Find what is eating disk space and why.

```bash
# Top 30 largest files under the project
bart --top 30 .

# Deep semantic breakdown of a suspicious directory
bart --explain ./dist

# Compare against the last scan to see what grew
bart --diff

# Get a bloat score plus auto-insights
bart --insights
```

### 3. Before/after cleanup

Always dry-run first, review the list, then apply.

```bash
# 1. See what disposable directories exist
bart --clean

# 2. See how much space they use
bart --explain ./target ./node_modules

# 3. Remove them
bart --clean --apply

# 4. Verify the cleanup
bart --diff
bart --insights .
```

### 4. JSON parsing and integration

Use `--json` whenever another agent, script, or cache consumes the tree.

```bash
# Full JSON export of the current directory
bart --json -d 3 -n 0 . > project-tree.json

# JSON export limited to the top 5 entries per directory
bart --json -d 2 -n 5 ./src > src-summary.json

# Export CSV for spreadsheet analysis
bart --csv -d 3 . > project-tree.csv
```

## Flags to avoid in agent sessions

| Flag | Why avoid |
|------|-----------|
| `-w, --watch` | Launches a live watcher; blocks the session. |
| `-f, --filter` | Launches an interactive TUI; blocks the session. |

## Anti-patterns

- **Do not parse human-readable output.** Emoji, progress spinners, and cached
  state make it unreliable. Use `--json` or `--csv` for programmatic work.
- **Do not run `bart --clean --apply` without a dry-run.** `--clean` first,
  review the list, then add `--apply`.
- **Do not flip `--no-ignore` across runs and expect consistent `--diff`.**
  `--diff` compares the current scan against the cached previous scan. Mixing
  `--no-ignore` states can make ignored directories appear as newly added or
  deleted.
- **Do not ignore stale `.toon` caches.** If output looks wrong, run
  `rm -f .toon` in the scanned directory and re-scan.

## Cache behavior and `--diff` caveats

`bart` writes a `.toon` cache file in the scanned directory. The cache can make
subsequent runs faster, but it can also return stale results:

- The human-readable tree may be served from `.toon` and not reflect a recent
  `--no-ignore` change.
- `--diff` compares against the most recent prior scan stored in `.toon`.
- If the cache seems out of sync, delete it and re-run:

  ```bash
  rm -f .toon
  bart --no-ignore -d 3 .
  ```

If no prior scan exists, `--diff` reports that; do not treat the absence of
output as a bug.

## Error handling

- **Empty output:** the directory may be empty, or every entry may be ignored
  by `.gitignore`. Try `--no-ignore` to confirm.
- **Permission denied:** ensure the path is readable; run from a directory with
  appropriate access.
- **`--diff` shows unexpected add/delete spikes:** check whether the prior
  scan used different flags (especially `--no-ignore`) or whether `.toon` is
  stale.
- **Non-zero exit:** verify the path exists and is a directory; re-run with
  `--json` to get structured output that is easier to inspect.

## Safety

- `--clean --apply` is destructive. Always run `--clean` alone first, read the
  proposed list, and confirm nothing important is included.
- By default bart respects `.gitignore`. Use `--no-ignore` only when you
  genuinely suspect ignored directories are relevant.
- bart creates a `.toon` cache file in the scanned directory. It is safe to
  delete, but do not commit it unless your project specifically needs it.
- When parsing `--json` output, handle the possibility of empty `children`
  arrays for files and empty directories.
