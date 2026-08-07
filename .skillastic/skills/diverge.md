---
name: diverge
description: >-
  Use `diverge` to run parallel agents with explicitly contradictory frames
  (optimist, skeptic, security reviewer, minimal-change advocate) and surface
  consensus or conflict. Reach for it when a multi-agent synthesis might
  groupthink around a wrong answer.
---

# diverge

`diverge` is a multi-agent consensus breaker. It runs several agents with
divergent system prompts, collects their answers and evidence, and either
synthesizes a consensus or surfaces a conflict for human review.

## Quick decision flow

1. **Run four default frames:** `diverge --question "should we deploy?"`
2. **Adjust consensus threshold:** `diverge --question "..." --threshold 0.5`
3. **Use fewer agents:** `diverge --question "..." --agents 2`

## When to call diverge

Call `diverge` when:

- A complex decision benefits from multiple perspectives.
- You suspect parallel agents might validate each other's mistakes.
- A security-sensitive action needs explicit challenge (skeptic, security
  reviewer frames).
- You want to avoid "averaging away" a minority view that carries evidence.

## Frames

Built-in frames:

- `optimist` — favours alignment and upside
- `skeptic` — questions assumptions and highlights risks
- `security_reviewer` — prioritises least privilege and threats
- `minimal_change_advocate` — prefers the smallest diff

## CLI

```bash
diverge --question "should we deploy the new feature?"
diverge --question "..." --agents 4 --threshold 0.66
```

## Output

JSON with `"kind": "consensus"` or `"kind": "conflict"`. In conflict mode,
`escalated` is true and the full set of agent responses is included.

## Safety notes

- The built-in simulator produces deterministic stub responses. For real use,
  provide a callback that calls your LLM with each frame's system prompt.
- `diverge` makes failure visible; it does not automatically block execution.
