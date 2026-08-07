---
name: deckhand
description: >-
  Use `deckhand` to inspect disk usage and clean build artifacts across Cargo,
  Node/Bun, Python, Go, Swift, Gradle, .NET, and Maven projects. Reach for it after builds,
  benchmarks, load tests, or dependency surgery when `target/`, caches, or
  generated artefacts need to be measured, pruned, or reset. It can also scan
  the filesystem for cleanup candidates (`inspect`), run threshold-triggered
  auto-cleans, and play opt-in ElevenLabs TTS completion summaries. Prefer
  `deckhand status` and `--dry-run` before any destructive clean or sweep.
---

# deckhand

`deckhand` keeps build artefacts under control. It is the operational-hygiene
counterpart to version-management tools like kaptaind: kaptaind commits the
work, deckhand clears the sea-state after builds and benchmarks.

It supports mixed-language workspaces, but in this repository it is primarily a
Cargo workspace cleaner for `target/` and related caches.

## When to call deckhand

Call deckhand when you need to:

- Report what is consuming disk after builds, benchmarks, or load tests.
- Clean Cargo `target/` artefacts safely after a build or test cycle.
- Sweep stale language caches and generated artefacts.
- Scan a directory tree for Rust projects with reclaimable `target/` space.
- Recover disk before packaging, imaging, or pushing a large change.
- Verify that cleanup did not leave unexpected workspace changes.

Do **not** call deckhand when:

- A build, test, benchmark, or daemon is still writing artefacts.
- You only need source context; use reading/search tools instead.
- You are unsure which paths a sweep will touch; run `status`/`--dry-run` first.

## CLI at a glance

```text
deckhand [OPTIONS] <COMMAND>
```

Commands:

| Command | Purpose |
|---------|---------|
| `status` | Report workspace “sea-state” / disk usage. |
| `clean` | Run Cargo-style artefact cleaning across the workspace. |
| `sweep` | Remove stale artefacts and caches beyond a normal clean. |
| `inspect` | Scan a tree for Rust projects and report cleaning candidates (read-only). |
| `auto-clean` | Match installed binaries to `target/` outputs and clean when thresholds are met. |
| `auto-start` | Install/manage a systemd user service that runs `auto-clean` at login. |
| `init` | Create `deckhand.toml` and `.deckhandignore` in the current project. |
| `update` | Check for a newer release tag and install it from the source clone. |

Global options:

| Flag | Meaning |
|------|---------|
| `-c, --config <PATH>` | Use a specific `deckhand.toml`. |
| `--no-color` | Disable coloured output. |
| `--no-emoji` | Disable emoji icons and progress spinners. |
| `--tts` / `--no-tts` | Force-enable or disable ElevenLabs spoken summaries. |
| `--tts-voice <VOICE_ID>` | Override the ElevenLabs voice for one run. |
| `--tts-model <MODEL_ID>` | Override the ElevenLabs model for one run. |
| `--tts-api-key <KEY>` | Explicit ElevenLabs key (prefer env/config; process-local). |

Exit codes: `0` when everything succeeded, `1` when any project or artefact
failed (per-project errors are printed to stderr). Always check the exit code
before assuming a cleanup happened.

Human-readable output uses emoji labels and a moon-phase spinner during long
scans. JSON output and TTS summaries stay plain text. Use `--no-emoji` to
disable icons and spinners.

## Common commands

```bash
# Inspect largest artefacts before deleting anything
deckhand status --limit 20

# Machine-readable status for another agent/tool
deckhand status --json --limit 20

# Preview a Cargo clean without removing files
deckhand clean --dry-run

# Clean only debug artefacts, preserving release artefacts
deckhand clean --profile debug

# Clean only artefacts older than 14 days
deckhand clean --profile all --older-than 14

# Preview stale-cache cleanup
deckhand sweep --dry-run

# Sweep the current project, keeping registry cache entries newer than 14 days
deckhand sweep --path . --keep-days 14

# Find Rust projects under ~ with the most reclaimable space (read-only)
deckhand inspect --min-size 100MB

# Preview a threshold-triggered auto-clean
deckhand auto-clean --dry-run

# Create project config and ignore file
deckhand init

# Speak a completion summary via ElevenLabs for one run
deckhand --tts clean --dry-run

# Force TTS off even when [tts].enabled = true
deckhand --no-tts sweep

# Preview a self-update from the configured source clone
deckhand update --dry-run

# Install a newer release without prompting
deckhand update --yes
```

