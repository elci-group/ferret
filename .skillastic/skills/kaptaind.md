---
name: kaptaind
description: >-
  Use `kaptaind` and `kaptaind-cli` for automated semantic versioning,
  change analysis, deterministic git commits, spoken notifications, and
  visual status feedback. Reach for kaptaind when the user wants to
  auto-version a project, analyze a working tree before committing, manage
  Aim-of-Change sessions, or configure notifications/TTS. Prefer
  `kaptaind-cli analyze` for safe dry-runs before enabling the daemon.
---

# kaptaind

`kaptaind` is an automated semantic-versioning daemon. It watches a repo,
clusters changes, scores the diff, computes a semantic bump, and can commit
on your behalf. `kaptaind-cli` provides one-off commands and visibility
without running the daemon.

Use kaptaind for:

- Auto-bumping `VERSION` based on actual code changes.
- Generating deterministic, rule-based commits.
- Running a test hook before every auto-commit.
- Dry-running impact analysis before a commit.
- CI/CD release/hold hints (`ci-hint`).
- Grouping multi-commit work into Aim-of-Change sessions (`aoc`).
- Spoken notifications via TTS when warnings or events fire.
- Visual task-progress feedback in the WebUI and Next.js dashboard.

## Common commands

```bash
# Dry-run analysis of the working tree (safe, no commit)
kaptaind-cli analyze

# Check daemon health
kaptaind-cli status

# View recent automated commits
kaptaind-cli log --limit 20

# Start a named feature session
kaptaind-cli aoc start "feature: auth flow"

# Stop the current feature session
kaptaind-cli aoc stop

# CI release recommendation (JSON)
kaptaind-cli ci-hint --format json

# Preview a manual release (binaries, installers, distribution)
kaptaind-cli ship plan

# Execute a manual release
kaptaind-cli ship run

# Initialize config for the current project
kaptaind-cli init

# Run the daemon in the foreground
kaptaind

# Run as a detached daemon
kaptaind --daemon

# Start the embedded WebUI dashboard
kaptaind --web
```

## Important CLI flags

| Flag | Meaning |
|------|---------|
| `-r, --repo <PATH>` | Operate on a different repo |
| `--format json` | JSON output (useful for `ci-hint`) |
| `--limit <N>` | Limit log output |
| `--web` | Start the WebUI dashboard (default port 8080) |
| `--web-port <PORT>` | Port for the WebUI |

## Configuration

`kaptaind.toml` controls behavior. An optional `.env` file is loaded at
startup (daemon and CLI) so API keys and secrets can stay out of the TOML.

```toml
[watch]
path = "."
recursive = true
ignore_file = ".kaptainignore"

[cluster]
window = 5

[test]
command = "cargo test"
required = true

[push]
enabled = false
branch = "main"

[notify]
webhook_url = "https://hooks.slack.com/services/..."
# Optional text-to-speech for notifications
[notify.tts]
enabled = true
provider = "auto"   # auto | system | elevenlabs | openai | azure | google | cartesia
voice = "21m00Tcm4TlvDq8ikWAM"
rate_limit_seconds = 30
```

Key sections:

- `[watch]` – what to watch and ignore.
- `[cluster]` – time window for batching rapid changes.
- `[weights]` – scoring weights for structural/API/dependency/runtime/risk.
- `[version_thresholds]` – score cutoffs for patch/minor/major bumps.
- `[test]` – test hook that gates commits.
- `[staging]` – which files to stage (`all`, `cluster`, or `pattern`).
- `[qualification]` – post-commit build/release pipeline.
- `[inference]` – optional LLM provider for commit messages.
- `[notify]` – shell/webhook/TTS notifications and nautical theme.

## TTS providers and `.env` keys

When `notify.tts.enabled = true`, kaptaind speaks notification titles through
the configured provider. Provider API keys are read from `.env`:

| Provider | Required env vars |
|----------|-------------------|
| `system` | none (uses `say` on macOS, `espeak` on Linux, PowerShell on Windows) |
| `elevenlabs` | `ELEVENLABS_API_KEY`, optional `ELEVENLABS_VOICE_ID` |
| `openai` | `OPENAI_API_KEY`, optional `OPENAI_TTS_MODEL` / `OPENAI_TTS_VOICE` |
| `azure` | `AZURE_SPEECH_KEY`, `AZURE_SPEECH_REGION` |
| `google` | `GOOGLE_API_KEY` (Cloud Text-to-Speech) |
| `cartesia` | `CARTESIA_API_KEY`, optional `CARTESIA_VOICE_ID` |

`provider = "auto"` prefers `system` when a local TTS command is available,
otherwise picks the first cloud provider whose key is present in the
environment.

## Visual feedback

- The daemon writes `.kaptaind/status.json` with live state, `current_task`,
  and `progress_percent`.
- The embedded WebUI (`kaptaind --web`) shows:
  - **Cloudy sky** while `Clustering` or `Testing`.
  - **Clear sky with horizon** while `Committing`.
  - **Storm / siren overlay** on warnings and failed qualifications.
- The Next.js dashboard shows the same sky-themed status badge and a
  progress bar.

## Integration with ViCo

1. Before enabling the daemon, run `kaptaind-cli analyze` and review the
   proposed bump and score.
2. If the project lacks config, run `kaptaind-cli init` first.
3. Start the daemon with `kaptaind --daemon` only after the user confirms.
4. Use `kaptaind-cli ci-hint --format json` to gate CI releases.
5. For spoken warnings, ensure `[notify.tts]` is enabled and the relevant
   `.env` key is set.

## Daemon hygiene

- **Do not stop the daemon to perform git cleanup.** If kaptaind runtime files
  (e.g. `.kaptaind/`) are accidentally tracked, add the directory to
  `.gitignore`, run `git rm --cached -r .kaptaind`, and either let kaptaind
  auto-commit the change or commit manually. Keep the daemon running.
- Only stop the daemon when the user explicitly asks for it or when it is
  genuinely misbehaving (e.g. stuck in a failing loop).
- Prefer letting kaptaind cluster and commit small follow-up changes rather
  than micro-managing commits around it.

## Safety

- The daemon can auto-commit. Make sure the repo is in the expected state
  before starting it.
- `required = true` under `[test]` means failed tests block commits and
  trigger a warning notification + storm/siren visual.
- Use `--apply` carefully; kaptaind does not push by default (`[push].enabled`
  is false), but commits are local and real.

## Local binary location

Pre-built release binaries are under the kaptaind source tree:

```bash
/home/sal/kaptaind/target/release/kaptaind
/home/sal/kaptaind/target/release/kaptaind-cli
```

Add them to `~/.local/bin` (or install system-wide via `install.sh --system`)
before relying on `kaptaind` being on PATH.
