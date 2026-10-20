---
name: kaptaind
description: >-
  Use `kaptaind` and `kaptaind-cli` (v10.x) for automated semantic
  versioning, change analysis, deterministic git commits, governed
  branch/release lifecycle, and spoken/visual status feedback. Reach for
  kaptaind when the user wants to auto-version a project, analyze a working
  tree before committing, manage Aim-of-Change sessions, gate a release, or
  configure notifications/TTS. Prefer `kaptaind-cli analyze` for safe
  dry-runs. New profiles are observe-only by default — commits only happen
  once `kaptaind.toml` has `[operation] mode = "actuate"`.
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
- Dry-running impact analysis before a commit (`analyze`, `--dry-run`).
- CI/CD release/hold hints (`ci-hint`).
- Grouping multi-commit work into Aim-of-Change sessions (`aoc`, `trace`).
- Governed branch lifecycle and integration analysis (`branch`, `integrate`,
  `pull`, `release`, `checkout`).
- Qualification/evidence pipelines for compliance-gated releases (`doctor`,
  `stress`, `report`, `audit`, `evidence`, `governance`).
- Multi-repo daemon fleet management (`trawl`, `monitor`, `service`,
  `autostart`).
- High-availability daemon leader election (`shark`).
- Spoken notifications via TTS when warnings or events fire.
- Visual task-progress feedback in the WebUI and Next.js dashboard.

## ⚠️ Observe-only by default

A freshly-generated `kaptaind.toml` **does not commit anything**. The daemon
and `kaptaind-cli analyze` both score changes and record the decision to
`.kaptaind/decisions.jsonl`, but nothing is staged, committed, VERSION-
written, pushed, or shipped until the config opts in:

```toml
[operation]
mode = "actuate"
```

Pushing additionally requires `[push] enabled = true` and
`[capabilities] network_push = true`. If a user expects commits and none are
appearing, check `kaptaind-cli explain` or grep `.kaptaind/decisions.jsonl`
for `"outcome":"observed"` before assuming something is broken.

## Common commands

```bash
# Dry-run analysis of the working tree (safe, no commit)
kaptaind-cli analyze

# Check daemon health and installed version
kaptaind-cli status

# View recent automated commits
kaptaind-cli log --limit 20

# Explain recent cluster decisions, including skipped/observed ones
kaptaind-cli explain

# Version history: tags, commits, bumps, pushes
kaptaind-cli history

# Start / stop a named Aim-of-Change session
kaptaind-cli aoc start "feature: auth flow"
kaptaind-cli aoc stop

# Inspect per-cluster AoC trace records
kaptaind-cli trace

# CI release recommendation (JSON)
kaptaind-cli ci-hint --format json

# Preview / execute a manual release (binaries, installers, distribution)
kaptaind-cli ship plan
kaptaind-cli ship run

# Governed branch lifecycle
kaptaind-cli branch
kaptaind-cli integrate
kaptaind-cli pull
kaptaind-cli release
kaptaind-cli checkout stable

# Undo the most recent kaptaind-produced commit
kaptaind-cli rollback

# Discover and initialize codebases under a directory tree
kaptaind-cli trawl

# Initialize config for the current project
kaptaind-cli init

# Live terminal dashboard
kaptaind-cli dashboard

# Run the daemon in the foreground
kaptaind

# Run as a detached daemon
kaptaind --daemon

# Start the embedded WebUI dashboard
kaptaind --web
```

## Full command surface (kaptaind-cli, v10.3.3)

Grouped by purpose — run `kaptaind-cli <command> --help` for details.

