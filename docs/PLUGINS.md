# Local plugins

Zeus plugins bundle MCP tools, inert skill documents and lifecycle hooks behind an explicit job configuration. They run as external processes, not dynamically linked Rust code. No package manager, remote installation, automatic repository discovery or hot reload is provided.

## Enable a bundle

Add this entry to an existing job (replace the manifest path):

```json
{
  "plugins": [
    {
      "manifest": "/absolute/path/to/echo/plugin.json",
      "allowed_tools": ["echo"],
      "allowed_hooks": ["run_start", "run_end", "run_error"]
    }
  ]
}
```

Run with `zeus run job.json --allow-network --allow-plugins`; add `--allow-exec` when using a verifier. `console` accepts the same flags. `--allow-mcp` is still required for separate entries in `mcp`, but is not needed for a plugin's own server. Neither `--allow-mcp` nor `--allow-exec` substitutes for `--allow-plugins`.

Relative manifest paths resolve against the job file's directory. The manifest must be outside both the source and state directories, including aliases. Copy a bundle to a trusted external directory if the source being worked on contains it. Explicit job selection is the installation/enable mechanism: remove an entry to disable it, or narrow its allowlists to disable specific capabilities. All manifests are validated before any server is started. Editing an allowlist in repository guidance cannot grant a capability.

Only approved tools are advertised, as `plugin__<plugin-name>__<tool-name>`. Plugin names occupy a separate namespace from ordinary `mcp__` tools. Parallel analysts and coding workers receive no plugin execution capabilities; only the integration implementer does. A job with `autonomy.write_paths` set cannot use executable plugin tools or hooks, since those unsandboxed programs could bypass its file scope. Skills-only use is permitted.

An omitted `allowed_tools` or `allowed_hooks` means an empty list, not “all.” Declaring a hook in a manifest does not authorize it. No process starts for a plugin with both allowlists empty, even if the manifest declares commands/tools.

## Manifest version 1

The working example is in `examples/plugins/echo/`. Its Python server has no third-party dependencies and performs no file/network writes. It uses `/usr/bin/env python3`; change the command to a trusted absolute interpreter if PATH discovery is undesirable.

```json
{
  "version": 1,
  "name": "echo",
  "description": "Echo text and acknowledge lifecycle events",
  "command": ["/usr/bin/env", "python3", "-I", "${PLUGIN_DIR}/server.py"],
  "timeout_seconds": 30,
  "tools": ["echo", "on_start", "on_end", "on_error"],
  "skills": [{"name": "usage", "path": "guide.md"}],
  "hooks": {
    "run_start": "on_start",
    "run_end": "on_end",
    "run_error": "on_error"
  }
}
```

Fields other than `version` and `name` are optional. A command is required when declaring tools/hooks. Unknown fields, unsupported versions, duplicate IDs, duplicate tool/skill names and unknown allowlist entries are errors. Hook targets must also appear in `tools`. A hook-only target is not model-callable unless separately selected in `allowed_tools`.

`command` is an argv array, never interpreted as a shell string. The executable must be absolute (or start with `${PLUGIN_DIR}/`). Every `${PLUGIN_DIR}` occurrence in argv is replaced with the manifest's canonical parent directory. Other relative arguments are not rewritten; use that placeholder for bundled scripts and resources. The child inherits Zeus' working directory. Each bundle gets one MCP connection shared by its allowed tool calls and hooks. The server must implement the existing Zeus MCP stdio handshake (`2024-11-05`), `tools/list` and `tools/call` contracts.

Plugin IDs use 1–16 ASCII letters/digits, `_` or `-`, without `__`. Tool names use ASCII letters/digits, `_` or `-`; the complete model-visible name must fit 64 bytes. Skill names use 1–64 ASCII letters/digits, `_` or `-`.

Limits: 16 plugin entries; 64 declared tools and 64 skills across bundles; 128 KiB per manifest; 16 KiB per UTF-8 skill; 32 KiB skill catalog. The combined workspace/plugin skill catalog also has a 64-item/32-KiB limit. Overflow fails explicitly rather than silently truncating. `timeout_seconds` defaults to 30 and accepts 1–600. Initialization plus all discovery pages share one deadline; each tool or hook call receives a fresh deadline. The 2 MiB MCP transport limits remain unchanged, as do the model's existing context and tool-output limits.

