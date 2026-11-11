# Zeus: implementation contract

Independent Rust runtime, not a wrapper around Hermes/Codex/Prime. No claim of feature parity or superior model intelligence. Broad target support comes from evidence-producing analysis tools, not a promise of universal source recovery.

## Prior source inspection (before implementation)

- Hermes c419cf7c864ac0ee511dbd9f7091a8bba0b567ee: tools/delegate_tool.py owns child isolation, limited inherited tools, depth/budget enforcement, and results. Reproduce those invariants, not its implementation.
- Prime c6d4da9421e6087e3fc1a763e947cf37153cd76f: crates/pa-agent/src/agent_loop/entry.rs constructs a per-run context and emits lifecycle events; tool_call.rs validates lookup/arguments, handles abort, and preserves tool call IDs. Reproduce bounded execution and correct transcript ordering.
- Source inspection only; not benchmarked against either product. No upstream source copied.

## Behavior and ownership

Job JSON -> config validation -> exclusive state lock -> immutable source inventory -> disposable candidate copy -> model-planned independent tasks with validated file ownership -> up to three scoped coding/read-only worker loops -> joined results -> one integration implementer -> user-supplied external verifier -> bounded repair -> evidence/report -> routing update.

- model.rs: validated HTTP endpoint, bounded JSON/SSE responses, and explicit Chat Completions/Responses/Anthropic transcript adapters. No credential discovery or automatic provider fallback.
- auth.rs: native ChatGPT public-client OAuth, validated account identity/scopes, protected local token storage, refresh rotation, and account-specific model discovery. Subscription credentials never enter jobs or audits; config/runtime pin them to the official Responses endpoint. The small direct dependencies `ring`, `base64`, and `url` reuse packages already present in the HTTP dependency graph rather than implementing crypto or URL codecs.
- agent.rs: model/tool loop and bounded parallel workers; ToolHost trait is the actual capability boundary.
- mcp.rs: stdio JSON-RPC lifecycle, discovery, allowlisted tool calls, limits and cleanup. Explicit host-execution approval required.
- policy.rs: persisted contextual UCB routing among direct and parallel analysis. Only verified candidate outcomes update it; infrastructure failures do not. This is bandit learning, not LLM-weight training.
- workspace.rs: private snapshot, bounded text/file inspection, traversal/symlink/secret guards and exact edits. A mutation mutex covers expected-digest validation through atomic rename, including new-file creation; optimistic literal edits revalidate under that lock. Original target never written.
- context.rs: bounded root project guidance and workspace-only skill discovery/read, using existing workspace path guards. Repository content is inert guidance, not approval. runtime.rs injects root guidance and registers skill/search evidence IDs.
- output.rs: pure model-facing presentation reduction for recognized successful libtest output, applied only after raw audit persistence. It cannot change verification decisions or raw reports.
- session.rs: read-only retained-run listing/search/show over existing audit/report files; no second database or transcript replay. main.rs owns the independent-task console and reuses the one-shot approval/status path.
- process.rs: fixed user-provided verifier argv, cleaned environment, time/output/resource bounds, process-group cleanup. macOS sandbox denies network and writes outside candidate. Unsupported platforms fail closed.
- main.rs: CLI approvals, authentication UI entry points and the internal verifier exec helper.
- runtime.rs: job orchestration, audit ledger, reports and routing updates. Validated disjoint file ownership bounds coding workers; all workers join before integration or verification. The state lock prevents conflicting runs; the native launch handshake and resource outcomes keep infrastructure failures out of rewards. User-selected direct/parallel strategy overrides learned selection, never approvals.

Coding plans may attach a bounded, task-specific `context` string to each coding
assignment. The runtime transfers it only as untrusted worker context; scopes,
budgets and verifier authority are unchanged. See
[the delegation comparison](docs/DELEGATION-COMPARISON.md) for the source evidence
and acceptance contract behind this Hermes-inspired behavioral adaptation.

## Trust boundaries

Artifact text, model outputs, and MCP responses are untrusted data, never approval. Model cannot edit verifier specification, policy state or audit history. Read-only analysts cannot write; coding workers may write only assigned candidate files. Neither receives command execution, verifier, MCP, or recursive delegation capabilities. Verifier argv comes only from the user's job file; a passing process alone does not establish universal equivalence. Immutable verifier must be external to candidate and bind to candidate through its working directory. Test bodies embedded in writable candidate are not trustworthy rewards. Unknown target execution is off by default. MCP servers are executable host integrations, not a sandbox; approval is explicit and separate from model-network permission.

## Initial acceptance and explicit limits

Prove real native CLI execution, HTTP tool-call protocol against clearly labelled deterministic test servers, scoped edits and child isolation, parallel workers, real external verifier subprocesses including denied writes/network, persisted policy updates and failure handling. Prove MCP transport with a clearly labelled local protocol fixture; exercise installed REA read-only where available. Live LLM coding and Codex comparison require an explicitly approved inference endpoint and a frozen task suite. Source/format inspection is built in; advanced native/mobile/firmware/browser analysis depends on configured tools and their runtime availability. No unverified backend is advertised as working.

Not included initially: GUI, gateway, LLM training, automatic deployment, unrestricted shell, automatic installation of analyzers, original-source recovery guarantees.
