# User-granted autonomy

Zeus can plan, delegate file-scoped coding work, integrate results and repair a
candidate after verification failures without interactive approval for every
file edit. Authority comes from the user's job file and CLI flags, not from the
planner, repository guidance or a worker's role name.

Add these fields to an existing job:

```json
{
  "strategy": "parallel",
  "max_turns": 12,
  "model_timeout": 180,
  "autonomy": {
    "write_paths": ["src/parser.rs", "tests/parser.rs"],
    "max_repairs": 2,
    "max_model_calls": 40
  }
}
```

This is a job fragment, not a complete connection configuration. The usual
source, state directory, model endpoint and model fields are still required.

## Controls

- `write_paths`: an exact relative-file allowlist, up to 128 paths, enforced on
  both workers and the integration/repair agent. Missing or `null` preserves
  whole-candidate editing. `[]` removes model file-write/edit capabilities.
  Directories, globs and prefixes do not grant implicit descendants. Parent
  traversal, absolute paths, duplicates and ancestor/descendant claims are
  rejected; normal workspace secret and symlink guards still apply.
- `max_repairs`: 0–8, default 2. This permits one initial integration attempt
  plus the specified number of retries after a completed failing verifier.
  Zero means no autonomous repair retries. A verified result stops early.
- `max_model_calls`: optional 1–1024 attempted inference requests shared across
  the planner, all workers, integration and repairs. Cloned clients share an
  atomic budget. Concurrent workers cannot each spend the full budget. Failed
  requests consume an admitted slot; there is no automatic refund or retry.
  Omission preserves existing per-agent bounds without an additional global cap.
- Existing `max_turns` and `model_timeout` still bound individual agent loops and
  model requests. The model-call cap is not a token/dollar cap or a whole-job
  deadline; provider token usage and pricing remain external.

Start a configured run using the existing approvals:

```sh
./target/release/zeus run path/to/job.json --allow-network --allow-exec
```

`--allow-exec` is needed only when the job has a verifier. It authorizes the
fixed, trusted external verifier, not arbitrary model-selected commands.
The verifier can write anywhere inside its candidate sandbox, independently of
`write_paths`: that list restricts model file tools, not trusted verification
code. For an inspection-only run with no candidate mutation, use `write_paths: []`
and no verifier or MCP configuration.

Scoped autonomy rejects MCP configuration even when `--allow-mcp` is supplied:
MCP tools execute on the host and cannot honestly be constrained by the file
allowlist. Whole-candidate mode retains the separate existing MCP approval path.

## Delegation and failure

Planner file assignments must be subsets of the user allowlist and globally
disjoint. They cannot enlarge the user's scope. Structured coding plans support
2–12 independent tasks with at most three active workers. Workers can inspect
files but change only their assignments; integration starts after all workers
finish. For code that cannot be safely split, the planner can use read-only
investigation tasks instead.

Model-budget exhaustion is an infrastructure error, not a failed verification
reward. It stops new work, retains the candidate/audit/error report and does not
train the router. In-flight siblings finish under existing request limits.
Verified/failed reports and infrastructure-error reports record the effective
autonomy settings. Earlier candidate writes are not rolled back on failure.

This does not grant shell execution, edits to the original source tree,
automatic deployment, recursive delegation, unattended credential access or
restart-resumable execution. The candidate remains a reviewable artifact; there
is no automatic promotion to the source repository.

## Proof

```sh
cargo test --locked --test autonomy_config --test autonomy_runtime --test model_budget
```

These use real CLI runs against deterministic localhost protocol fixtures and
real external verifier subprocesses. They check file-scope denial, read-only
capabilities, delegation ceilings, exact repair counts, shared concurrent
request budgets, retained error evidence and unchanged original files. They
prove runtime enforcement, not live-model coding quality.
