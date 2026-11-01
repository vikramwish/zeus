# Zeus: implementation contract

Independent Rust runtime, not a wrapper around Hermes/Codex/Prime. No claim of feature parity or superior model intelligence. Broad target support comes from evidence-producing analysis tools, not a promise of universal source recovery.

## Prior source inspection (before implementation)

- Hermes c419cf7c864ac0ee511dbd9f7091a8bba0b567ee: tools/delegate_tool.py owns child isolation, limited inherited tools, depth/budget enforcement, and results. Reproduce those invariants, not its implementation.
- Prime c6d4da9421e6087e3fc1a763e947cf37153cd76f: crates/pa-agent/src/agent_loop/entry.rs constructs a per-run context and emits lifecycle events; tool_call.rs validates lookup/arguments, handles abort, and preserves tool call IDs. Reproduce bounded execution and correct transcript ordering.
- Source inspection only; not benchmarked against either product. No upstream source copied.

## Behavior and ownership

Job JSON -> config validation -> exclusive state lock -> immutable source inventory -> disposable candidate copy -> model-planned independent analysis questions -> up to three read-only worker loops -> one writing implementer -> user-supplied external verifier -> bounded repair -> evidence/report -> routing update.

- model.rs: validated HTTP endpoint, bounded JSON/SSE responses, and explicit Chat Completions/Responses/Anthropic transcript adapters. No credential discovery or automatic provider fallback.
- auth.rs: native ChatGPT public-client OAuth, validated account identity/scopes, protected local token storage, refresh rotation, and account-specific model discovery. Subscription credentials never enter jobs or audits; config/runtime pin them to the official Responses endpoint. The small direct dependencies `ring`, `base64`, and `url` reuse packages already present in the HTTP dependency graph rather than implementing crypto or URL codecs.
- agent.rs: model/tool loop and bounded parallel workers; ToolHost trait is the actual capability boundary.
- mcp.rs: stdio JSON-RPC lifecycle, discovery, allowlisted tool calls, limits and cleanup. Explicit host-execution approval required.
- policy.rs: persisted contextual UCB routing among direct and parallel analysis. Only verified candidate outcomes update it; infrastructure failures do not. This is bandit learning, not LLM-weight training.
- workspace.rs: private snapshot, bounded text/file inspection, traversal/symlink/secret guards and exact edits. Original target never written.
- process.rs: fixed user-provided verifier argv, cleaned environment, time/output/resource bounds, process-group cleanup. macOS sandbox denies network and writes outside candidate. Unsupported platforms fail closed.
- main.rs: CLI approvals, authentication UI entry points and the internal verifier exec helper.
- runtime.rs: job orchestration, audit ledger, reports and routing updates. Single writer and state lock prevent conflicting runs; the native launch handshake and resource outcomes keep infrastructure failures out of rewards.

## Trust boundaries

Artifact text, model outputs, and MCP responses are untrusted data, never approval. Model cannot edit verifier specification, policy state or audit history. Child analysts cannot write or execute commands. Verifier argv comes only from the user's job file; a passing process alone does not establish universal equivalence. Immutable verifier must be external to candidate and bind to candidate through its working directory. Test bodies embedded in writable candidate are not trustworthy rewards. Unknown target execution is off by default. MCP servers are executable host integrations, not a sandbox; approval is explicit and separate from model-network permission.

## Initial acceptance and explicit limits

Prove real native CLI execution, HTTP tool-call protocol against clearly labelled deterministic test servers, scoped edits and child isolation, parallel workers, real external verifier subprocesses including denied writes/network, persisted policy updates and failure handling. Prove MCP transport with a clearly labelled local protocol fixture; exercise installed REA read-only where available. Live LLM coding and Codex comparison require an explicitly approved inference endpoint and a frozen task suite. Source/format inspection is built in; advanced native/mobile/firmware/browser analysis depends on configured tools and their runtime availability. No unverified backend is advertised as working.

Not included initially: GUI, gateway, LLM training, automatic deployment, unrestricted shell, automatic installation of analyzers, original-source recovery guarantees.
