# Delegation: Hermes principles adapted to Zeus

Inspected the local Hermes checkout at `bf0203a794a07aac3ff764972ab9087baa4e4f4e`
and the Zeus working tree based on `c3d26fc255c12ba24f48719893a45120788c5fb0`.
Existing Zeus plugin work was present and preserved. Hermes was read-only.
This is a source comparison, not a live-model quality benchmark.

Both repositories' instructions and `floorplan/architecture.json` were read.
Floorplan located Hermes's tools/agent boundaries and Zeus's execution,
workspace, and verification boundaries; source establishes the behavior below.
The codebase-memory graph supplied Zeus symbols, call relationships and source
snippets. Its fresh fast Hermes index excluded `tools/`; coverage checks exposed
that gap, so the delegation implementations there were traced directly in source.
No absence claim is inferred from that graph gap.

## End-to-end comparison

Hermes paths below are relative to its repository root. Zeus paths link to this
repository. Symbols identify the inspected code even as line numbers move.

| Stage | Hermes source behavior | Zeus source behavior |
| --- | --- | --- |
| Planning and admission | `tools/delegate_tool.py::delegate_task`, `_build_children`, and `delegate_tool_tasks.py::_normalize_task_list` validate goals, routes and schemas before constructing children; individual unroutable tasks can be refused. | [`runtime.rs::run_agents`](../src/runtime.rs) parses the bounded planner response and validates every task and exact-file assignment before calling the scheduler. Any invalid task rejects the whole plan. |
| Context transfer | `_build_children` passes each task's `context` to `_build_child_agent`; `delegate_tool_progress.py::_build_child_system_prompt` includes it plus workspace guidance. Child conversation history is independent. | Before this change, workers received the original job context and a task of at most 4096 bytes, without a separate field for selected planner findings. **Implemented:** optional bounded coding-task context, passed only to its worker as untrusted user-message data. |
| Tools and ownership | `delegate_tool_toolsets.py::_resolve_child_toolsets` derives child toolsets from the parent and applies role restrictions. Depth-enabled orchestrators may delegate. `_ChildRun.seed_workspace` gives children separate cwd records; `_create_isolated_worktree` optionally creates a worktree and may fall back to the shared workspace. | [`runtime.rs::Host`](../src/runtime.rs) enforces read-only or exact-file authority at dispatch, intersected with user scope. [`workspace.rs`](../src/workspace.rs) provides a private candidate and digest-checked edits. Workers never receive verifier, shell, MCP, plugin execution or delegation tools. |
| Concurrency and limits | `delegate_tool_dispatch.py::_run_children_parallel` uses a bounded executor. `_build_child_agent` explicitly passes `iteration_budget=None` (fresh child budget). `delegate_tool_config.py` defaults to 10 children, depth 1, and no configured child timeout. `_ChildRun.wait_liveness_aware` treats a configured child timeout as inactivity, resetting it on progress. | [`agent.rs::run_tasks`](../src/agent.rs) provides at most three scoped threads with work-conserving admission. [`model.rs::complete`](../src/model.rs) charges attempts against one optional shared request budget. `Host::observe` accumulates provider-reported tokens; [`cancel.rs`](../src/cancel.rs) enforces a whole-job deadline and signal cancellation. Each worker also has bounded turns/tool calls. |
| Failure and cleanup | `_run_single_child` returns structured failure outcomes and runs `_ChildRun.cleanup` in `finally`. Cleanup releases registry entries, credential leases, tool resources and kernels. The parallel interrupt path uses `shutdown(wait=False)` and can abandon pending daemon workers; child close can be deferred until a timed-out future settles. | `run_tasks` closes admission after a published error, catches worker panics, and joins all admitted workers. Runtime worker outcomes and candidate edits remain audited; an infrastructure failure skips integration and policy training. State locks release through ownership cleanup. |
| Collection and verification | `_execute_and_aggregate` orders results by task index; `delegate_tool_results.py::_finalize_child_results` handles summary bounds, hooks and cost rollup. `_build_result_entry` distinguishes terminal status, errors and budget exhaustion; optional output-schema validation is not a fixed external code verifier. | `run_agents` appends worker reports in task order only after joins, then integrates and performs bounded repair. `check_verifier` uses the fixed external verifier; `run` persists proof before updating routing from eligible final outcomes. Model completion is not verification. |

