# Modelrouter

Alpha local-first AI routing control plane for subscription CLIs, local models, and API-backed providers.

Modelrouter is a Rust smart router for sending AI work to the cheapest capable provider: local models, subscription CLIs such as Codex/Claude/Gemini, or OpenAI-compatible local/API endpoints.

The default policy is intentionally subscription-first for code and reasoning, local-first for private or low-risk summarization, and configurable in `modelrouter.toml`.

## Status

This project is early alpha. It is designed for localhost use and assumes you trust the machine running the daemon. Provider CLI behavior, model names, and billing APIs can change. Spend tracking and budget controls are guardrails, not financial guarantees.

## What It Routes On

- Task type inferred from the prompt or `--hint`: simple, code, deep reasoning, or writing.
- Provider capabilities: codebase editing, tests, long context, privacy, local execution, writing, reasoning.
- Billing mode: `local`, `subscription`, or `api`.
- Estimated input/output tokens and optional max cost.
- Provider availability in daemon mode through CLI PATH checks or OpenAI-compatible `/models` checks.
- User routing rules and project profiles.
- Actual token usage from OpenAI-compatible API responses when the provider returns `usage`.
- Actual spend reports from OpenAI costs, Anthropic cost reports, and Google Cloud Billing export data.

## Providers

Built-in provider ids:

- `local`: Ollama via `ollama run <model>`.
- `codex`: Codex subscription CLI via `codex exec`.
- `claude`: Claude subscription CLI via `claude --print`.
- `gemini`: Gemini subscription CLI via `gemini`.
- `lmstudio`: OpenAI-compatible local endpoint at `http://localhost:1234/v1`.
- `llamacpp`: OpenAI-compatible local endpoint at `http://localhost:8080/v1`.
- `openai_compatible`: configurable OpenAI-compatible endpoint.
- `aider`: agent CLI route for codebase work.

Copy `modelrouter.example.toml` to `modelrouter.toml` and adjust providers, models, and endpoints.

## CLI

Route without running a model:

```sh
cargo run -- route --prompt "Fix the failing parser tests in this repo" --json
```

Route and run the selected provider:

```sh
cargo run -- run --prompt "Summarize this private note."
```

Check provider health:

```sh
cargo run -- health --json
```

Write JSONL request logs:

```sh
cargo run -- route --prompt "Draft a short memo" --log .modelrouter/requests.jsonl
```

Show local spend/request metrics from the JSONL log:

```sh
cargo run -- spend report --log .modelrouter/requests.jsonl --json
```

Apply project profiles by passing `--cwd` or by running from a matching directory:

```sh
cargo run -- route --prompt "Fix the parser" --cwd /path/to/repo --json
```

## Routing Rules And Profiles

Rules force a preferred provider when a condition matches:

```toml
[[rules]]
name = "private-work-stays-local"
prefer = "local"

[rules.when]
private = true
```

Profiles override routing defaults for matching project paths:

```toml
[[profiles]]
name = "my-rust-repo"
path_contains = "my-rust-repo"
code_provider = "codex"
default_provider = "claude"
```

API-backed providers can be blocked by a monthly budget:

```toml
[budget]
monthly_api_budget_cents = 5000.0
```

Subscription and local providers do not count against this budget.

## Spend Tracking

Modelrouter tracks spend in three layers:

- Estimated request cost before a route runs.
- Exact token usage from API provider responses when `usage` is present.
- Actual provider spend from official billing/cost APIs or billing export data.

OpenAI actual spend request:

```sh
OPENAI_ADMIN_KEY=... cargo run -- spend sync-openai \
  --start-time 1779676800 \
  --end-time 1782268800
```

Anthropic actual spend request:

```sh
ANTHROPIC_ADMIN_KEY=... cargo run -- spend sync-anthropic \
  --starting-at 2026-05-01T00:00:00Z \
  --ending-at 2026-06-01T00:00:00Z
```

Gemini spend is handled through Google Cloud Billing export. Generate the BigQuery SQL with:

```sh
cargo run -- spend google-query \
  --table '`billing.gcp_billing_export_v1_ABCDEF`' \
  --start-date 2026-05-01 \
  --end-date 2026-06-01
```

## Daemon And GUI

Start the local daemon:

```sh
cargo run -- serve --host 127.0.0.1 --port 8787 --log .modelrouter/requests.jsonl
```

Endpoints:

- `GET /`: local HTML GUI.
- `GET /health`: provider health report.
- `GET /metrics`: rollup from the request log when `--log` is configured.
- `GET /spend`: alias for spend/request metrics.
- `GET /budget`: configured API budget.
- `GET /queue`: queued job list.
- `POST /route`: routing decision.
- `POST /run`: routing decision plus provider output.
- `POST /queue`: route and run through the daemon queue.
- `POST /feedback`: append route feedback next to the request log.
- `POST /v1/chat/completions`: non-streaming OpenAI-compatible proxy endpoint.

Example route request:

```sh
curl -s http://127.0.0.1:8787/route \
  -H 'content-type: application/json' \
  -d '{"prompt":"Fix the failing Rust tests","hint":"code"}'
```

Point OpenAI-compatible clients at the daemon:

```sh
curl -s http://127.0.0.1:8787/v1/chat/completions \
  -H 'content-type: application/json' \
  -d '{"model":"modelrouter","messages":[{"role":"user","content":"Fix the failing tests"}]}'
```

Compression is available as a library primitive today through `compress_prompt`; it preserves prompt edges plus action/tag lines and reports estimated token savings. The next natural daemon upgrade is automatic compression policy per endpoint/provider.

## MCP

Start the MCP stdio server:

```sh
cargo run -- mcp --config modelrouter.toml
```

The MCP server exposes:

- `modelrouter_route`: returns a route decision without running a model.
- `modelrouter_run`: routes and runs the selected provider.
- `modelrouter_broker`: routes, runs, and returns both decision metadata and output.

Example MCP client config shape:

```json
{
  "mcpServers": {
    "modelrouter": {
      "command": "/absolute/path/to/modelrouter",
      "args": ["mcp", "--config", "/absolute/path/to/modelrouter.toml"]
    }
  }
}
```

## Development

Warnings are failures. The main validation command is:

```sh
make ci
```

Individual commands:

```sh
make fmt-check
make typecheck
make test
make lint
```

Coverage is available with:

```sh
cargo install cargo-llvm-cov
make coverage
```
