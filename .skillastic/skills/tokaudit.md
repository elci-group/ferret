---
name: tokaudit
description: >-
  Use `tokaudit` to audit agent session logs for token-efficiency problems.
  Run it after a long session, before a retrospective, or when you suspect a
  task burned more tokens than necessary. Prefer `--format json` when another
  tool or agent consumes the report.
---

# tokaudit

`tokaudit` parses agent session logs (e.g. `~/.kimi/logs/*.log`) and reports
quantitative token-wastage patterns. It is read-only and safe: it does not
modify logs, sessions, or the runtime.

## Quick decision flow

1. **Finished a long or expensive session?** `tokaudit --root ~/.kimi --issues-only`
2. **Want a machine-readable report?** `tokaudit --root ~/.kimi --format json`
3. **Checking a specific log file?** `tokaudit ~/.kimi/logs/kimi.log`
4. **Only care about high-waste patterns?** `tokaudit --root ~/.kimi --min-waste 1000000`

## When to call tokaudit

Call tokaudit when:

- A session felt unnecessarily long or expensive.
- You are doing a retrospective and want hard numbers on token use.
- You want to prove that a recent optimisation actually reduced bloat.
- You are choosing between tools/workflows and need an efficiency comparison.

Do **not** call tokaudit when:

- The session log does not exist yet or the task is still running.
- You need real-time token accounting (tokaudit is post-hoc).

## CLI at a glance

```text
tokaudit [OPTIONS] [FILES]
```

| Flag | Default | Meaning |
|------|---------|---------|
| `-r, --root <PATH>` | `.` | Directory to scan recursively for `.log` files |
| `-f, --format <text\|json>` | `text` | Output format |
| `--min-waste <N>` | `0` | Hide patterns with estimated waste below N tokens |
| `--issues-only` | off | Only list sessions that matched a wastage pattern |
| `FILES` | none | Specific log files; overrides `--root` |

## Common commands

```bash
# Human-readable efficiency report, sessions with issues only
tokaudit --root ~/.kimi --issues-only

# Full JSON report for programmatic analysis
tokaudit --root ~/.kimi --format json > /tmp/token_report.json

# Audit a single log file
tokaudit ~/.kimi/logs/kimi.log

# Focus on the costliest patterns
tokaudit --root ~/.kimi --min-waste 1000000
```

## Wastage patterns

| Pattern | Meaning | Typical fix |
|---------|---------|-------------|
| `BloatedContext` | LLM step carried >100k input tokens | Re-read only changed files, use narrower snapshots, summarise earlier |
| `LowYieldStep` | High input, very low output | Stop over-explaining to the model, batch tool calls, verify before reasoning |
| `ContextCompaction` | Session hit its context limit | Eject stale turns earlier, use dedicated tools instead of long context |
| `WireReplayBloat` | Persistent wire file >10 MB | Reduce per-turn payload, avoid bundling generated artefacts |
| `ExpensiveCheapTool` | Shell/Grep/Glob/ReadFile took >10s | Use bounded searches, narrower globs, avoid blocking commands |
| `ToolHeavyChurn` | Many LLM steps, little output | Write a deterministic script/tool instead of iterating with the LLM |

## JSON output schema

```json
{
  "total_sessions": 60,
  "total_llm_steps": 8625,
  "total_input_tokens": 935692213,
  "total_output_tokens": 4068007,
  "sessions_with_compactions": 23,
  "wire_too_large_count": 16,
  "patterns": [
    {
      "kind": "BloatedContext",
      "description": "...",
      "occurrences": 4593,
      "sessions_affected": 43,
      "estimated_wasted_input_tokens": 483347658,
      "evidence": ["..."]
    }
  ],
  "sessions": [...],
  "tool_totals": [...]
}
```

## Agent workflows

### Retrospective after a long task

```bash
tokaudit --root ~/.kimi --issues-only --format json
```

Inspect the patterns, then decide whether the root cause is:

- Over-broad context → use `bound` with tighter filters or read only diffs.
- Repetitive debugging → write a deterministic check with `deliver` next time.
- Slow tooling → replace broad `Shell`/`Grep` calls with scoped `Glob` + `Read`.

### Validate an optimisation

Run tokaudit before and after a workflow change, then compare:

```bash
tokaudit --root ~/.kimi --format json > before.json
# ... make changes ...
tokaudit --root ~/.kimi --format json > after.json
# diff the totals and pattern counts
```

## Anti-patterns

- **Running tokaudit on active logs.** Results are incomplete until the session
  finishes and the log is flushed.
- **Treating estimated waste as exact cost.** The waste numbers are heuristics
  (excess tokens above a healthy baseline), not billing data.
- **Ignoring cheap-tool slowness.** A Shell call that blocks for 60s can waste
  as much wall-clock time as an expensive LLM call.

## Safety

- Read-only: tokaudit never writes to the scanned logs.
- Local only: it does not send log contents anywhere.
- It will skip unreadable files and report the error to stderr.
