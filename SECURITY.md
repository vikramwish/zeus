# Security boundaries

Zeus is an experimental local development runtime. Use only targets you are authorized to inspect or modify. A successful test suite is not a security certification.

## Explicit approvals

Model access, verifier execution, and MCP program launch each require a separate CLI flag. Model text, repository instructions and tool responses cannot grant those permissions. Read-only analysts receive no write capabilities; coding workers receive write/edit only for their validated, disjoint exact file assignments. Neither worker type receives verifier-execution, MCP, shell, or recursive delegation capabilities. The integration implementer starts only after all workers join. All agent edits are confined to a private candidate through path-checked tools; the original source is not a working directory for generated code.

## Verifier

The caller supplies fixed argv, outside model control. Jobs accept only `["/absolute/executable"]`, `["/absolute/interpreter", "/absolute/check-script"]`, or `["/absolute/python", "-I", "/absolute/check.py"]`. Every entry point must be an existing regular file outside both source and state/candidate, including through symlink aliases. Relative scripts, inline code, module discovery and additional arguments are rejected before a run is created. Put more complex invocation logic in a trusted external wrapper; the wrapper must own its assertions and must not delegate pass/fail to mutable candidate tests. This deliberately narrow contract avoids guessing which arbitrary interpreter arguments are executable code. Use a trusted **external** verifier, not a test script in the writable candidate. Prefer a real interpreter executable, not an Apple `/usr/bin/python3`/Xcode discovery shim; the current sandbox may deny a shim's toolchain discovery and cache writes. The example uses Homebrew Python.

Execution uses macOS Seatbelt (`sandbox-exec`) or, on Linux, bubblewrap with all namespaces unshared, read-only binds of the system directories, executable, launcher and explicit external files, and the candidate as the only writable bind. Both deny network, clear the environment, use an inert `HOME=/var/empty` and a candidate-local TMPDIR, and limit writes to the candidate (plus `/dev/null`). The bubblewrap backend has not yet been exercised end to end on Linux; its argument list is unit-tested and a missing `bwrap` fails closed. Python user-site startup is disabled and safe-path mode is requested; trusted wrappers remain responsible for interpreter-specific imports and nested subprocess failures. A private close-on-exec channel from the native launcher distinguishes kernel launch failures from completed test exits, including legitimate exit status 71. Launch/resource failures abort without training the router. Read access includes system/runtime directories and explicitly selected external regular-file arguments. This means external verification scripts can be read by candidate code, but not changed through the sandbox. This is not a hidden-test boundary.

Wall time, CPU, open-file limits, per-file write size and captured-output limits are enforced. Same-process-group descendants are killed after the leader exits. A descendant using `setsid`/`setpgid` can escape that cleanup; output collection still has a deadline. There is no VM memory/disk quota or complete hostile-process-tree containment. Do not run malicious artifacts with this verifier. Use a separately designed disposable VM/container environment for that threat model. Unsupported host platforms fail closed rather than silently executing without the sandbox.

The current reward authority is the user-selected verifier. Python assertions, test output and exit codes must be interpreted according to that verifier's actual contract. Candidate-controlled tests are not an independent correctness oracle. A model's prose is never the reward.

## MCP

MCP servers are trusted, explicitly approved host programs. They may read host files, connect to networks or execute other processes according to their own implementation. Tool allowlists constrain what Zeus advertises/calls; they do not sandbox the server or prove that an allowed operation is read-only. No live-device action or unknown executable should be authorized merely because it is exposed as an MCP tool.

The protocol client clears inherited environment except PATH/HOME/TMPDIR, refuses unlisted tools, bounds request/response traffic and time, and terminates its server process group on drop. HOME remains available for server functionality; a server can read credentials from disk. Never approve an untrusted server.

## Plugins

Plugin bundles require `--allow-plugins`, independently of ordinary MCP and verifier flags. Job-owned tool and hook allowlists are checked against bounded external manifests before any plugin starts. Plugins cannot be auto-discovered from candidate guidance. Model-callable plugin tools are exposed only to the integration implementer; scoped-autonomy jobs reject executable plugin capabilities. Skills are cached, digest-bound untrusted data and never grant authority.

Plugin tools and lifecycle hooks use the same **unsandboxed trusted-host** boundary as MCP. A plugin may access host credentials, mutate source/verifier/audit files, or perform network operations; the allowlist is not a sandbox. A selected hook executes even when no model call is needed. Successful terminal hooks are followed by fresh verification when a verifier exists; changed outcomes and hook failures abort without policy learning. This prevents accidental stale verdicts, not malicious host-program interference. Error hooks are best-effort and do not run after cancellation or job-deadline expiration. See [PLUGINS.md](docs/PLUGINS.md) for payloads, limits, lifecycle semantics and library-caller responsibilities.

## Data and concurrency

Common credential filenames are excluded from snapshots, and text files matching well-known key shapes are kept for the verifier but withheld from the model's read and search tools. Secrets in other shapes are not detected. Review target scope before allowing model access. Transcripts include selected source and model/tool content, and are stored inside a private state directory; never publish the runs directory.

The state lock protects cooperative Zeus processes, not a hostile same-user process. Filesystem checks do not promise protection against concurrent malicious filesystem mutation. Do not run untrusted sibling processes against a candidate while Zeus edits it. Stale locks after abrupt termination require checking the owner before removal.

Within a run, workspace mutation holds one mutex from expected-version checking through rename. Concurrent requests using the same old digest (or both claiming a new file) cannot both commit. Model calls and independent worker reasoning still run concurrently. Workers share a candidate, not isolated filesystem snapshots; cross-file reads are not a transactional snapshot. On worker failure, already-started siblings are joined and partial candidate edits are retained for inspection, not rolled back or promoted. No final verifier or integration agent starts on an infrastructure-failed worker batch.

Keep policy state, verifier definitions and audit records outside the agent's writable candidate. A policy update demonstrates routing learning, not model improvement. Persisted evidence/report links must be reviewed before using a candidate in production.
