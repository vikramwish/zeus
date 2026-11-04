# Verifier output presentation

`output::compact_verifier_result(&serde_json::Value) -> serde_json::Value`
is a pure, deterministic local transform. It launches nothing, reads no files,
loads no configuration and uses no added dependencies.

## Trust boundary

Call it **after the raw verifier result has been durably audited**, and use its
return value **only for model-facing presentation**. The input is borrowed and
never modified. Keep the original `ProcessResult` for rewards, reports, exit
status, timeouts and output-limit decisions. This function does not itself store
raw evidence; the runtime owns that obligation. Do not wrap or rewrite verifier
argv, invoke RTK, or treat text summaries as execution authority.

Only `stdout` can change; every other JSON field, including `stderr`, arbitrary
failure metadata and audit IDs, remains identical. Missing/wrongly typed fields,
nonzero/null exits, unsuccessful results, timeouts and output-limit outcomes are
returned unchanged. Required status fields are `success: true`, `exit_code: 0`,
`timed_out: false`, `output_limit: false`, and string stdout/stderr.

## Supported format (intentionally narrow)

The entire stdout must consist of one or more complete Rust libtest **successful
pretty-output** suites, with optional LF-only blank lines between/around them:

```text
running N tests
test module::name ... ok
... exactly N passing test lines ...

test result: ok. N passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`running 1 test` is the singular form. Counts must match and be canonical unsigned
decimal integers. Test names contain only Unicode alphanumeric characters,
underscores and colons. Duration is decimal digits, a dot, more decimal digits,
and `s`. The final summary need not end in a newline.

Each suite's passing-test lines are replaced only when the replacement is
strictly shorter in UTF-8 bytes:

```text
[zeus: omitted 25 passing test lines; 1420 UTF-8 bytes]
```

Those numbers count the **removed original lines and their bytes**, including
LFs; they are not net savings. Net stdout byte reduction is original stdout
length minus presented stdout length, including markers. Suite headers, counts,
summary, duration and blank lines remain exact. No token/billing estimate is
made. Individual passing test names remain in raw audit evidence, not the
presentation. Reapplying the function leaves the result unchanged.

**Everything else falls back unchanged for the entire stdout**, not partial
best-effort filtering: failed suites and their full diagnostics, ignored or
filtered tests, benchmarks, doctest names with spaces, compiler/log prefixes in
stdout, arbitrary extra text, ANSI/control characters, CRLF, pytest/Jest/JSON,
quiet/progress output, incomplete output, inconsistent counts or unknown format.
Stderr is always retained byte-for-byte, even when stdout is compacted. No
failure detector based on English keywords, line truncation, deduplication,
head/tail clipping or guessed summary is used. This is not full RTK parity.

## Upstream inspection and licensing

Inspected installed `/opt/homebrew/bin/rtk` (`rtk 1.1.0`), `rtk --help`, and
`rtk pipe --help`. RTK has many command-aware wrappers and a real stdin-only
`rtk pipe --filter cargo-test` mode. A local isolated-HOME probe fed a 215-byte
four-test successful suite to that command: exit 0, empty stderr, stdout
`cargo test: 4 passed (1 suite, 0.00s)`. No verifier was wrapped or run by this
probe; input was synthetic test text, not a claimed verification result.

Upstream inspected at develop revision
`63b8c3e26ec9487eb3953a142a8279366b5f5689`:

- https://github.com/rtk-ai/rtk/blob/63b8c3e26ec9487eb3953a142a8279366b5f5689/src/cmds/system/pipe_cmd.rs
  — stdin handling, explicit/auto filter dispatch, and never-worse output guard.
- https://github.com/rtk-ai/rtk/blob/63b8c3e26ec9487eb3953a142a8279366b5f5689/LICENSE
  — Apache License 2.0 at repository root.
- https://github.com/rtk-ai/rtk/blob/63b8c3e26ec9487eb3953a142a8279366b5f5689/engine/LICENSE
  — Business Source License 1.1 for `rtk-engine`, additional use restrictions,
  change date 2030-09-09 and change license Apache 2.0.

The installed binary's commit was not established; the revision above identifies
source inspection, not a claim that the binary was built from that revision.
No upstream source was copied or vendored. Zeus independently implements the
narrow parser above. Although RTK offers useful stdin filtering, subprocess
execution would add an installation/version/configuration boundary and output
semantics broader than this contract. Keeping a small native presentation
transform avoids that dependency and any new execution authority.

## Verification

`cargo test --test output_compaction` covers reduction, exact byte accounting,
input immutability, metadata preservation, idempotence, multi-suite/Unicode
handling, small-output no-expansion, malformed counts, unsupported formats,
failed diagnostics, stderr retention, and incomplete/infrastructure results.
`cargo test --test runtime_features` exercises the actual CLI with a deterministic
HTTP fixture and external verifier: raw audit and final reports retain original
output while only the `run_check` model-facing result is reduced.
