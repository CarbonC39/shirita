# Provider Configuration

Shirita supports multiple AI model providers. Configuration is set in Settings → Provider (each source keeps its own API key, base URL, and model), or via environment variables as a fallback.

## Supported sources

| Source | Adapter | Default base URL |
|--------|---------|------------------|
| OpenAI | OpenAI-compatible `/v1/chat/completions` | `https://api.openai.com/v1` |
| Anthropic | `/v1/messages` (native API) | `https://api.anthropic.com` |
| Google | OpenAI-compatible `/v1` | `https://generativelanguage.googleapis.com/v1beta` |
| OpenRouter | OpenAI-compatible | `https://openrouter.ai/api/v1` |
| Mistral | OpenAI-compatible | `https://api.mistral.ai/v1` |
| DeepSeek | OpenAI-compatible | `https://api.deepseek.com/v1` |
| Groq | OpenAI-compatible | `https://api.groq.com/openai/v1` |
| xAI | OpenAI-compatible | `https://api.x.ai/v1` |
| Cohere | OpenAI-compatible | `https://api.cohere.ai/v1` |
| Together | OpenAI-compatible | `https://api.together.xyz/v1` |
| Perplexity | OpenAI-compatible | `https://api.perplexity.ai` |
| Ollama (local) | OpenAI-compatible | `http://localhost:11434/v1` |
| Custom… | User-defined base URL | — |

## Environment fallback (desktop first-launch, or when no settings are configured)

| Variable | Default | Purpose |
|----------|---------|---------|
| `PROVIDER` | *(empty = OpenAI compat)* | Selects adapter: `anthropic`, `ollama`, or empty |
| `OPENAI_API_KEY` | — | API key (also used for Anthropic) |
| `OPENAI_BASE_URL` | `https://api.openai.com/v1` | Base URL override |
| `OPENAI_MODEL` | `gpt-4o` | Model name |
| `ANTHROPIC_BASE_URL` | `https://api.anthropic.com` | Anthropic-only (ignored unless `PROVIDER=anthropic`) |
| `OLLAMA_BASE_URL` | `http://localhost:11434/v1` | Ollama-only (ignored unless `PROVIDER=ollama`) |

When UI settings exist, they take priority over env vars.

## Adapter behavior

- **Ollama**: Uses the OpenAI-compatible `/v1/chat/completions` endpoint with a fixed `"ollama"` API key (Ollama doesn't validate it). No adapter code needed — `PROVIDER=ollama` suffices.
- **Anthropic**: Uses the native `/v1/messages` API. Rolling summaries are injected as `<history_summary>` prepended to the first user message (see M6 spec §4).
- **Echo** (offline): When no API key is configured and no provider env is set, Shirita uses an offline `EchoProvider` that mirrors input — useful for testing and development without network access.
