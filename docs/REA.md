# REA integration

Zeus uses its own Rust MCP client, not Hermes' tool bridge. REA must already be installed and trusted. Do not install or update analyzers implicitly during a task.

Example `mcp` entry in a job file (adjust executable paths):

```json
{
  "name": "rea",
  "command": [
    "/usr/bin/env",
    "REA_MCP_OUTPUT_SCHEMA_PROFILE=off",
    "/opt/homebrew/bin/rea-agents",
    "--mcp"
  ],
  "allowed_tools": ["inspect_go_binary"]
}
```

Then supply `--allow-mcp` in addition to model/verifier permissions. The model sees the qualified function `mcp__rea__inspect_go_binary`. The allowlist is literal; tool output cannot expand it. Analyst children do not receive MCP capabilities in this release.

The installed REA 6.8.0 default output-schema catalog exceeded Zeus' 2 MiB transport limit in a real smoke test. Its native `REA_MCP_OUTPUT_SCHEMA_PROFILE=off` setting omits the large advertised output schemas while preserving REA's own result validation and structured evidence. This is configured only for that child process; no Hermes profile or global REA configuration is modified. Avoid weakening the transport bound to solve a catalog-size problem.

## Actual integration proof

```sh
cargo test --test rea_live -- --ignored --nocapture
```

This opt-in test compiles a small Go executable, does not execute it, connects to the installed REA MCP server, requests embedded Go build metadata, checks the returned evidence contains the actual executable's SHA-256 and Go compiler identity, and verifies an unlisted execution tool is denied.

It requires `/opt/homebrew/bin/go` and `/opt/homebrew/bin/rea-agents`; ordinary `cargo test` does not launch REA. The test is not a native-code decompilation/reconstruction benchmark. It proves one real backend path; each additional target/tool family needs its own acceptance evidence.

For broader targets, choose exact approved inspection tools from the server's actual catalog. Static inspection and target execution are different permissions. Do not allow general debugger or process-execution tools for an unknown artifact without an appropriate isolation environment.
