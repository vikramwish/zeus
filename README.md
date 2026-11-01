# Zeus

A standalone Rust agent runtime for evidence-driven engineering and reverse engineering.

Zeus is an early working implementation, not a Codex replacement or a demonstrated "better-than-Codex" system. The goal is broad investigation and reconstruction with measured correctness. Model quality and accessible evidence still bound what it can do.

## What exists

- Native HTTP model/tool loop supporting OpenAI-compatible Chat Completions, streamed Responses, and Anthropic Messages; no dependency on the Hermes, Codex or Prime runtime.
- Native ChatGPT subscription OAuth login, protected local credential storage, refresh, and account-specific model listing. API-key and local endpoints remain separate; no automatic billing fallback. Authentication/protocol fixtures are tested; real account authorization still requires the user's browser sign-in.
- Direct execution or model-planned investigations with up to three parallel, read-only analyst agents, followed by one writing implementer.
- Private candidate snapshots: original targets are never intentionally edited. File writes require the current SHA-256; traversal and symlinks are rejected.
- Artifact inventory and signature identification, including unknown-format reporting. Identification is not decompilation.
- MCP stdio client with explicit server/tool allowlists for external analyzers (for example REA). A real installed REA backend has been exercised on a compiled Go artifact, confirming digest-bound compiler metadata and denial of an unlisted execution tool. Other analyzer capabilities need separate verification.
- Evidence records distinguishing observations, inferences and unknowns. Valid source references are checked; an LLM claim is not automatically established as true.
- Fixed external verification, bounded repair attempts, JSONL audit records and retained candidates.
- Persistent contextual UCB routing between direct and parallel strategies, trained only from verifier outcomes. This changes routing, not LLM weights. Infrastructure errors are not intended as training examples.

## Build and test

Requirements: Rust/Cargo and macOS for the current verifier sandbox. The test fixture also uses Python 3; the end-to-end test currently expects Homebrew Python at `/opt/homebrew/bin/python3`.

```sh
cargo build --release
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

The executable is `target/release/zeus`. Tests include clearly labelled deterministic HTTP/MCP fixtures; passing those tests does not demonstrate model intelligence.

## Inspect without inference

```sh
./target/release/zeus inspect /absolute/path/to/artifact-or-repository
```

This reports file hashes, format candidates and skipped entries without running the target or contacting a model. Current inventory bounds: 2,000 files, 16 MiB per file, 64 MiB total. Select a focused target for larger applications. Common secret filenames and dependency/build directories are excluded; this is not general secret detection.

## Run a task

Use `examples/reconstruction/job.example.json` as a template. Set its absolute external verifier path and choose your model endpoint. Relative source/state paths resolve against the job file's directory. The state directory's parent must already exist; state must be outside the source.

`model_timeout` optionally sets the whole-request deadline in seconds (default 180, allowed 1–900), including response reads. Slow local inference shared by parallel analysts may need a larger value, for example 600. This is not a per-byte inactivity timer, a whole-job deadline, or an automatic retry. Exhausted model requests remain infrastructure errors and do not train the routing policy. OS DNS lookup remains subject to the system resolver rather than this interruptible deadline.

Verifier argv must name either one absolute external executable or an absolute interpreter followed by an absolute external script (Python's `-I` is optionally supported before the script). Relative scripts, inline code, module discovery, and extra arguments are rejected. For other invocation patterns, supply a trusted external wrapper that owns the checks. Verifier files must be outside both source and state directories; candidate-controlled tests cannot be the reward authority.

```sh
./target/release/zeus run examples/reconstruction/job.local.json \
  --allow-network --allow-exec
```

Permissions are separate:

- `--allow-network`: send task/source/tool evidence to the exact configured model endpoint. HTTPS is required except for loopback HTTP; redirects and environment proxies are disabled.
- `--allow-exec`: run the fixed user-selected verifier in the macOS sandbox. The model cannot select arbitrary commands.
- `--allow-mcp`: launch the configured MCP programs on the host and expose only allowlisted tools. This is an approval for executable host integrations, **not** a sandbox. Approve only trusted servers and tool operations.

For API connections, credentials come from the named `api_key_env` environment variable, never from committed job files or automatic Hermes credential discovery. Subscription connections use the explicit ChatGPT login flow instead. Do not paste credentials into tasks. See [model connections](docs/PROVIDERS.md) for subscription login commands, model discovery, supported protocols, and example connection settings.

A first completed training run uses the direct route. The next run in the same context explores parallel analysis; subsequent choices use accumulated verified outcomes. A verifier that already passes causes an `already_verified` no-op instead of unnecessary model work. Without a verifier, output is explicitly `unverified` and the CLI returns nonzero.

Parallel planner responses are limited to 64 KiB of UTF-8 text, including whitespace and framing. The accepted form is one JSON object containing only `tasks`: an array of 2–3 non-whitespace strings, each at most 4,096 UTF-8 bytes. Optional surrounding whitespace and a single complete, non-nested leading `<think>...</think>` envelope are accepted; the envelope is ignored only at this planner boundary. Fences, other prose, repeated envelopes, multiple JSON values, duplicate/unknown fields and invalid tasks fail closed before workers or the implementer start, without a direct-route fallback or policy update. Raw planner output remains in the private audit. This deliberately narrow compatibility rule does not normalize worker/implementer output or grant analysts additional capabilities.

Each run retains its original/final inventories, `events.jsonl`, evidence findings, final report or infrastructure error, and the candidate directory. Policy state lives outside the candidate. A `run.lock` prevents concurrent writers to one state store. After a killed process, inspect the recorded PID before manually removing a stale lock. No automatic promotion, deployment, or overwrite of the original is performed.

## Reverse-engineering scope

Zeus is designed to route arbitrary artifact types to suitable tools and explicitly record unsupported capabilities. Source repositories can be inspected directly. Native binaries, packages, mobile applications, firmware, protocols and running applications require configured analyzers and, where necessary, authorized runtime evidence. Current built-ins alone do **not** recover these targets' internals.

Use source IDs and tool-event IDs to trace a reconstructed specification back to observations. Static interpretation, runtime observations, and source-derived facts should remain distinct. Finite tests establish only the behavior they cover, not universal equivalence or original-source recovery.

## Safety and present limitations

Read [SECURITY.md](SECURITY.md) before execution. In particular, the current macOS sandbox is not a VM and is not sufficient containment for deliberately hostile executables. MCP programs have host privileges. A user's verifier determines what a pass means; a trivial or compromised verifier cannot supply meaningful rewards.

There is no GUI, unrestricted shell, automatic analyzer installation, automatic deployment, LLM-weight training, production-grade session resume, or established Codex-level benchmark result. Model requests and tool calls are bounded; exhausted budgets and missing capabilities are errors, not successes.

See [ARCHITECTURE.md](ARCHITECTURE.md) for source-inspection notes and component ownership, and [the reconstruction exercise](examples/reconstruction/README.md) for the public fixture's contract and sensitivity checks.