## Command details

### `status`

Use `status` first. It is the safe read-only view of disk pressure.

```bash
deckhand status --json --limit 20
```

Useful flags:

| Flag | Meaning |
|------|---------|
| `-j, --json` | Emit JSON instead of text. |
| `-l, --limit <N>` | Show only the top N largest artefacts. |

### `clean`

`clean` removes build artefacts. In a Rust workspace this is the `cargo clean`
path; use `--profile` and `--older-than` to avoid deleting more than intended.

```bash
deckhand clean --profile debug --older-than 7 --dry-run
```

Useful flags:

| Flag | Meaning |
|------|---------|
| `-p, --profile <debug\|release\|all>` | Which build profile to clean. Default: `all`. |
| `--dry-run` | Print what would be removed without deleting. |
| `-o, --older-than <DAYS>` | Remove only artefacts older than N days. |
| `-t, --target-dir <PATH>` | Override the Cargo target directory. |

Profile mapping: `debug` is passed to cargo as the `dev` profile (cargo
reserves the name `debug`), so `deckhand clean --profile debug` removes
`target/debug` contents only. `release` passes through unchanged; `all`
cleans the whole target directory.

`--older-than` age-filters the **entire** target directory and ignores
`--profile` — do not combine them expecting a profile-scoped age filter.

### `sweep`

`sweep` is broader than `clean`: it can prune registry caches, git checkouts,
Python bytecode, and language-specific caches depending on config. Treat it as
destructive and preview it first.

```bash
deckhand sweep --path . --keep-days 30 --dry-run
```

Useful flags:

| Flag | Meaning |
|------|---------|
| `-p, --path <PATH>` | Root path to sweep. Default: `.`. |
| `--dry-run` | Print what would be removed without deleting. |
| `-k, --keep-days <DAYS>` | Keep registry cache entries newer than N days. Default: `30`. |

### `inspect`

`inspect` is a read-only scanner: it walks a scope for Rust projects and
reports how much `target/` space each could reclaim. It never deletes
anything.

```bash
deckhand inspect                       # scan $HOME
deckhand inspect --min-size 500MB      # only heavy candidates
deckhand inspect --path ~/projects --json
```

Useful flags:

| Flag | Meaning |
|------|---------|
| `--scope <home\|root>` | Scan `$HOME` (default) or `/` (confined to the root filesystem). |
| `--path <PATH>` | Explicit scan root; overrides `--scope`, expands a leading `~`. |
| `--min-size <SIZE>` | Only show candidates with at least this much reclaimable (e.g. `100MB`). |
| `-j, --json` | Emit JSON instead of text. |

Prefer `--path` over `--scope root`; a full-filesystem scan is slow.

### `auto-clean`

`auto-clean` implements a match-and-patch strategy: it scans the configured
`scan_paths` for installed binaries that match the current contents of a
project's `target/` directory, queues matching projects FIFO (state in
`.deckhand/auto-clean.toml`), and cleans them only when `clutter_tolerance`
or `min_free_space` thresholds are met, after any configured cooldown.

It is **disabled by default** and does nothing unless
`[auto_clean].enabled = true` and at least one threshold is set in
`deckhand.toml`.

```bash
deckhand auto-clean --dry-run
```

Do not enable `auto-clean` unless the user asks: once enabled it cleans
projects without a human invoking deckhand, including projects outside the
current workspace.

### `auto-start`

`auto-start` manages a systemd **user** service (`deckhand-auto-clean.service`)
that runs `deckhand --config <path> auto-clean` at login.

```bash
deckhand auto-start status              # is the service installed/enabled?
deckhand auto-start install --dry-run   # preview the unit file
deckhand auto-start install             # write + enable the unit
deckhand auto-start uninstall           # remove the unit
```

This mutates persistent user systemd state. Only install or uninstall it when
the user explicitly asks, and preview with `--dry-run` first.

### `init`

