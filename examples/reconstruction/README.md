# Local reconstruction exercise

This is a public educational fixture, **not a hidden benchmark**, a Codex parity
comparison, or a claim of universal equivalence. It needs only Python's standard
library. Running the verifier itself makes no network calls and uses no inference
service; the optional end-to-end exercise below uses a locally installed model.

## Task and ownership

Use `source/` as the immutable input template. The runtime should snapshot **only
that directory** into a disposable candidate, then repair the candidate's
`kv.py`. Do not copy `verify.py` into the candidate or edit the template to pass.

There is one intentional defect: `save_state` is a no-op. Mutations report success
but disappear across CLI invocations. Parsing, reads, and in-process mutations
are implemented. Repair persistence without changing the public contract.

This follows `ARCHITECTURE.md`: source inventory → candidate → single writing
implementer → fixed external verifier. `verify.py` belongs to the trusted host,
not the candidate. Its read-only file mode is an accident guard, not a security
boundary: the runtime must enforce immutability and prevent candidate writes to
it. No production Rust code is part of this fixture.

## CLI contract

```text
python3 -I kv.py STATE_FILE set KEY JSON_VALUE
python3 -I kv.py STATE_FILE get KEY
python3 -I kv.py STATE_FILE delete KEY
```

- One JSON response on stdout, no stderr. JSON whitespace and object-key order
  are irrelevant. Keys are strings (including empty and Unicode strings).
- Set/overwrite and successful delete: `{"ok": true}`, exit 0.
- Get: `{"value": <stored JSON value>}`, exit 0. Supports strings, nested arrays
  and objects, numbers, booleans, and null; Unicode must survive round trips.
- Missing get/delete: `{"error": "missing key"}`, exit 1.
- Bad operation, arity, or JSON value: `{"error": "invalid input"}`, exit 2.
- Malformed/non-object state or state I/O failure:
  `{"error": "invalid state"}`, exit 2. Reject non-finite JSON numbers, including
  exponent overflow. Preserve malformed state rather than silently resetting it.
- Missing state means an empty store. Reads and rejected commands do not create
  or alter it. State is a UTF-8 JSON object keyed by the supplied keys.
- Successful mutations must survive separate processes and preserve unrelated
  keys. Publish writes atomically so a failed write does not truncate old state.
- Operations are sequential; simultaneous writers, crash durability, hostile
  filesystem races, and arbitrary store sizes are outside this miniature task.

## External verification

Run with the **candidate directory as cwd**, using fixed absolute verifier argv:

```sh
cd /path/to/disposable/candidate
/usr/bin/python3 -I /absolute/path/to/native-agent/examples/reconstruction/verify.py
```

The verifier launches each command as
`[sys.executable, "-I", candidate/kv.py, state_path, ...]`, with a five-second
per-command timeout. It creates and removes its own isolated state directory
inside the candidate, never changes `kv.py`, and never imports candidate modules
into its own process. It checks JSON responses, exit statuses, stderr,
cross-process persistence, overwrite, deletion, preserved unrelated keys,
Unicode, missing keys, invalid arguments/JSON, and corrupt-state preservation.
It also tests a state-read failure using a directory as the store, and set/delete
write failures using a readable store in a non-writable directory. Both failed
mutations must preserve the original bytes. Run as an **unprivileged POSIX user**;
root or a filesystem that ignores permission bits cannot exercise that denial.
Permissions are restored in a `finally` block before temporary-directory cleanup.

These are finite contract checks, not an exhaustive proof. Mid-write failures,
crash durability, and every atomic-publication failure are not fault-injected.
Direct execution is for trusted local code only: `-I` isolates Python imports but
is **not** an OS sandbox. Through Zeus, verifier execution uses the macOS
write/network sandbox and resource limits described in `SECURITY.md`; that is
still not complete containment for deliberately hostile executables. The verifier
itself writes only inside the candidate; running it directly without Zeus does
not prevent a child from writing elsewhere or accessing the network.

## Observed sensitivity proof

