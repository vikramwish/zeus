# Sessions and independent task console

Zeus sessions are **retained runtime runs**, not resumable agent conversations. These commands use the existing run directory and audit format; there is no second database, import, migration, background service, or dependency.

## Browse without inference or execution approval

```sh
zeus sessions list /absolute/path/to/state
zeus sessions search /absolute/path/to/state 'parser'
zeus sessions show /absolute/path/to/state run-<timestamp>-<pid>
```

Replace the run ID with an exact ID returned by `list`. Commands emit JSON. `list` returns task, source, ID, status and final summary, ordered by descending run ID. `search` performs a case-insensitive literal substring match over task, summary, ID and status, not over all tool output. `show` additionally exposes the first `start` audit event, immutable report, infrastructure error and separate learning record, when present. The report's `policy_updated` can remain false even when a later `learning.json` records training; this is the runtime's existing proof-before-training format, not a reconstructed claim.

Browsing reads only the explicitly supplied state directory. It never contacts a provider, loads credentials, acquires/removes `run.lock`, modifies policy, executes tools, reads candidate contents, or restores a pending tool call. A missing report/error is labelled `incomplete`; this does not distinguish an active run from a killed run. An unreadable or corrupt artifact is an error, **not** an empty catalog or a successful run. A missing state directory is also an error; no directory is created by browsing.

Limits are explicit: 10,000 directory entries, 1,000 run IDs, 64 MiB of artifact bytes per invocation, 1 MiB per JSON artifact, and 128 KiB for the first audit line. Exceeding a limit fails rather than silently returning a partial result. Search queries are nonblank and at most 4,096 UTF-8 bytes. Only exact `run-<digits>-<digits>` IDs are accepted. State/run directories and artifact files must not themselves be symlinks; artifacts must be regular files and Unix opens use no-follow. Like the existing workspace, this is not containment against a malicious same-user process concurrently replacing ancestor directories.

Run text and tool evidence may contain sensitive source content. JSON escapes terminal control characters in stored strings, but is not secret redaction. Do not publish private state directories.

## Interactive task console

```sh
zeus console path/to/job.json --allow-network
# Add --allow-exec for a configured verifier, --allow-mcp for configured MCP servers.
```

The console loads and validates the job once through `Job::load`, with exactly the same explicit CLI approvals as `zeus run`. It does not run anything merely on entry. The job's endpoint, source, verifier, MCP configuration and approvals stay fixed until exiting and starting another console. Task/repository/model text cannot change permissions.

| Input | Effect |
| --- | --- |
| `/help` | Explain commands and independent-run semantics |
| `/run` | Run the job's original configured task |
| Ordinary nonblank text | Run that line as a new task (at most 32,768 UTF-8 bytes) |
| `/list` | Browse the configured state directory |
| `/search <query>` | Search its retained runs |
| `/show <exact-run-id>` | Show retained evidence for one run |
| `/quit` or EOF | Exit without starting another task |

Unknown slash commands, including `/resume`, are rejected, never sent to the model. Prompts/help/errors go to stderr; reports and browsing results are separate pretty-printed JSON values on stdout. Each task calls the actual `runtime::run`; completed and failed runs use the runtime's existing durable artifacts and state lock. Failure or an unverified result is printed and leaves the console available for browsing or another explicitly entered task. Exit is nonzero if **any** task was unverified/failed or a command failed; a later successful command cannot hide it. Read/UTF-8/oversized-input errors terminate the console without running that input. Each runtime invocation retains its original network, tool, context and workspace budgets; there is no additional whole-console task budget.

**Each task starts a fresh snapshot of the original source and a fresh model conversation.** Prior candidate changes and transcripts are not carried forward. Existing routing policy may be reused by the runtime, but only its external verifier can train it. This is not multi-turn editing, transcript continuation, restart recovery, or full Hermes/Prime session parity. There is no automatic replay, shell, background task queue, live event viewer, pagination, session deletion, renaming or branching. Interrupted runs require the existing manual stale-lock procedure; the console does not remove locks.

## Inspected upstream behavior and ownership

Source inspection informed boundaries, not a source-code port; no upstream implementation was copied.

- Hermes checkout `bf0203a794a07aac3ff764972ab9087baa4e4f4e`: `hermes_cli/web_routers/sessions.py` (`get_sessions`, `_resolve_session_id`, `search_sessions`) separates bounded read-only listing from corrupt/unavailable storage and message search. `tui_gateway/session_history.py` separately shapes conversation history. License: MIT, copyright Nous Research (local `LICENSE`). Zeus deliberately exposes existing run metadata, not an invented compatible conversation store.
- Prime checkout `c4c238ca025e382d96fb5f37e6328ca77fb82ef3`: `crates/pa-daemon/src/session_catalog.rs` resolves saved sessions with ambiguity checks and explicit restore behavior. License: MIT, copyright Prime Intellect Ltd.; its license also acknowledges Mario Zechner's original TypeScript product. Zeus uses exact run IDs and performs no restore or wake-up.
- Zeus `src/runtime.rs::run` already owns candidate creation, exclusive state locking, audit, reports and learning. `src/session.rs` only reads these artifacts. `src/main.rs` shares the existing `run_job` status handling between one-shot execution and the console, without duplicating the runtime or approval layer.

## Proof

`cargo test --test session_cli --test cli_flow --test provider_cli` covers the actual CLI, durable browsing from a separate process, case-insensitive search, independent task requests against a deterministic localhost HTTP fixture, unchanged original files, nonzero unverified exit, approval gates, no execution on entry, corruption/size/path/symlink rejection, retained infrastructure failures, and successful external-verifier status plumbing. The verifier fixture is deliberately trivial and the HTTP fixture is scripted: neither demonstrates model intelligence or meaningful project correctness. No paid inference, account login or live MCP server is used.
