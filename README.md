# Ferret

**Ferret — Review memory that learns.**

Ferret is a repository-specific review intelligence daemon. It learns the mapping between code changes and review outcomes from teachers — CodeRabbit, GitHub reviews, SARIF, CI fix loops, and humans — and progressively assumes more of the review workload.

<p align="center">
  <img src=".brandi/assets/logo-1024.png" alt="Ferret logo" width="180">
</p>

<p align="center">
  <a href=".brandi/identity.yaml">Brand genome</a> ·
  <a href="tapes/">VHS tapes</a> ·
  <a href="https://github.com/elci-group/ferret">Source</a>
</p>

---

It is **review distillation**, not a generic linter. Ferret builds a corpus from:

- Code diffs and stable hunk IDs
- Review comments, severity, and category
- Developer accept / reject / fix dispositions
- Repository context and conventions

Over time Ferret answers questions like:

- Would CodeRabbit usually complain about this?
- Is this repository strict about lifetime annotations?
- Does this team prefer `Result<T, E>` over panics?

## Confidence routing

| Confidence | Action |
|------------|--------|
| ≥ 0.98 | Ferret reviews alone |
| 0.75 – 0.98 | Ferret + optional teacher verification |
| < 0.75 | Escalate to teacher (default: CodeRabbit) |

Architecture findings escalate earlier (default threshold 0.95).

## Workspace

| Crate | Role |
|-------|------|
| `ferret-core` | Domain types, teacher trait |
| `ferret-store` | SQLite corpus persistence |
| `ferret-ingest` | Git diff extraction, hunk normalization |
| `ferret-teachers` | GitHub, SARIF, CodeRabbit adapters |
| `ferret-index` | Corpus indexing hooks |
| `ferret-infer` | Retrieval-based predictor |
| `ferret-route` | Confidence routing |
| `ferret-cli` | `ferret` command-line tool |
| `ferret-daemon` | `ferretd` webhook daemon |

## Quick start

```bash
cargo build
cargo test

# Initialize local corpus database
cargo run -p ferret-cli -- init

# Register a repository
cargo run -p ferret-cli -- register --slug elci-group/ferret

# Capture a snapshot from a unified diff
cargo run -p ferret-cli -- snapshot \
  --repo elci-group/ferret \
  --base abc123 --head def456 \
  --diff /path/to/changes.diff

# Corpus stats
cargo run -p ferret-cli -- stats

# Start daemon (health + stats endpoints)
cargo run -p ferret-daemon -- --listen 127.0.0.1:8787
```

## Project and Business scans

Ferret's scanner operates on exactly one coherent project boundary. Business
mode is a read-only portfolio orchestrator: it discovers project boundaries
beneath an environment root, passes each project through that same scanner in
isolation, and then aggregates the results.

**`ferret` requires a project target; `ferret business` discovers projects
within an environment and scans them independently.**

```text
USAGE:
    ferret [TARGET]
    ferret business [ROOT]

EXAMPLES:
    ferret .
    ferret ~/src/ferret
    ferret business
    ferret business /home --json
```

The default Business discovery root is `$HOME`. Discovery recognizes Rust,
Node, Python, Go, PHP, Ruby, Elixir, Java, and .NET manifests. It does not
follow directory symlinks by default and skips common repository, dependency,
cache, and build directories. Cargo workspace members are attributed to their
workspace rather than scanned as duplicate logical projects.

Business controls include `--max-depth`, `--include`, `--exclude`,
`--project-kind`, `--parallel`, `--no-git`, and `--follow-symlinks`. Results
are always sorted by canonical project path, independent of completion order.
One malformed project is recorded as an error while remaining projects
continue to scan.

Both modes support `--json`. Business output uses the
`ferret.business/v1` schema and retains every project's identity, analysis,
status, and error separately from cross-project `business_findings`.

Business exit codes are stable:

| Code | Meaning |
|------|---------|
| `0` | Every discovered project passed |
| `1` | One or more project or Business findings were reported |
| `2` | One or more projects could not be scanned |
| `3` | Business invocation or discovery failed |

### Animated result chase

Interactive Ferret commands render every result line as a 3form-backed chase:
the result text is revealed as the trail behind an animated ferret pursuing a
rabbit. The animal font was selected through six recursive mutation/evaluation
rounds and provides three distinct poses for each animal.

Animation follows 3form mode negotiation. It is enabled only for interactive
UI terminals; `--json`, pipes, redirected output, and `3FORM_MODE=machine`
remain deterministic and ANSI-free. Set `FERRET_ANIMATION=0` to disable the
effect explicitly, or `NO_COLOR=1` to retain the chase without colour.

## CLI in action

### Logo

Print the Ferret brand mark.

![Ferret logo](tapes/output/logo.gif)

### Init

Initialize the local database.

![Ferret init](tapes/output/init.gif)

### Register

Register a repository profile.

![Ferret register](tapes/output/register.gif)

### Snapshot

Capture a review snapshot from a diff.

![Ferret snapshot](tapes/output/snapshot.gif)

### Stats

Show corpus statistics.

![Ferret stats](tapes/output/stats.gif)

### Predict

Predict findings for a snapshot using retrieval.

![Ferret predict](tapes/output/predict.gif)

### Route

Show routing decisions for predicted findings.

![Ferret route](tapes/output/route.gif)

## Brand

Ferret's identity is stored in `.brandi/` and linted with [brandi](https://github.com/elci-group/brandi). The palette is inspired by the animal: warm browns, cream, and a pink nose, with clear semantic colors for success, warning, and error states.

| Role | Color |
|------|-------|
| Primary | `#C47E5A` |
| Cream | `#F5E6D3` |
| Pink nose | `#FF6B8A` |
| Dark mask | `#1A1512` |
| Success | `#7EBA5A` |
| Warning | `#FFB020` |
| Error | `#E8505A` |
| Info | `#58D5FF` |

## VHS recordings

All terminal recordings live in `tapes/` and are rendered to `tapes/output/`. Re-render them with:

```bash
for tape in tapes/*.tape; do vhs "$tape"; done
```

## Remote

https://github.com/elci-group/ferret

## License

MIT
