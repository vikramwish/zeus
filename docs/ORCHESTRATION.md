# Bounded orchestration

Zeus uses native scoped-thread, work-conserving agent execution. It does not
launch Hermes or Prime, import their credentials, or introduce a service or a
background worker registry.

## API and lifecycle

`agent::run_parallel(client, host, system, tasks, max_turns)` remains compatible
and delegates to the additive API with a worker limit of three:

```rust
pub fn run_parallel_with_limit(
    client: &ChatClient,
    host: &dyn ToolHost,
    system: &str,
    tasks: &[String],
    max_turns: usize,
    max_workers: usize,
) -> Result<Vec<AgentResult>, String>
```

- `max_workers` must be `1..=3`, including for an empty task list. Invalid limits
  fail before starting workers or calling the host/model. Valid empty input
  returns an empty result, preserving the original API's behavior.
- At most `max_workers` scoped OS threads consume a FIFO task queue. Each task
  has an independent `run` context, message history, turn and tool-call budget.
  A free worker claims the next task immediately; it does not wait for slower
  siblings to finish a batch. The generic API shares its supplied host; the
  runtime uses the same internal scheduler with task-specific capability hosts.
- Results preserve **input task order**, not completion order. Each task index
  has one result slot and is claimed once. No automatic retry or replay occurs.
- Admission and error publication are serialized under one short mutex. Model
  requests, tool calls and agent reasoning run outside this lock. Once a failure
  is published, no new task can be claimed. Tasks admitted just before failure
  may still run and are joined; this is not forcible cancellation.
- Every started thread is joined, including after task errors, unwinding panics,
  or thread-launch failures. A task panic becomes `agent worker panicked` and
  closes admission. The first task error in input order is returned after joins;
  partial successful results are not presented as a completed run.

This is a bounded, work-conserving worker queue using Rust's standard library,
not a claim of a universally fastest scheduler. Zeus currently uses blocking
HTTP model calls and a small concurrency bound. Removing batch barriers avoids
leaving free slots idle behind a slow request without introducing an async
runtime, per-worker stealing deques or another dependency. File ownership and
atomic writes remain separate safety boundaries; faster scheduling cannot grant
permissions.

Joining is not forcible interruption: an in-flight synchronous `ToolHost` call
must return. The runtime already enforces `max_job_seconds`, cooperative signal
cancellation, shared model-call limits, and provider-reported token accounting.
Model-request helper threads may outlive a cancelled wait until their transport
deadline; this does not detach the admitted coding workers or give helpers file
tools. There is no detached-work recovery. Process aborts are not caught.

## Authority and protocol invariants

`ToolHost` remains the actual capability boundary. The generic parallel API
neither constructs a privileged host nor grants write, execution, MCP or network
approval. The job runtime supplies read-only or explicitly file-scoped coding
hosts. Callers supplying another host retain responsibility for its authority
and concurrent-call safety.

The single-agent loop is unchanged: entire tool batches are validated before
any dispatch, mixed allowed/denied batches execute no calls, IDs and budgets are
consumed consistently, and only a subsequent model turn may request fresh calls.
Malformed protocol and host/infrastructure errors remain fatal. No model or
repository text becomes approval. Orchestration does not update routing rewards.

## Writable task plans

The job planner can return `{"tasks":[{"task":"implement A","write_paths":["a.rs"]},{"task":"implement B","write_paths":["b.rs"]}]}`. All-coding plans allow 2–12 tasks; legacy string or mixed analysis/coding plans retain the 2–3 bound. At most three workers run concurrently through the same work-conserving scheduler.

Every coding task needs a nonempty exact-file scope. The full plan is validated before dispatch. Duplicate, ancestor/descendant and ASCII case-alias claims are rejected globally. Paths must use canonical relative ASCII spelling without traversal or repeated separators and must not name symlinks or directories. Unicode-named files remain supported by the integrator, but cannot currently be assigned to parallel coding workers. Existing workspace secret/path guards still apply to every write. If the user configured an autonomy allowlist, every assignment must be its subset.

Workers receive read/search/evidence tools plus scoped write/edit; not verifier, MCP, shell or recursive delegation. Authority is checked at dispatch, not inferred from role text. Worker IDs and scopes are attached to tool audits and outcomes. All admitted workers join before integration and final verification. Infrastructure failure retains partial candidate edits and successful sibling outcomes, skips integration, and does not train the router.

### Planner-to-worker context

Coding task objects optionally include a `context` string for selected findings
from the planner's investigation, separate from the task's goal:

```json
{
  "tasks": [
    {
      "task": "Repair persistence",
      "write_paths": ["store.rs"],
      "context": "The save path returns before committing the new state. Recheck the current file before editing."
    },
    {"task": "Update the usage notes", "write_paths": ["README.md"]}
  ]
}
```

