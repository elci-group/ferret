---
name: fract
description: >-
  Use `fract` to measure architectural health (module entropy, cohesion,
  duplication, cycles) in Rust, Python, TypeScript, and JavaScript projects,
  and to work safely on the fract codebase itself at /home/sal/fract. Reach
  for this skill when the user asks to index a project's module health, find
  monoliths or god objects, run the fract daemon, or make changes to fract —
  its quality gate, zero-dependency policy, and kaptaind auto-commit workflow
  are unusual and mandatory.
---

# fract

`fract` is an autonomous architectural maintenance daemon. It indexes a
project's modules, scores structural entropy and confidence, queues refactor
candidates, and — in daemon mode — invokes an LLM refactor engine, validates
the result (`fmt`, `clippy`, `check`, `test`), and journals every step to
`.fract/state.jsonl`.

Source and docs: `/home/sal/fract` (private repo `github.com:elci-group/fract`).
Design details: `/home/sal/fract/ARCHITECTURE.md`.

## Using fract as a tool

Binary: `/home/sal/fract/target/release/fract` (rebuild with
`cargo build --release` in `/home/sal/fract` if stale).

```bash
fract index                     # index cwd, print module health table
fract index -f json             # machine-readable: human, json, jsonl, sarif, markdown
fract --no-color index          # plain output for logs/LLM consumption
fract run                       # daemon mode (default command): watch + auto-refactor
fract init                      # write a default config file
```

- Entropy ≥ ~0.65 marks a module as a refactor candidate; use `index` output
  to find monoliths/god objects (sort by ENTROPY, read NEXT ACTION).
- Dogfood baseline for fract itself: 59 modules, ~98% health, 0 critical,
  worst module ≤ 0.67. If a self-index regresses past that, investigate
  before adding code.
- `NO_COLOR` env var and `--no-color` are equivalent; `RUST_LOG=fract=debug`
  raises daemon logging.

## Working on the fract codebase

### The quality gate is law

`bash scripts/ci.sh` runs, in order: `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test`, `amber` (threshold
70, strict), `cargo audit`, `cargo build --release`. A change is not done
until this passes. Clippy **pedantic** is enforced workspace-wide via
`[lints.clippy]` in `Cargo.toml` (single documented exception:
`cast_precision_loss`; item-level allows need a justification comment).

### Zero-dependency policy (amber-enforced)

Anything implementable in-tree with `std` lives in-tree. Forbidden crates
include `anyhow`, `thiserror`, `serde_json`, `walkdir`, `clap`, `chrono`,
`uuid`, `tempfile`, `regex`, `reqwest`, `tokio-test` — use the internal
replacements instead: `error`, `json`, `walk`, `cli`, `time`, `id`,
`scratch` modules. Allowed: `tokio`, `notify`, `axum`, `futures-util`,
`serde`, `toml`, `tracing`, `tracing-subscriber`, `git2`. Proposing a new
dependency means editing `.amber.toml` and justifying it; reintroducing a
forbidden crate fails CI.

### kaptaind owns commits — never commit by hand

A kaptaind daemon watches `/home/sal/fract`, clusters filesystem events,
scores them (patch ≥ 0.15 / minor ≥ 0.45 / major ≥ 0.75), runs `scripts/ci.sh`
as a pre-commit hook, and commits with an automatic semver bump. Therefore:

- **Do not run `git commit`/`git add`** in this repo; leave changes in the
  working tree and let the daemon sweep them (staging mode `all` sweeps the
  whole dirty tree on the next qualifying cluster).
- **Never hand-edit version numbers** in `Cargo.toml` or `VERSION`.
- After a batch of edits, poll `git status --porcelain | wc -l` until 0
  (daemon has committed), then `git push origin main` (kaptaind does not
  push). A broken tree simply won't commit — if it stays dirty >15 min,
  check `.kaptaind/status.json` `last_error` for the gate failure.
- Do not stop or restart the daemon unless the user asks. The user's daemon
  is shared infrastructure; never run a second `kaptaind --daemon` on the
  same repo (double-daemon races produce empty version-bump commits).

### Testing conventions

- Inline `#[cfg(test)]` unit tests per module; integration tests in `tests/`.
- Property tests use a hand-rolled deterministic generator (no `proptest`/
  `quickcheck` — dependency policy). Seed from a fixed `u64`, document the
  characterization being pinned.
- Miri: `cargo +nightly miri test` must pass with 0 UB. Tests that read
  process-global state (env vars, cwd) need an injectable seam — see
  `Style::detect_auto` in `src/report/style.rs` for the pattern.
- Known quirks are pinned by characterization tests, not "fixed":
  `LlmConfig` serde defaults, untracked-files-don't-dirty-tree, entropy
  non-monotonicity in `lines` for NaN-heavy inputs. Don't change these
  semantics without updating the tests deliberately.

### Layout map

`src/` modules: `indexer`, `complexity`, `confidence`, `queue`, `refactor`,
`validation`, `merge`, `git`, `store`, `daemon/` (pipeline, notify),
`engine_http`, `web`, `events`, `report/` (style, sarif, model), `scanner/`
(rust, python, jsts, mask), `model/`, plus the dependency-replacement
modules `error`, `json`, `walk`, `cli`, `time`, `id`, `scratch`, `config`.
Full map with data flow: `ARCHITECTURE.md`.

### Hard rules

- No `unsafe` (the only matches are scanner test fixtures).
- Blocking work goes on `spawn_blocking`; stores expose `*_async` wrappers.
- Swallowed errors are a regression — log with `warn!` and context.
- Extract literal numbers into named constants.
- Baseline to hold: 360 tests passing, ~92% line coverage, rustdoc with 0
  warnings, release binary ≈ 6 MB.
