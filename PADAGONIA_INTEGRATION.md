# PADAGONIA Integration Roadmap

See the enterprise directives in `/home/sal/padagonia/docs/enterprise-integration-directives.md`.

## Modules

- `provider_registry`: persist provider/model capabilities, endpoints, versions,
  and health observations.
- `benchmark_ingest`: store reproducible workloads, hardware, latency, recall,
  cost, and confidence intervals with content hashes.
- `scoring_reader`: retrieve historical performance and freshness-bounded data.
- `decision_explainer`: link each routing decision to observations and policy
  versions.
- `fallback_queue`: continue locally when Padagonia is unavailable and replay
  idempotently later.

## Acceptance gates

Stale observations are explicit, benchmark reruns are reproducible, secrets are
never stored, and routing remains safe when the graph is unavailable.