| Group | Commands |
|-------|----------|
| Core versioning | `status`, `log`, `analyze`, `explain`, `history`, `rollback`, `validate` |
| Sessions | `aoc` (Aim-of-Change sessions), `trace` (per-cluster traces) |
| Branch/release governance | `branch`, `integrate`, `pull`, `release`, `checkout`, `ship`, `ci-hint` |
| Qualification pipeline | `doctor` (host profile), `stress` (synthetic-repo drive), `report` (evidence bundle) |
| Compliance | `audit`, `evidence`, `governance`, `integrations`, `environment` |
| Fleet / daemon lifecycle | `trawl`, `monitor`, `service`, `autostart`, `enable-autostart`, `disable-autostart` |
| Observability | `dashboard`, `logs`, `probe`, `schema` |
| Storage / HA | `storage`, `shark` (leader election), `migrate` (`.kaptaind` state schema) |
| Setup | `init` |

`kaptaind` (the daemon binary) also exposes `--dock`, `--radar`, `--lanes`
for live views, `--shark-mode`/`--shark-arbiter` for HA, and `pull` directly.

## Important CLI flags

| Flag | Meaning |
|------|---------|
| `-r, --repo <PATH>` | Operate on a different repo |
| `--format json` | JSON output (useful for `ci-hint`, `status`, etc.) |
| `--limit <N>` | Limit log/history output |
| `--force` | Skip qualification gates / dirty-tree guard where supported |
| `-w, --web` | Start the WebUI dashboard (default port 8080) |
| `--web-port <PORT>` | Port for the WebUI |
| `--health-port <PORT>` | Override the health server port |
| `--dry-run` | Show the decision the daemon would make, without acting |

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

[weights]
s = 0.35
a = 0.30
d = 0.20
r = 0.15

[version_thresholds]
minor = 0.6
patch = 0.0

[test]
command = "cargo test"
required = true

[operation]
mode = "actuate"   # omit/observe = analysis-only, no commits

[push]
enabled = false
branch = "main"

[capabilities]
network_push = true   # required in addition to [push].enabled to push

[ratelimit]
min_commit_interval = 2

[trust]
execution = "trusted"

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
- `[operation]` – `mode = "actuate"` opts a repo into real commits; anything
  else is observe-only.
- `[push]` / `[capabilities]` – both must be enabled to push automatically.
- `[ratelimit]` – minimum interval between automated commits.
- `[trust]` – execution trust level for the repo.
- `[staging]` – which files to stage (`all`, `cluster`, or `pattern`).
- `[qualification]` – post-commit build/release pipeline (feeds `doctor`,
  `stress`, `report`).
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
- `kaptaind-cli dashboard` gives the same live view in the terminal;
  `kaptaind --dock` / `--radar` / `--lanes` give lighter-weight daemon views.

## Integration with ViCo

1. Before enabling the daemon, run `kaptaind-cli analyze` and review the
   proposed bump and score.
2. If the project lacks config, run `kaptaind-cli init` first.
3. Confirm `[operation] mode = "actuate"` is set if the user actually wants
   commits — otherwise the daemon will only observe and log decisions.
4. Start the daemon with `kaptaind --daemon` only after the user confirms.
5. Use `kaptaind-cli ci-hint --format json` to gate CI releases, or
   `kaptaind-cli report` / `kaptaind-cli governance` for compliance-gated
   releases.
6. For spoken warnings, ensure `[notify.tts]` is enabled and the relevant
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

- The daemon can auto-commit, but only once `[operation] mode = "actuate"`
  is set — a fresh profile is observe-only. Make sure the repo is in the
  expected state before flipping that on.
- `required = true` under `[test]` means failed tests block commits and
  trigger a warning notification + storm/siren visual.
- Pushing needs both `[push].enabled = true` and
  `[capabilities].network_push = true`; without them commits stay local.
- `kaptaind-cli rollback` reverts the most recent kaptaind-produced commit
  (preview by default; pass `--yes` to apply).

## Local binary location

Pre-built release binaries are under the kaptaind source tree, and are also
installed on PATH:

```bash
/home/sal/kaptaind/target/release/kaptaind
/home/sal/kaptaind/target/release/kaptaind-cli
/home/sal/.local/bin/kaptaind
/home/sal/.local/bin/kaptaind-cli
```