## Skills

`list_skills` combines workspace skills and approved plugin skills. Read plugin guidance with `read_skill` using `echo/usage`, distinct from an unqualified workspace skill such as `checks`. Names are matched exactly, not treated as arbitrary filesystem paths.

Skills are loaded once at run startup, cached as UTF-8, labelled `untrusted_plugin_data`, and identified by `plugin:<plugin>/<skill>#<sha256>`. Their source IDs can be cited in findings after listing or reading. Their text never overrides the user task, system rules, or permissions. Referenced scripts and files are not automatically opened or executed. Relative skill paths must remain inside the bundle; symlinks and nonregular/hardlinked files are refused. Manifests also must be regular, non-symlink, non-hardlinked files.

## Lifecycle

Hooks are ordinary MCP tool calls made by the runtime, not by the model. They run synchronously in job plugin order. A handler receives one JSON object as its tool arguments:

| Event | Timing | Arguments |
| --- | --- | --- |
| `run_start` | After the candidate, audit and all connections exist; before baseline verification or model work | `task`, `candidate`, `run_dir` |
| `run_end` | After normal agent/baseline completion, before final inventory/report persistence and policy learning | `status`, `candidate`, `run_dir` |
| `run_error` | On an error after plugin initialization, including hook failures; not on preflight/startup failures | `error`, `candidate`, `run_dir` |

`status` at `run_end` can be `already_verified`, `verified`, `failed` or `unverified`. It is provisional, not a commit notification. `run_error` is not called for an ordinary completed verifier failure (`failed`), and is skipped after cancellation or expiration of the job deadline. It is best-effort: a broken or already-closed plugin connection can prevent delivery. There is no crash/shutdown guarantee or replay on restart.

Each handler's request is durably recorded before dispatch as `plugin_hook_request`, and its result immediately afterward as `plugin_hook_result`, including plugin name and target tool. Earlier handlers' evidence survives a later failure. An audit-write failure stops dispatch. Hook failure is fatal and never becomes a training reward; an error-hook failure preserves the original error as well. Outputs are audit data, not instructions injected into the model. The first failing handler stops that event's remaining handlers.

A terminal hook can modify candidate files. If any `run_end` handler ran and a verifier is configured, Zeus reruns the fixed verifier before committing a report. A changed pass/fail outcome is an infrastructure error (`run_end invalidated verification`), with no policy update. Otherwise the fresh result is recorded as `post_plugin_verification`, and final inventory is captured after hooks. Without a verifier the result remains `unverified` regardless of a hook's success or text.

## Trust and compatibility

Tools and hooks are trusted host programs, not sandboxed extensions. Allowlists constrain RPC dispatch; they do not constrain what a server does at startup or within an allowed call. Plugins may access host files/network, modify the verifier or audit, or spawn processes. Approve only reviewed code. Reverification guards accidental post-verification mutation, not a malicious host plugin forging evidence.

The MCP child environment is cleared except PATH/HOME/TMPDIR. That is not credential isolation: a host program may read files under HOME. Do not embed credentials in manifests or skills; skill contents and tool results may be sent to the configured model. Process-group cleanup and cancellation use the existing MCP transport, including job-deadline checks during blocked writes. Detached descendants and hostile same-user filesystem races are outside the containment guarantee.

Library callers own approval: `Job::load` enforces CLI-style flags, while direct `runtime::run` and `PluginSet::connect` callers are trusted hosts. `runtime::run` still validates plugin manifests and rejects executable plugins under scoped autonomy. Console tasks reload the bundle on each independent run; approval authorizes the selected bundle, not a cryptographic content pin.

The shipped example and integration tests exercise real local stdio processes and deterministic HTTP model fixtures. They do not certify compatibility with arbitrary third-party MCP servers or demonstrate live-model task quality.

```sh
cargo test --test plugin_bundles --test plugin_runtime --test plugin_deadline --test plugin_example
```