The checked source overrides stale Hermes instruction prose: its fresh child
budget and 10-child/depth-1 defaults differ from the shared-budget/default-3/depth-2
descriptions in its instructions. These differences are not changes to make in
Zeus. Likewise, Zeus's older orchestration prose saying it had no job deadline
was stale; this change corrects that documentation, not the deadline mechanism.

## Selection

**Already implemented:** independent contexts, bounded concurrency, task-order
results, worker capability restrictions, explicit ownership, retained outcomes,
shared execution limits, cancellation checkpoints, joined integration and
external verification. No scheduler replacement is warranted.

**Selected gap:** a bounded, task-specific planner handoff. The planner can read
files and discover constraints, but previously could communicate them only by
packing them into the short goal or forcing workers to repeat discovery. A
separate context field carries selected facts, source references and unknowns
without replaying the whole planner transcript. This is the highest-value small
change supported by the inspected coding path: it improves the information
boundary while leaving the authority and scheduling boundaries intact.

**Other verified differences, not implemented:** Hermes exposes richer per-child
terminal status and parent-visible tool traces, plus result-summary budgeting.
Zeus already retains model/tool audits and fails the run on worker errors; a new
result format or recovery policy needs a separate concrete use case. These are
possible future work, not features added here.

**Does not fit Zeus:** recursive delegation; role-granted tool expansion;
inherited shell/MCP execution; fresh independent child request budgets;
asynchronous completion delivery; optional worktree isolation that falls back to
shared writes; returning while admitted coding workers remain alive. Those
behaviors serve Hermes's interactive assistant architecture and conflict with
Zeus's current coding-run guarantees. No Hermes code or new dependency was copied.

## Observable acceptance contract

Defined before implementation:

1. A planner can inspect a file and transfer selected findings to the assigned
   coding worker, including an exactly 8192-byte UTF-8 handoff. Sibling handoffs
   and the planner's full transcript are not replayed.
2. Context is untrusted data. A handoff claiming broader permissions does not
   enable verifier/MCP/shell/delegation tools or permit a sibling-file write.
3. An oversized later handoff rejects the entire plan before worker admission or
   candidate edits. Non-string and duplicate context fields are rejected too.
   These failures retain evidence but produce no policy reward.
4. Existing plans remain valid. Independent edits still join before integration,
   the fixed verifier checks the integrated candidate, and originals remain intact.

`tests/parallel_coding.rs` adds two real-CLI/local-HTTP regressions and extends the
malformed-plan cases. The handoff tests were run before the runtime change:
both failed because `context` was rejected by the old schema. They pass after the
change, including the accepted byte boundary and ASCII/multibyte overflow cases.
The successful fixture makes real scoped edits, exercises denied sibling writes,
checks retained planner evidence, and runs a fixed external Python verifier.

Validation on the local macOS checkout:

- `cargo test --locked`: **249 passed, 0 failed, 2 ignored**.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `cargo build --locked --release`: passed.
- Floorplan rebuilt with descriptions for all 64 code files, no unassigned
  files, and the runtime/test changes highlighted under Recent work.

Network-fixture and verifier tests ran outside the outer workspace sandbox;
the verifier's own isolation remained active. The ignored tests are the
61-second transport fixture and the opt-in installed REA/Go smoke test. Linux
end-to-end behavior and live inference were not tested by this change.

The behavior contract is documented in [ORCHESTRATION.md](ORCHESTRATION.md).
No claim is made that JSON quoting prevents prompt injection, that planner facts
are true, or that this improves live-model completion rates. Exact-file dispatch
remains the security boundary. Legacy string analysts do not gain a context
field, and the integrator still receives worker reports rather than automatic
planner-transcript inheritance. Provider token accounting is post-response and
cannot prevent already in-flight usage. Synchronous host calls must finish;
transport helpers may outlive cancelled waits within their transport bounds.
