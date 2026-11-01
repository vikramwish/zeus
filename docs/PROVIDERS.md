# Model connections

Zeus selects a protocol and credential source explicitly. Model IDs are not hardcoded: set the identifier accepted by the chosen endpoint. A model must support the text/function-tool conversation used by Zeus. Image/audio-only models and provider-specific extensions are not automatically supported.

## API keys and local servers

Jobs default to `"protocol": "chat_completions"` and `"auth": "environment"` for compatibility. `api_key_env` is optional for a local server; if specified, the variable must exist. Zeus never searches another application's credential files.

The connection fields below are merged into a normal job with `task`, `source`, `state_dir`, and the optional external verifier:

```json
{
  "protocol": "chat_completions",
  "auth": "environment",
  "endpoint": "http://127.0.0.1:1234/v1/chat/completions",
  "model": "your-installed-model-id"
}
```

OpenAI-compatible API gateways use the same protocol with an HTTPS endpoint and the name of an environment variable containing their API key. Provider/model availability and billing belong to that endpoint.

For the OpenAI Responses API with an explicitly supplied API key:

```json
{
  "protocol": "responses",
  "auth": "environment",
  "endpoint": "https://api.openai.com/v1/responses",
  "model": "your-api-model-id",
  "api_key_env": "OPENAI_API_KEY"
}
```

For Anthropic's Messages API:

```json
{
  "protocol": "anthropic",
  "auth": "environment",
  "endpoint": "https://api.anthropic.com/v1/messages",
  "model": "your-claude-api-model-id",
  "api_key_env": "ANTHROPIC_API_KEY"
}
```

The adapter translates the common text/tool transcript to the selected wire protocol. Responses requests are streamed with `store=false` and accepted only after a completed terminal event. Anthropic requests use `x-api-key`, not an OpenAI bearer header. There is no automatic protocol/provider retry or paid fallback. Unsupported replies fail before dispatching tools.

## ChatGPT subscription

```sh
./target/release/zeus auth login chatgpt
./target/release/zeus auth status chatgpt
./target/release/zeus models chatgpt
```

Login opens the system browser. Finish authorization there; do not paste tokens into chat, job files, or the terminal. The model-list command returns the current signed-in account's catalog rather than a bundled model list. Use one returned `id` in your job:

```json
{
  "auth": "chatgpt",
  "protocol": "responses",
  "endpoint": "https://api.openai.com/v1/responses",
  "model": "an-id-returned-by-zeus-models-chatgpt"
}
```

Subscription credentials are pinned to the exact public Responses endpoint. Endpoint overrides, other protocols, and `api_key_env` are rejected in subscription mode. A subscription usage limit or authentication failure is an error, not permission to charge an API account. Normal task execution still requires `--allow-network`.

One active ChatGPT registration is supported. `auth login chatgpt --new-account` explicitly starts account replacement; the previous record is not replaced until the new identity and permissions are validated. `auth logout chatgpt` clears local credentials; it does not claim remote revocation. See [AUTH.md](AUTH.md) for storage, OAuth validation, refresh behavior, and sources.

Access tokens are refreshed if needed at the start of a task. That task retains its selected token/account rather than switching identities mid-run. If it expires during a long run, the run fails as an infrastructure error; start another run after refreshing/signing in. No model-weight training or subscription-limit bypass is provided.

## Verification boundary

Protocol and OAuth flows are exercised with local HTTP fixtures. The local Qwen reconstruction exercise is separately documented in `examples/reconstruction/README.md`. A real user's ChatGPT browser authorization and subscription inference have not been completed by those fixture tests. API-key inference is also not implied by protocol tests. These require the user's credentials, permission and provider entitlement.

Claude API support is separate from Claude subscriptions. Zeus does not collect Claude.ai session/OAuth tokens or impersonate Claude Code. Anthropic's documented third-party integration path is API-key/provider access; running the unmodified Claude Code product is a different integration, not this native runtime.

## Primary provider references

- OpenAI subscription sign-in: https://developers.openai.com/siwc/token-sharing-open-source/sign-in
- Subscription model listing and inference: https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference
- Anthropic credential-use restrictions: https://code.claude.com/docs/en/legal-and-compliance
