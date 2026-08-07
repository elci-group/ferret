---
name: amber
description: >-
  Use `amber` to analyze Rust projects for replaceable or removable third-party
  dependencies. Run it whenever a Cargo project may be carrying crates that are
  lightly used, easily replaced with stdlib code, or internally vendorable.
  Prefer `--format json` when another tool consumes the report and `--propose`
  when you need concrete replacement modules.
---

# amber

`amber` is an autonomous dependency-reduction engine for Rust. It inventories a
Cargo project's dependencies, analyzes how each crate is used through AST
visits, scores replaceability, and can emit drop-in replacement proposals.

## Quick decision flow

1. **First look at a Rust project's dependencies:** `amber /path/to/project`
2. **Machine-readable report for another tool:**
   `amber /path/to/project --format json analyze --output amber_report.json`
3. **Generate replacement modules:**
   `amber /path/to/project --threshold 70 --propose`
4. **Score one specific crate:** `amber /path/to/project score anyhow`
5. **Generate a single replacement file:**
   `amber /path/to/project replace anyhow --out-dir amber_out`
6. **List every dependency with metadata:** `amber /path/to/project list`

## When to call amber

Call amber when you need to:

- Audit a Rust codebase for dependency bloat.
- Find crates that can be removed or replaced with stdlib/internal code.
- Produce a scored dependency report for a migration or cleanup task.
- Generate a starting replacement module for a high-scoring utility crate.
- Validate that a project's dependency tree complies with a policy.

Do **not** call amber when:

- The project is not a Rust/Cargo project.
- You already know the dependency set and no analysis is required.
- You intend to edit `Cargo.toml` or source files without reviewing amber's
  proposals first.

## CLI at a glance

```text
amber [OPTIONS] [PATH] [COMMAND]
```

| Flag | Default | Meaning |
|------|---------|---------|
| `PATH` | `.` | Path to the Rust project or `Cargo.toml` |
| `-f, --format <console\|json\|pr\|sarif\|emoji>` | `console` | Output format |
| `--emoji` | off | Shorthand for `--format emoji` |
| `-t, --threshold <0-100>` | `50` | Minimum replaceability score to flag |
| `-T, --transitive` | off | Include transitive dependencies |
| `-p, --propose` | off | Write replacement proposals to `amber_proposals/` |
| `--no-dev` | off | Exclude dev-dependencies |
| `-c, --crates <NAMES>` | none | Focus on comma-separated crate names |
| `-v, --verbose` | off | Repeat for debug/trace logging |
| `--config <PATH>` | `.amber.toml` | Path to amber config |

## Commands

| Command | Purpose |
|---------|---------|
| `analyze` | Full analysis and report (default when no command given) |
| `list` | List all dependencies with metadata |
| `score <crate>` | Score one crate's replaceability |
| `replace <crate>` | Generate a replacement module for one crate |
| `roadmap` | Show Amber's internal module roadmap |

## Output formats

- **console** — Human-readable tables and score cards.
- **json** — Machine-readable report; good for agent consumption.
- **pull-request** — Markdown formatted like a pull-request description.
- **sarif** — Static analysis results interchange format for CI.
- **emoji** — Console output with emoji markers.

## Configuration

Amber reads `.amber.toml` from the target project:

```toml
threshold = 60

[policy]
required = ["serde"]
forbidden = ["deprecated_crate"]
strict = false

[weights]
# Optional per-dimension scoring weights (0-1)
simplicity = 0.25
transitive_value = 0.15
security = 0.20
maintenance = 0.15
testability = 0.10
api_surface = 0.15
```

## Common commands

```bash
# Analyze the project in the current directory
amber

# Analyze a specific project with JSON output (note: --output belongs to `analyze`)
amber /path/to/project --format json analyze --output report.json

# Generate replacement proposals for crates scoring 70+
amber /path/to/project --threshold 70 --propose

# Score a single crate
amber /path/to/project score anyhow

# Generate a replacement module for one crate
amber /path/to/project replace anyhow --out-dir amber_out

# Include transitive dependencies
amber /path/to/project --transitive

# Exclude dev-dependencies
amber /path/to/project --no-dev

# Analyze with a custom policy config
amber /path/to/project --config /path/to/.amber.toml
```

## Agent workflows

### Dependency cleanup task

1. Run `amber /path/to/project --format json analyze --output /tmp/amber_report.json`.
2. Read `/tmp/amber_report.json` and identify crates above the threshold.
3. For each candidate, run `amber /path/to/project score <crate>` for details.
4. Generate a proposal: `amber /path/to/project replace <crate> --out-dir amber_out`.
5. Review the generated module and integrate it if appropriate.
6. Remove the crate from `Cargo.toml` if it is now unused.
7. Run `cargo check` and `cargo test` to validate.

### CI-style gating

1. Add a `.amber.toml` with `required`/`forbidden` policies.
2. Run `amber /path/to/project --format sarif analyze --output amber.sarif`.
3. Inspect exit code: `0` no findings, `1` warnings, `2` strict policy violation.

## Interpreting scores

- **overall** — Aggregate replaceability score (0-100). Higher means safer to
  replace or remove.
- **confidence** — How visible the usage is to amber's AST analysis.
- Dimensions include **simplicity**, **transitive_value**, **security**,
  **maintenance**, **testability**, and **api_surface**.

A crate with a high overall score but low confidence should be reviewed
manually before removal.

## Anti-patterns

- **Blindly removing high-scoring crates** without reading generated proposals.
- **Ignoring low-confidence scores** — amber may not see dynamic or macro-heavy
  usage.
- **Running with `--threshold 0` and proposing replacements** for every crate;
  this produces noise.
- **Forgetting `cargo test`** after applying a replacement.
- **Running amber outside a Cargo project** — it requires `Cargo.toml`.

## Safety

- amber only reads files and, with `--propose`/`replace`, writes proposal files
  to the specified output directory. It never modifies `Cargo.toml` or source
  files directly.
- Validated replacements are checked with `cargo check` in a temporary context,
  but always re-check in the real project before committing.
- The `online` feature requires network access to crates.io; default operation
  is fully offline.