The decoded string is limited to **8192 UTF-8 bytes per task**. Omitted or empty
context adds nothing; `null`, non-strings, duplicate fields, and oversized strings
are rejected. The existing 4096-byte task limit and 64 KiB complete planner
response limit still apply. Every task and scope is validated before admission,
so an invalid handoff on a later task starts no earlier workers.

Only the assigned worker receives the handoff, as a labelled JSON string in its
initial user message, never in the system prompt. It is untrusted planner text,
not permission, validated evidence, or a replacement for inspecting current
files. It cannot expand file scopes, enable tools, change budgets, or alter the
verifier. No full planner transcript or sibling handoff is copied. Workers still
receive the original job, initial inventory, baseline result, autonomy limits,
and root project guidance. Legacy string analysis tasks remain unchanged.

The existing `planner` audit event retains the complete plan and handoffs.
Integration still receives task-order worker reports after all admitted workers
join; it does not automatically receive the planner handoffs. A worker may cite
relevant findings in its report. Claims and source IDs in context are not
automatically registered or validated as findings.

This adapts Hermes's explicit goal/context separation without inheriting its
system-prompt placement, tool surface, budgets, or lifecycle. See the
[source comparison and acceptance evidence](DELEGATION-COMPARISON.md).

Run `cargo test --locked --test parallel_coding --test planner_contract --test workspace_concurrency` for actual CLI protocol-fixture coverage of writable overlap, scope denial, invalid plans, source preservation and post-join verification.

## Source inspection and licensing

Inspected local source before implementation (behavior references, not copied
code):

- Hermes revision `bf0203a794a07aac3ff764972ab9087baa4e4f4e`, MIT license,
  copyright Nous Research. `tools/delegate_tool.py::_build_children` keeps task
  indices and derives child tools through the parent-owned construction path.
  `tools/delegate_tool_dispatch.py::_run_children_parallel` bounds concurrency,
  associates futures with task indices and sorts final results by index.
  Its interrupt path can abandon daemon workers; Zeus intentionally does **not**
  adopt that behavior and keeps complete scoped joining.
- Prime revision `c4c238ca025e382d96fb5f37e6328ca77fb82ef3`, MIT license,
  copyright Prime Intellect Ltd.; its license also credits the original
  TypeScript product by Mario Zechner.
  `crates/pa-agent/src/agent_loop/entry.rs::run_agent_loop` constructs per-run
  context and emits lifecycle events. `agent_loop/tool_call.rs` validates tool
  lookup/arguments, retains call IDs, races execution with abort and uses cleanup
  guards for background update work. Zeus reuses its existing independent
  contexts, validation and scoped ownership instead of porting that async stack.

## Deterministic proof

Run:

```sh
cargo test --test parallel_execution --test agent_protocol
```

`parallel_execution` uses a localhost HTTP fixture and in-process gated host,
not a live inference provider or external tool. It exercises limits one, two and
three, the legacy default, seven-task result association, invalid limits and
empty input. The server withholds responses until all configured slots have
submitted requests, proving overlap without throughput timing heuristics.
Another test holds task zero blocked while a second worker completes tasks one
through four, proving that the queue refills without a batch barrier. Results
remain in input order despite out-of-order completion.

The failure fixture blocks all three host calls, releases a controlled error or
panic, and keeps siblings blocked while checking that the API has not returned.
It then releases and observes each sibling, asserts deterministic error selection
(including when a later-index task fails first), and verifies that only the first
three tasks ever entered host schema discovery. Queued tasks are not merely
hidden from the result. Channel/socket timeouts are watchdogs; release order is
explicit, not driven by sleeps. `agent_protocol` separately retains the existing
whole-batch validation, denied-capability recovery and budget regression proofs.

These tests establish orchestration/protocol behavior, not model quality or
live-provider compatibility.

### Bounded worker result handoffs

After every admitted worker joins successfully, the runtime persists each full
`AgentResult` in a `worker` audit event, then sends the integrator a JSON
projection in task order: zero-based `index`, `audit_event`, `tool_calls`,
`original_bytes`, `omitted_bytes`, and `text`. These are reports, not verification
or permission grants. Worker errors still stop integration.

Each `text` is limited to 8192 UTF-8 bytes. Longer text keeps approximately three
quarters from the beginning and one quarter from the end, with a middle-omission
marker inside that limit. Cuts respect character boundaries; `omitted_bytes`
counts removed original bytes, excluding the inserted marker. Short and empty
reports are unchanged. The planner admits at most 12 workers, bounding retained
report text to 96 KiB (JSON escaping adds overhead). Existing total model-context
limits still apply to the entire prompt, including the original task.

The full audit is retained for review; audit IDs do not grant candidate tools
access to audit files. The integrator must inspect candidate files to resolve
omitted details. This deliberately provides no new retrieval tool, model-based
summarizer, partial-success recovery, or adaptive token budget. Truncation can
omit useful findings; the fixed external verifier remains the acceptance gate.
