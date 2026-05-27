# Distributed Load Testing Plan

This document is the implementation guide for turning lorust from a single-node
load generator into a distributed load testing system.

The goal is not for Codex to generate the whole distributed system in one pass.
The goal is for the project owner to hand-code most of the design and learn the
distributed systems pieces directly, with Codex acting as a guide, reviewer, and
pairing assistant. Each step below should be small enough to implement, test,
review, and commit independently.

## Principles

- Keep the single-node executor usable at every step.
- Prefer explicit data contracts over implicit file formats.
- Make workers simple and deterministic.
- Do not stream every request metric to the supervisor by default.
- Preserve raw metrics when useful, but support compact summaries for large runs.
- Fail clearly when distributed runs are partial or inconsistent.
- Commit every meaningful step separately, with checks passing for each commit.

## Target Architecture

- Supervisor:
  - Owns the run plan, worker list, run ID, global rate/count/duration, and final report.
  - Splits work across workers.
  - Coordinates prepare/start/stop.
  - Collects worker summaries and optional raw metrics.
  - Merges metrics and writes final output.

- Worker:
  - Runs as a long-lived process.
  - Accepts prepared run plans.
  - Executes its assigned local load shard.
  - Spools metrics locally.
  - Reports status, summary, and optional raw metrics.

- Shared executor:
  - The current local load generator remains the core execution engine.
  - CLI, supervisor, and worker modes should all build a run plan and hand it to
    the same executor path where possible.

## Phase 0: Local Prerequisites

These are the minimum local changes needed before distributed work starts.

- [x] Add simple CLI mode for direct HTTP tests.
- [x] Add sortable per-request start timestamp.
- [ ] Add metric identity fields:
  - [ ] `run_id`
  - [ ] `worker_id`
  - [ ] `task_id`
  - [ ] `sequence`
- [ ] Add CLI flags for local distributed-compatible metadata:
  - [ ] `--run-id`
  - [ ] `--worker-id`
- [ ] Make metric sorting stable by:
  - [ ] `started_at_nanos`
  - [ ] `worker_id`
  - [ ] `task_id`
  - [ ] `sequence`
- [ ] Add tests or smoke checks proving metric identity fields are present.

## Phase 1: Define Contracts

- [ ] Create a module for distributed contracts.
- [ ] Define `RunId` and `WorkerId` types or aliases.
- [ ] Define a `RunPlan` struct containing:
  - [ ] `run_id`
  - [ ] target config or flow config
  - [ ] mode: request count or duration
  - [ ] rate
  - [ ] timeout
  - [ ] metrics mode
  - [ ] output mode
- [ ] Define `WorkerPlan` containing:
  - [ ] `run_id`
  - [ ] `worker_id`
  - [ ] assigned request count or duration
  - [ ] assigned rate
  - [ ] flow/functions to execute
  - [ ] planned start time
- [ ] Define `WorkerStatus` containing:
  - [ ] state: idle, prepared, running, completed, failed, stopped
  - [ ] current counts
  - [ ] last error
  - [ ] start/end timestamps
- [ ] Define `WorkerSummary` containing:
  - [ ] total requests
  - [ ] success count
  - [ ] failure count
  - [ ] error rate
  - [ ] latency histogram or percentile data
  - [ ] bytes uploaded/downloaded
- [ ] Add JSON serialization tests for all contracts.

## Phase 2: Local Summary Model

- [ ] Extract summary generation out of `load_gen` printing code.
- [ ] Create a reusable `HttpMetricSummary` struct.
- [ ] Compute summary from `Vec<HttpMetric>`.
- [ ] Print summary from the struct.
- [ ] Serialize summary as JSON.
- [ ] Add summary output mode:
  - [ ] raw metrics only
  - [ ] summary only
  - [ ] both
- [ ] Keep existing default behavior compatible for local CLI users.

## Phase 3: Raw Metrics Output Format

- [ ] Keep JSON array output for small runs.
- [ ] Add NDJSON output for large runs.
- [ ] Add a metrics output enum:
  - [ ] `json`
  - [ ] `ndjson`
- [ ] Add one writer abstraction used by local, worker, and supervisor modes.
- [ ] Make raw metrics writes streamable so large tests do not require all metrics
  in memory.
