---
name: verify-chain
description: >-
  Use `verify-chain` to audit an agent's final output against the trace of
  sources, diffs, test results, and artifacts that produced it. Reach for it
  when the model claims code is fixed, tests pass, citations exist, or a task is
  complete, and you need evidence-based verification.
---

# verify-chain

`verify-chain` is an output claim auditor. It takes a model's final output plus
the agentic trace that produced it and checks whether the claims actually follow
from the evidence.

## Quick decision flow

1. **Verify a final output against a trace:**
   `verify-chain --trace trace.json --output output.txt`
2. **Also run claimed tests:**
   `verify-chain --trace trace.json --output output.txt --execute-tests`
3. **Fetch cited URLs:**
   `verify-chain --trace trace.json --output output.txt --fetch-urls`

## When to call verify-chain

Call `verify-chain` when:

- A model claims a bug is fixed or tests pass.
- A research claim needs citation support.
- A task-completion claim needs an external artifact (file, API result,
  screenshot).
- You want to score output confidence before presenting it to a user.

## Trace format

The trace is a JSON array of `TraceEvent` objects:

```json
[
  {
    "id": "...",
    "actor": "tool",
    "timestamp": "2026-07-17T00:00:00Z",
    "event_type": "tool_result",
    "payload": {
      "tool": "execute",
      "command": "cargo test",
      "exit_code": 0,
      "output": "test ok"
    }
  }
]
```

## CLI

```bash
verify-chain --trace trace.json --output output.txt
verify-chain --trace trace.json --output output.txt --execute-tests --fetch-urls
```

## Safety notes

- By default `verify-chain` does not execute commands or fetch URLs. Enable
  those flags only in a sandboxed environment.
- Command execution restricts shell metacharacters and only allows known test
  runners by default.
