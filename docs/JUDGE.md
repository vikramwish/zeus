# Zeus Judge: experimental shadow routing

Judge is a native Rust, local outcome scorer for the existing `Direct` and
`Parallel` strategies. It records an experimental ranking or a typed abstention.
It **cannot change execution**: explicit user routing or the existing UCB policy
still selects the strategy. There is no active-model routing switch.

This is the first implementation of the proposed typed decision model, not a
recreation of Jev's model or training method. It ships a linear inference contract
and evidence collection, **not trained weights, a trainer, a text encoder or
calibrated probabilities**. Deterministic fixture weights test mechanics only.

## Enable collection

Add this to an existing job:

```json
"shadow_judge": {}
```

The run records its pre-execution state and a `no_model` abstention. To evaluate
an independently fitted artifact, use:

```json
"shadow_judge": {"model_path": "judge-model.json"}
```

Relative paths resolve beside the job file. Omission or `null` disables Judge
and creates no extra records. There are no additional network requests, model
calls, tool permissions or dependencies. Artifact errors abstain without
blocking the normal run. Evidence persistence errors still fail closed.

Each enabled run retains:

- `shadow-decision.json`: feature vector, domain digest, actual selected strategy,
  routing source, experimental predictions, ranking/abstention, artifact digest
  and version. Written before candidate creation and execution.
- `shadow-model.json`: exact bytes of an accepted artifact, when supplied.
- `shadow-outcome.json`: reference to the decision and immutable report/error,
  actual executed strategy, status, verifier label, elapsed seconds, and reported
  provider usage. No outcome is invented for the unexecuted strategy.

`verifier_success` is true only for `verified`, false only for `failed`, and null
for infrastructure errors, cancellation, unverified runs and an already-passing
baseline. Infrastructure failures never update routing rewards. Elapsed time
covers candidate preparation and execution, excluding artifact loading and
outcome persistence. Provider usage completeness is explicitly `unknown`:
missing or partial provider usage must not become a zero-cost training label.
A process crash can leave a decision with no outcome; exclude it from training.

## Artifact version 1

A UTF-8 JSON object, at most 64 KiB, loaded from a regular file. Unix final-path
symlinks and special files are rejected. Unknown/duplicate fields, incompatible
schemas, malformed arrays and invalid numbers abstain. The artifact contains:

| Field | Contract |
| --- | --- |
| `schema_version` | `1` |
| `feature_schema` | `"job_limits_v1"` |
| `model_version` | Nonempty string, at most 128 UTF-8 bytes |
| `domain_sha256` | Exact digest from a collected decision's state |
| `feature_min`, `feature_max` | Eight finite lower/upper support bounds in [0,1] |
| `direct`, `parallel` | Independent outcome heads, described below |

Each head contains `training_samples` (unsigned integer), `success_logit`,
`log1p_tokens`, and `log1p_seconds`. Each weight array has nine finite values:
an intercept followed by eight feature weights. Absolute coefficients are
limited to 100. These are data, never executable model code.

Feature order and normalization are fixed:

| Feature | Value |
| --- | --- |
| task_bytes | UTF-8 task length / 32768 |
| max_turns | Turn limit / 32 |
| max_repairs | Repair limit / 8 |
| max_model_calls | Shared call limit / 1024; 0 when absent |
| has_model_call_limit | 1 when configured, otherwise 0 |
| log_token_limit | ln(1 + token limit) / ln(1 + u64::MAX); 0 when absent |
| has_token_limit | 1 when configured, otherwise 0 |
| job_seconds | Whole-job deadline / 86400 |

The domain digest binds the context, endpoint, model identifier, protocol and
verifier argument vector. It does **not** fingerprint model weights, verifier
file contents or repository semantics. Re-evaluate artifacts when those change.
Feature extraction uses only the original job, never final reports or verifier
outcomes. The schema stores names as well as values to support offline review.

For each strategy, inference applies sigmoid to the success head and `expm1`
to the token/time heads. Cost-head dot products outside [0,30] abstain rather
than silently clamp. Success probabilities are independent: both strategies
can have high or low predicted success. They are not a softmax over actions.

Ranking uses predicted success only. Both heads must declare at least 30
training samples, all features must lie in the declared support box, a verifier
must be configured, and predicted success must differ by at least 0.05.
Otherwise Judge abstains with a typed reason. These thresholds are conservative
engineering defaults for shadow comparison, **not statistical guarantees**.
Cost/time predictions are retained for evaluation, not used as decision weights.
Every record labels probabilities `uncalibrated`, including ranked results.
Training counts and support bounds are artifact-author assertions, not certified
provenance. Bounding boxes do not detect every distribution shift.

## Acceptance and tests

Criteria defined before implementation:

1. Enabling shadow scoring never changes selected strategy, call count, tool
   authority, candidate isolation, verifier selection or UCB reward updates.
2. Missing/invalid artifacts produce explicit abstentions. Resource use is
   bounded; malformed weights cannot become executable actions.
3. Features and the exact accepted artifact are retained before execution;
   outcomes link to proof afterward. Only the executed strategy is labelled.
4. Unverified, already-verified and infrastructure outcomes have no success
   training label. Missing usage is never asserted to be complete.

`tests/job_limits.rs` runs the actual CLI against local HTTP fixtures and an
external verifier: no-model collection, a parallel-preferring scorer with actual
direct execution (UCB and explicit user routing), and infrastructure failure.
The no-model regression failed before implementation because the job schema
rejected `shadow_judge`; it passes afterward. `tests/shadow_judge.rs` checks
numeric predictions, independent probabilities, missing support, insufficient
samples, ambiguous scores, domain mismatch, schema/size errors and symlinks.
Existing execution and ownership tests remain the authority regression suite.

## Next stages — not implemented

Fit the heads offline from eligible observed outcomes. Split by repository and
time, keep training and calibration data separate, and evaluate Brier score,
reliability plots, decision coverage/error, completion rates and costs. Training
on selected-action outcomes has selection bias: don't label unchosen actions or
claim counterfactual improvement from shadow logs. Paired benchmarks in separate
private candidates are needed for strategy comparisons.

A frozen text encoder, richer repository features, learned calibration,
validated abstention thresholds and cost-aware utility can follow those results.
Automatic routing must remain disabled until held-out evidence supports it.
None of these future stages can relax Rust-enforced permissions, shared budgets,
joined workers, fixed verification, private candidates or retained evidence.

Validation on macOS: `cargo test --locked` passed 261 tests (0 failures,
2 existing ignored tests). Formatting, Clippy with warnings denied and the
release build passed. Floorplan covers all 66 code files, with no unassigned
files or reported health findings. Linux end-to-end execution and live-model
prediction quality were not evaluated.