- [ ] Add smoke tests that parse produced JSON and NDJSON.

## Phase 4: Worker Process

- [ ] Add `lorust worker --listen <addr> --worker-id <id>`.
- [ ] Add a small HTTP server dependency after reviewing options.
- [ ] Implement `GET /health`.
- [ ] Implement `POST /prepare`.
- [ ] Implement `POST /start`.
- [ ] Implement `POST /stop`.
- [ ] Implement `GET /status`.
- [ ] Implement `GET /summary`.
- [ ] Implement `GET /metrics`.
- [ ] Store worker state in a single state object protected by an async lock.
- [ ] Reject invalid state transitions:
  - [ ] start without prepare
  - [ ] prepare while running
  - [ ] metrics before completion, unless explicitly requested
- [ ] Add local integration smoke test with one worker process.

## Phase 5: Supervisor Process

- [ ] Add `lorust supervise`.
- [ ] Accept worker addresses:
  - [ ] repeated `--worker http://host:port`
  - [ ] optional `weight`
- [ ] Reuse the simple `http` command shape under supervisor mode.
- [ ] Validate all workers with `/health`.
- [ ] Split global request count by worker weights.
- [ ] Split global rate by worker weights.
- [ ] Send `/prepare` to every worker.
- [ ] Start workers with a shared future start timestamp.
- [ ] Poll `/status`.
- [ ] Stop all workers if one worker fails, unless partial mode is enabled.
- [ ] Collect summaries from every worker.
- [ ] Optionally collect raw metrics from every worker.
- [ ] Merge worker summaries into a global summary.
- [ ] Merge raw metrics and sort by:
  - [ ] `started_at_nanos`
  - [ ] `worker_id`
  - [ ] `task_id`
  - [ ] `sequence`

## Phase 6: Failure Handling

- [ ] Define worker failure states clearly.
- [ ] Add supervisor timeout for prepare/start/status/metrics calls.
- [ ] Save partial results when a run fails.
- [ ] Mark final report as partial when any worker is missing.
- [ ] Add a non-zero exit status when:
  - [ ] worker fails
  - [ ] thresholds fail
  - [ ] run is partial and partial mode is not allowed
- [ ] Add `--allow-partial` for exploratory runs.

## Phase 7: Thresholds And CI Use

- [ ] Add threshold config:
  - [ ] max error rate
  - [ ] max p95 latency
  - [ ] max p99 latency
  - [ ] min requests/sec
- [ ] Evaluate thresholds against local and distributed summaries.
- [ ] Print threshold pass/fail output.
- [ ] Return non-zero exit code when thresholds fail.
- [ ] Add examples for CI usage.

## Phase 8: Clock And Ordering Notes

- [ ] Document that cross-machine timeline ordering depends on clock sync.
- [ ] Recommend NTP or chrony for distributed runs.
- [ ] Keep `started_at_nanos` for approximate global timeline sorting.
- [ ] Keep `worker_id`, `task_id`, and `sequence` for deterministic per-worker
  ordering even when clocks skew.
- [ ] Consider adding worker clock offset reporting later.

## Phase 9: Load Modes

- [ ] Add duration mode:
  - [ ] run for `--duration 5m`
  - [ ] split duration equally across workers
- [ ] Add constant concurrency mode:
  - [ ] keep N virtual users active
  - [ ] replace finished users until duration ends
- [ ] Add ramp mode:
  - [ ] ramp from rate A to rate B over duration
  - [ ] split ramp by worker weights
- [ ] Keep request-count mode as the simplest default.

## Phase 10: Learning Workflow

- [ ] Pick one todo item at a time.
- [ ] Write the intended API or data shape before coding.
- [ ] Implement the smallest working version.
- [ ] Run formatter, compile check, tests, and one smoke run.
- [ ] Commit the working checkpoint.
- [ ] Ask Codex for review when the behavior or contract is uncertain.
- [ ] Prefer Codex explanations and code review over full code generation for the
  distributed system.

## Immediate Next Step

Implement Phase 0 metric identity fields in the local single-node app. This gives
future distributed aggregation the minimum stable data needed before any networked
worker/supervisor code exists.
