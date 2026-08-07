---
name: bound
description: >-
  Use `bound` to bundle a filtered slice of a codebase into a single text
  snapshot. Use it whenever ViCo needs to send source context to an LLM,
  create a searchable bundle, or inspect many files at once without opening
  each one. Always respect token budgets and write large snapshots to a temp
  file instead of the clipboard.
---

# bound

`bound` is a recursive file aggregator with language-aware filtering and
clipboard support. It produces a single text snapshot (or JSON) from a
directory tree, which is ideal for LLM context, project manifests, and bulk
inspection.

## Quick decision flow

1. **Only 1–2 files?** Use `Read` directly.
2. **Many files of one type?** Run `bound "[.ext]" --meta --tree -t 8000 --out /tmp/...`
3. **Multiple extensions needed?** Run once per extension; multi-extension
   syntax is unreliable in the current build.
4. **Whole-project overview?** Run without a filter, which scans the current
   directory. It includes every non-hidden file, so use it sparingly.
5. **Output size unknown or > ~50 KB?** Always write to `--out`.

## When to use `bound`

Use `bound` instead of opening files individually when:

- You need a coherent codebase snapshot for an LLM prompt.
- You want every file of one type (e.g., all `.rs` or `.py`).
- You need a project manifest with `--meta` and/or `--tree`.
- You want to search or reason across many files at once.

Do **not** use `bound` when:

- You only need one or two specific files (use `Read` directly).
- You need secret-free content (`bound` does not redact; filter carefully).
- You need the complete content of files larger than your token/size limit.

## CLI at a glance

```text
bound [OPTIONS] [FILTER] [DIRECTORY]
```

| Flag | Default | Meaning |
|------|---------|---------|
| `-t, --token-limit <N>` | none | Max tokens per file (truncates content) |
| `-s, --size-limit <N>` | none | Max bytes per file (truncates content) |
| `-d, --depth-limit <N>` | none | Max recursion depth |
| `--meta` | off | Include per-file metadata headers |
| `--meta-hash` | off | Add SHA-256 hash to metadata (requires `--meta`) |
| `--tree` | off | Include a file tree at the top |
| `--out <PATH>` | clipboard | Write snapshot to file |
| `--json` | off | Emit JSON instead of `expandable` text |
| `--furnace` | off | Reserved; currently returns no analysis |

- `FILTER` is optional. If you omit it, omit `DIRECTORY` too; the binary scans
  the current directory.
- When `FILTER` is supplied, `DIRECTORY` defaults to `.`.

## Output target: file vs clipboard

| Situation | Target | Rationale |
|-----------|--------|-----------|
| Output ≤ ~50 KB and trivial to recreate | Clipboard | Fast, no cleanup |
| Output > ~50 KB, unknown size, or fed to another LLM | `--out <path>` | Avoids clipboard truncation/loss |

For ViCo workflows, default to a temp file:

```bash
bound "[.rs]" --meta --tree -t 8000 --out /tmp/bound_$(uuidgen).txt .
```

If `uuidgen` is unavailable, use a counter or date string. Always delete the
file after the turn.

## Token-budget heuristics

`-t, --token-limit` caps tokens **per file**, not the whole snapshot. The total
snapshot is the sum of file contents, metadata, and tree. Choose `-t` so the
whole bundle leaves room for instructions and the model's response.

| Consumer context window | Recommended `-t` | Notes |
|------------------------|------------------|-------|
| ~4K total (very tight) | 1000–2000 | Bundle only the most relevant files |
| ~8K–16K | 4000–6000 | Good default for focused tasks |
| ~32K–128K | 8000–12000 | Standard for rich codebase snapshots |
| ~200K+ | 12000–16000 | Only when you genuinely need huge files |

Rules of thumb:

- Start with `-t 8000` for typical LLM consumption.
- If the snapshot exceeds ~75% of the model's context window, narrow the filter
  or scope the directory instead of raising the limit.
- Prefer excluding generated files, lockfiles, and binaries over raising the
  limit.
- For very small files you can omit `-t` entirely.

## Filter syntax

- `[.ext]` — single extension. Reliable. Examples: `[.rs]`, `[.py]`.
- `{.ext}` — single-extension alternate form. Also reliable for one extension.
- `{.ext1,.ext2}` — documented for multiple extensions, but the current binary
  build (v0.1.2) does not match files with this form. **Avoid it.**
- No filter — includes every non-hidden file recursively. Use with caution.

If you need several extensions, run `bound` once per extension and concatenate
the outputs:

```bash
bound "[.ts]" --meta --tree -t 8000 --out /tmp/ts.txt .
bound "[.tsx]" --meta --tree -t 8000 --out /tmp/tsx.txt .
cat /tmp/ts.txt /tmp/tsx.txt > /tmp/web.txt
```

## Common commands

```bash
# Standard Rust snapshot to a temp file
bound "[.rs]" --meta --tree -t 8000 --out /tmp/rust_snapshot.txt .

# Python snapshot
bound "[.py]" --meta --tree -t 8000 --out /tmp/py_snapshot.txt .

# TypeScript/React snapshot
bound "[.tsx]" --meta --tree -t 12000 --out /tmp/web_snapshot.txt .

# Project manifest only
bound "[.toml]" --meta --tree -t 4000 --out /tmp/manifests.txt .

# Scoped subsystem snapshot
bound "[.rs]" --meta --tree -t 8000 --out /tmp/api_snapshot.txt ./src/api
```

## Examples by language / ecosystem

### Rust

```bash
bound "[.rs]" --meta --tree -t 8000 --out /tmp/rust_snapshot.txt .
bound "[.toml]" --meta --tree -t 4000 --out /tmp/rust_manifests.txt .
bound "[.rs]" --meta --tree -t 8000 --out /tmp/rust_src.txt src/
```

