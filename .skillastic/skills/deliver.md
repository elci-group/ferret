---
name: deliver
description: >-
  Use `deliver` to deterministically verify that a task's deliverables exist,
  contain no forbidden placeholders, and pass required build/test commands.
  Run it before declaring a task complete or before handing off to another
  agent. Prefer `--format json` when another tool consumes the report.
---

# deliver

`deliver` turns subjective "looks done" into a pass/fail report. It checks
files, runs commands, and optionally inspects git state. Use it as a gate
before you tell the user a task is finished.

## Quick decision flow

1. **Task produces files and should pass tests?** Write a `deliver.toml` and
   run `deliver --spec deliver.toml --strict`.
2. **Just need to confirm a few files exist?**
   `deliver --file src/lib.rs src/main.rs Cargo.toml --strict`.
3. **Consuming the result in another tool?** `deliver --spec deliver.toml --format json`.
4. **Need to gate a CI-like step?** Use `--strict`; it exits non-zero on failure.

## When to call deliver

Call deliver before marking work complete:

- After writing or editing code.
- After adding tests.
- Before ending a plan-mode session.
- Before telling the user "done".

Do **not** call deliver when:

- The task is exploratory and has no defined deliverable yet.
- You have not written any files or commands to validate.

## CLI at a glance

```text
deliver [OPTIONS]
```

| Flag | Default | Meaning |
|------|---------|---------|
| `-s, --spec <PATH>` | none | Path to TOML or JSON spec file |
| `--json <STRING>` | none | Inline JSON spec (overrides `--spec`) |
| `--file <PATH>...` | none | Quick file existence check; glob patterns supported |
| `-b, --base <PATH>` | `.` | Base directory for relative paths |
| `-f, --format <text\|json>` | `text` | Output format |
| `--strict` | off | Exit non-zero if any check fails |

## Spec file: TOML

Save as `deliver.toml` in the project root:

```toml
[[file]]
path = "src/lib.rs"
required = true
forbid_regex = ["TODO", "FIXME", "unimplemented!", "panic!"]

[[file]]
path = "src/main.rs"
required = true
forbid_regex = ["TODO", "FIXME"]

[[command]]
name = "cargo test"
cmd = ["cargo", "test"]
expect_exit = 0
timeout_secs = 300
stdout_contains = ["test result: ok"]

[[command]]
name = "cargo build"
cmd = ["cargo", "build"]
expect_exit = 0
timeout_secs = 300

[git]
no_uncommitted_changes = false
no_untracked_files = false
```

### File check options

| Option | Meaning |
|--------|---------|
| `path` | Relative path from `--base` |
| `required` | Fail if the file is missing |
| `min_size_bytes` | Minimum file size |
| `max_size_bytes` | Maximum file size |
| `forbid_regex` | Each regex must NOT match the file content |
| `require_regex` | Each regex MUST match the file content |
| `require_line_count` | File must have at least this many lines |

### Command check options

| Option | Meaning |
|--------|---------|
| `name` | Human-readable check name |
| `cmd` | String or array of command + args |
| `cwd` | Working directory for the command |
| `env` | Extra environment variables |
| `expect_exit` | Expected exit code |
| `timeout_secs` | Kill the command after this many seconds |
| `stdout_contains` | Required stdout substrings |
| `stderr_contains` | Required stderr substrings |

## Common commands

```bash
# Validate a project against deliver.toml
deliver --spec deliver.toml --strict

# Quick file check
deliver --file README.md Cargo.toml src/lib.rs --strict

# Glob file check
deliver --file "src/**/*.rs" --strict

# JSON report for another agent
deliver --spec deliver.toml --format json
```

## JSON output schema

```json
{
  "pass": true,
  "duration_ms": 2450,
  "checks": [
    {
      "name": "file: src/lib.rs",
      "pass": true,
      "kind": "file",
      "message": "OK (24702 bytes)"
    },
    {
      "name": "cargo test",
      "pass": true,
      "kind": "command",
      "message": "OK (exit 0, 179 ms)"
    }
  ]
}
```

## Agent workflows

### Gate before "done"

After implementing a change:

```bash
deliver --spec deliver.toml --strict
```

If it fails, fix the failures and rerun. Do not report completion while the
report is failing.

### Define a spec at the start of a task

When planning, write `deliver.toml` first. It becomes the acceptance contract:

```toml
[[file]]
path = "src/feature.rs"
forbid_regex = ["TODO", "FIXME"]

[[command]]
name = "tests"
cmd = ["cargo", "test", "feature"]
stdout_contains = ["test result: ok"]
```

### Hand off between agents

Before delegating, run deliver and include the JSON report in the subagent
prompt so the next agent knows the current state.

## Anti-patterns

- **Writing a spec after the work.** The spec should define done, not describe
  what you already did.
- **Omitting `--strict` in scripts.** Without `--strict`, deliver always exits
  0 and the gate is meaningless.
- **Overly strict regex checks.** Do not forbid every possible imperfection;
  check only what the task actually requires.
- **Long timeouts.** Keep `timeout_secs` tight; a hanging command is a failed
  deliverable.

## Safety

- deliver only reads files and runs the commands you specify.
- It kills commands that exceed `timeout_secs`.
- It does not auto-commit, push, or modify git state.