`init` creates project-local deckhand config:

```bash
deckhand init        # create deckhand.toml and .deckhandignore
deckhand init --force
```

Run it only when the user wants deckhand configuration added to the project. Do
not leave generated config behind in a repository unless it is part of the task.

### `update`

`update` checks the configured git remote for the highest semver release tag,
compares it to the running binary, and installs a newer tag from the local
source clone.

```bash
deckhand update            # notify only (default; requires [update].enabled = true)
deckhand update --dry-run  # preview what would be installed
deckhand update --yes      # install without prompting
deckhand update --force    # reinstall the latest tag even if already on it
```

Useful flags:

| Flag | Meaning |
|------|---------|
| `--dry-run` | Print what would be installed without changing the binary. |
| `--force` | Reinstall the latest tag even if the current binary already matches it. |
| `--yes` | Skip the confirmation prompt and install immediately. |

The command requires `[update].enabled = true` in `deckhand.toml` (or `--yes` /
`--force`) and a local git clone at `[update].source_dir` (default
`~/deckhand`). It fetches the tag, checks it out, runs `./install.sh
INSTALL_DIR=<dir>`, then restores the original branch. Tag discovery uses `git
ls-remote --tags <remote>` with a 30-second timeout, so slow or unauthenticated
remotes fail gracefully.

## Configuration

`deckhand.toml` controls workspace scope, clean behaviour, sweep behaviour, and
the optional auto-clean strategy. `deckhand init` generates:

```toml
# Deckhand configuration
# See https://github.com/elci-group/deckhand

[workspace]
path = "."
members = "auto"          # "auto", "all", or ["crate-a", "crate-b"]

[clean]
profiles = ["debug", "release"]
keep_incremental = false
keep_days = 0             # 0 = no age filter
languages = [
  "cargo"
]
allow_native_commands = true
remove_node_modules = false
remove_venvs = false

[sweep]
registry_cache = true
git_checkouts = true
keep_registry_days = 30
node_modules = false
python_bytecode = true
go_build_cache = false
swift_derived_data = false
nuget_cache = false
bun_cache = false
maven_repository = false

[status]
warn_free_percent = 10

[update]
enabled = false
auto_install = false
remote = "origin"
source_dir = "~/deckhand"
install_dir = "~/.local/bin"

# Optional spoken summaries via ElevenLabs. Disabled by default.
# Key resolution order: --tts-api-key / DECKHAND_TTS_API_KEY, this file,
# project .env, ~/.config/deckhand/deckhand.toml, then ELEVENLABS_API_KEY from
# the environment or ~/.bashrc, ~/.zshrc, or ~/.profile.
# [tts]
# enabled = true
# provider = "elevenlabs"
# voice_id = "21m00Tcm4TlvDq8ikWAM"
# model_id = "eleven_multilingual_v2"
# output_format = "mp3_44100_128"
# base_url = "https://api.elevenlabs.io"
# announce = ["clean", "sweep", "auto_clean"]
# timeout_secs = 30
# api_key = "xi-..."                 # local project key; avoid committing it
# api_key_env = "ELEVENLABS_API_KEY" # or read a named environment variable

# Match-and-patch auto-clean strategy.
# When enabled, deckhand scans the paths below for installed binaries that match
# the current contents of each project's target/ directory. Matched projects are
# queued FIFO and cleaned only when clutter or free-space thresholds are met,
# and only after any configured cooldown has elapsed.
[auto_clean]
enabled = false
scan_paths = ["/bin", "/usr/bin", "/usr/local/bin", "~/.local/bin"]
# clutter_tolerance = "5GB"     # activate when queued artifacts exceed this
# min_free_space = "10GB"       # activate when free space drops below this
# cooldown = "1h"               # global cooldown between automated cleans

# Per-project overrides take precedence over the global cooldown.
# [auto_clean.projects."my-crate"]
# cooldown = "30m"
```

`.deckhandignore` can exclude paths from deckhand operations. Add build outputs,
caches, and generated artefacts there when they should be visible but never
removed automatically.

## ElevenLabs TTS

Deckhand can speak short completion summaries via ElevenLabs. It is opt-in and
never fails a cleanup command if synthesis or playback fails.