### Python

```bash
bound "[.py]" --meta --tree -t 8000 --out /tmp/py_snapshot.txt .
bound "[.toml]" --meta --tree -t 2000 --out /tmp/py_manifests.txt .
bound "[.py]" --meta --tree -t 8000 --out /tmp/py_prod.txt src/
```

### TypeScript / JavaScript

```bash
bound "[.ts]" --meta --tree -t 8000 --out /tmp/ts_snapshot.txt .
bound "[.tsx]" --meta --tree -t 8000 --out /tmp/tsx_snapshot.txt .
bound "[.json]" --meta --tree -t 2000 --out /tmp/node_manifests.txt .
bound "[.tsx]" --meta --tree -t 8000 --out /tmp/components.txt components/
```

### Go

```bash
bound "[.go]" --meta --tree -t 8000 --out /tmp/go_snapshot.txt .
bound "[.mod]" --meta --tree -t 2000 --out /tmp/go_mod.txt .
bound "[.go]" --meta --tree -t 8000 --out /tmp/go_internal.txt internal/ pkg/
```

### C / C++

```bash
bound "[.c]" --meta --tree -t 8000 --out /tmp/c_snapshot.txt .
bound "[.h]" --meta --tree -t 8000 --out /tmp/h_snapshot.txt .
bound "[.cpp]" --meta --tree -t 8000 --out /tmp/cpp_snapshot.txt .
```

### Configuration / infra

```bash
bound "[.tf]" --meta --tree -t 8000 --out /tmp/tf_snapshot.txt .
bound "[.yaml]" --meta --tree -t 6000 --out /tmp/k8s_snapshot.txt k8s/
bound "[.sh]" --meta --tree -t 4000 --out /tmp/shell_snapshot.txt .
```

## JSON output schema

With `--json`, `bound` emits a single JSON object:

```json
{
  "tree": "<string | null>",
  "files": [
    {
      "metadata": {
        "relative_path": "src/lib.rs",
        "size_bytes": 44,
        "line_count": 1,
        "modified_unix": 1783090367,
        "sha256": "..."
      },
      "content": "pub fn add(...)  {...}\n",
      "furnace_report": null
    }
  ]
}
```

Notes:

- `tree` is `null` unless `--tree` is used.
- `metadata` is `null` unless `--meta` is used.
- `sha256` is `null` unless `--meta-hash` is used.
- `content` is truncated when `-t` or `-s` is set.
- Hidden files (dotfiles) are skipped.
- `--furnace` does not currently populate `furnace_report`.

## Reading and cleaning up output

When using `--out`:

1. Read the file with `Read` or `cat`.
2. Include the contents in the prompt or analysis context.
3. Delete the temp file when done.

```bash
bound "[.rs]" --meta --tree -t 8000 --out /tmp/bound_work.txt .
cat /tmp/bound_work.txt
rm /tmp/bound_work.txt
```

If the snapshot is too large to read in one turn, read it in chunks or
summarize by file before passing it forward.

Avoid writing the snapshot file into the directory you are snapshotting;
otherwise a later no-filter run may bundle its own output.

## Integration with ViCo

In build-mode reflection, use `bound` to capture an existing project snapshot
before asking the planner model to reason about it. Typical flow:

1. Detect project type.
2. Choose extension filters for that type (e.g., `[.rs]` for Rust, `[.py]` for
   Python).
3. Run `bound` with `--meta --tree -t 8000 --out /tmp/bound_<uuid>.txt`.
4. Read the file and include it in the reflection context.
5. Delete the temp file after use.

If the reflection target is a subsystem, scope `bound` to that directory. When
multiple file types are needed, run `bound` once per type and concatenate the
snapshots.

## Anti-patterns

- **One-off reads:** Using `bound` for a single file wastes tokens and adds
  clutter.
- **Multi-extension filters:** Using `{.rs,.py}` returns empty results in the
  current build.
- **Passing `.` without a filter:** `bound --tree .` is parsed as an invalid
  filter. Omit the directory to scan `.`, or supply a filter first.
- **Writing snapshots into the source tree:** The next no-filter run will bundle
  its own output.
- **Relying on `--furnace`:** It currently produces no analysis.
- **Ignoring size:** Big snapshots blow context windows; prefer scoping over
  raising limits.

## Troubleshooting

| Symptom | Likely cause | Fix |
|---------|--------------|-----|
| `Invalid filter format: '.'` | Passed directory as first argument with no filter | Run `bound --tree` or `bound "[.rs]" .` |
| Empty output with correct filter | Wrong directory, `-d` too shallow, or filter typo | Check path and remove/raise `-d` |
| Metadata missing | Forgot `--meta` | Add `--meta`; add `--meta-hash` for hashes |
| Tree missing | Forgot `--tree` | Add `--tree` |
| Content truncated | `-t` or `-s` limit hit | Raise limit or scope to fewer files |
| SHA-256 null | Used `--meta` but not `--meta-hash` | Add `--meta-hash` |

## Safety

- Always use `--out` for non-trivial projects; clipboard can be lost or
  truncated.
- Respect token limits. A good default is `8000` tokens per file.
- Do not bundle secrets (`.env`, key files). `bound` does not redact content.
- Exclude dependency directories (`node_modules`, `.venv`, `target`) and
  generated artifacts by scoping `DIRECTORY` to source directories.
- Delete temp snapshot files after the LLM turn to avoid leaving code copies on
  disk.
- Hidden files are skipped, but assume sensitive files could still be included
  if they lack a leading dot.