Run the deterministic public verifier regressions from the repository root:

```sh
cargo test --test reconstruction_contract
```

The read-error mutant passed the old 77 checks, making its new regression test
fail (RED). Adding the directory-as-state probe made that test pass (GREEN).
The write-error mutant then passed 78 checks, making its regression fail (RED).
Adding permission-denied set/delete probes and preserved-byte checks made all
three tests pass (GREEN), including the atomic-persistence positive control.
These are verifier sensitivity controls, **not** model-generated acceptance.

The strengthened verifier reports 82 checks. The unchanged intentional template
fails 11 checks. The historical Qwen candidate
`runs/model-test/run-1793473784506036000-57618/candidate/kv.py` passed the old 77
checks but now fails two write-error checks: its exception tuple references the
nonexistent `json.JSONEncodeError`, producing a traceback when saving fails.
This closes a public coverage gap without changing the CLI contract.

The source template remains unchanged (SHA-256):
`2d44d5b4f2c4ec8820015d99ac2d3b8d4c569d2a9debbdf53c2abaf884370b05`.

## Real local-model exercise

Use an ignored `job.*.local.json` copied from `job.example.json`, with an existing
local model and an explicitly approved loopback endpoint. Do not download a model
or configure a paid provider merely to run this exercise. Set a fresh ignored
`runs/` state directory to avoid stale locks; reusing it on a second run retains
policy evidence and selects the previously untried parallel strategy.

Use verifier argv `[absolute_python, "-I", absolute_external_verify_py]`. Keep
`source` pointed at the original template, not a previous passing candidate.
Helpful task hints for a small local model: use relative tool paths (`kv.py`),
copy `expected_sha256` exactly from `read_file.sha256` (not `source_id`), make
`record_finding.limitations` an array of strings, and keep reasoning concise.

```sh
cargo run -- run examples/reconstruction/job.acceptance.local.json \
  --allow-network --allow-exec
```

Local inference may take several minutes. The transport defaults to a 180-second
whole-response deadline per request. A job may set `model_timeout` to 1–900 seconds;
600 seconds can accommodate slower concurrent local inference without removing
the deadline or automatically retrying requests. Allow a longer overall CLI execution window.
Retain ignored run directories, events, reports, and policy evidence. A verifier
pass establishes only these finite public checks, not general model quality,
security containment, hidden-benchmark performance, or REA/MCP integration.

### Observed local acceptance (November 1, 2026)

Existing LM Studio `qwen/qwen3.5-9b` was loaded locally with a 32768-token
context. The actual CLI used `http://127.0.0.1:1234/v1/chat/completions`, the
unchanged source template, the strengthened external verifier, and the task hints
above. No model downloads or paid APIs were used. Artifacts are intentionally
ignored, under `runs/acceptance-1793510220883704000/`:

| Run directory | Actual outcome |
| --- | --- |
| `run-1793510221260149000-31069` | Direct infrastructure failure: `model request failed: Io`; retained candidate failed its intermediate check. No policy reward recorded. |
| `run-1793510587887846000-38536` | Direct retry: CLI exit 0, `status: verified`, final `PASS: 82 checks, 0 failures`; baseline was 82 checks / 11 failures. |
| `run-1793510846996568000-43530` | Same-context next run selected Parallel, then exited 1: `planner must return JSON without fences`. No parallel acceptance or policy reward claimed. |

The final policy contains Direct 1 trial / 1 success and Parallel 0 trials /
0 successes. Logs are `runs/acceptance-direct.log`,
`runs/acceptance-direct-retry.log`, and `runs/acceptance-parallel.log`; the local
job is `examples/reconstruction/job.acceptance.local.json`. Run directories retain
`events.jsonl`, candidate files, and the successful run's report. No run was
terminated by an arbitrary 420-second wall-clock cutoff. The direct candidate
uses a fixed `.tmp` sibling and does not clean it up after every possible failure;
concurrent writers and crash durability remain outside the declared contract.
The successful public check does not erase these limits or the observed failed
attempts.