Enable it either per run or in config:

```bash
deckhand --tts clean
```

```toml
[tts]
enabled = true
provider = "elevenlabs"
announce = ["clean", "sweep", "auto_clean"]
api_key_env = "ELEVENLABS_API_KEY"
```

API-key resolution order:

1. `--tts-api-key` / `DECKHAND_TTS_API_KEY`
2. Project `deckhand.toml` `[tts].api_key` or `[tts].api_key_env`
3. Project `.env` (`DECKHAND_TTS_API_KEY` or `ELEVENLABS_API_KEY`)
4. Top-level `~/.config/deckhand/deckhand.toml` `[tts]`
5. User environment / shell files: `ELEVENLABS_API_KEY` from the environment,
   `~/.bashrc`, `~/.zshrc`, or `~/.profile`

The implementation uses `curl` plus a local audio player (`ffplay`, `mpg123`,
`mplayer`, `play`, `paplay`, or `aplay`), so it adds no Rust dependencies. Never
log or echo the resolved key.

## Agent workflows

### After a build or benchmark

1. Run `deckhand status --json --limit 20` to see what is heavy.
2. Preview cleanup with `deckhand clean --dry-run`.
3. Run the narrowest clean that solves the problem, usually
   `deckhand clean --profile debug` before `deckhand clean --profile all`.
4. Check the exit code: deckhand exits `1` if any project failed, with the
   per-project error on stderr. Do not assume silence means success.
5. Re-run `git status --short` to confirm no source files were touched.

### Before packaging or releasing

1. Run `deckhand status --limit 50`.
2. Run `deckhand clean --profile all --dry-run`.
3. Run `deckhand sweep --dry-run` if caches are still large.
4. Apply only the operations whose dry-run output is expected, and check the
   exit code afterwards.
5. Rebuild or test if the next step depends on fresh artefacts.

### Reclaiming space across many projects

1. Run `deckhand inspect --min-size 100MB --json` to rank candidates.
2. Clean per project with `deckhand clean -c <project>/deckhand.toml` or from
   inside that project's directory, instead of sweeping from a broad root.

## Safety

- Prefer `status` and `--dry-run` before `clean` or `sweep`.
- Do not run `clean` or `sweep` while builds, tests, benchmarks, or daemons are
  active.
- Use `--profile debug` first when release artefacts may still be needed.
- Use `--older-than` to preserve recently used artefacts; remember it ignores
  `--profile` and age-filters the whole target directory.
- Do not run `deckhand sweep` from `$HOME`, `/`, or another broad root unless the
  user explicitly asks for that scope.
- `sweep` can delete caches and, if configured, `node_modules`, virtualenvs, or
  other large generated trees. Read the dry-run output carefully.
- Do not enable `[auto_clean]` or install the `auto-start` systemd service
  unless the user explicitly asks; both make deckhand act without being run
  manually, and `auto-clean` can clean projects outside the current workspace.
- Deckhand changes are usually reversible by rebuilding, but they can cost a lot
  of time. Keep cleanup scoped to the project being worked on.
- TTS is optional: keep it disabled unless the user wants spoken summaries, and
  never print the ElevenLabs API key or store it in tracked config.

## Kaptaind integration

This repository uses `kaptaind` to monitor, commit, test, and push changes.
`kaptaind.toml` has `[push] enabled = true` and `[test] command = "cargo test && ./install.sh"`,
so kaptaind will run the full test suite and reinstall the binary before every
push. Pushing requires a loaded SSH key for the configured GitHub remote; the
daemon does not handle HTTPS authentication or key unlocking.

Keep `kaptaind` stopped while you are making multi-file edits — it auto-commits
on file changes and can bump the version or push a partial state before you are
ready.

## Local binary location

```bash
/home/sal/.local/bin/deckhand
```

If `deckhand` is not on `PATH`, check that binary and add `~/.local/bin` to the
shell PATH before relying on the skill.

If flags documented here are rejected (e.g. `error: unexpected argument
'--tts'`), the installed binary is stale: `deckhand --version` should match the
`VERSION` file in the source repo (`~/deckhand`). Rebuild and reinstall with
`~/deckhand/install.sh`.
