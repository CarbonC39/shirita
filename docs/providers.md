# Provider 配置

后端按环境变量 `PROVIDER` 选择模型适配器（默认 OpenAI 兼容；无 key 则离线 Echo）。

| PROVIDER | 端点 | 必要 env |
|---|---|---|
| (空) / 其它 | `OPENAI_BASE_URL`（默认 OpenAI） | `OPENAI_API_KEY`、`OPENAI_MODEL` |
| `ollama` | `OLLAMA_BASE_URL`（默认 `http://localhost:11434/v1`） | `OPENAI_MODEL`（如 `llama3`） |
| `anthropic` | `ANTHROPIC_BASE_URL`（默认 `https://api.anthropic.com`） | `OPENAI_API_KEY`（即 Anthropic key）、`OPENAI_MODEL`（如 `claude-sonnet-4-6`） |

- **Ollama**：复用 OpenAI 兼容端点 `/v1/chat/completions`，无需新代码——`PROVIDER=ollama` 即可。
- **Anthropic**：走 `/v1/messages`，滚动摘要作为 `<history_summary>` user 消息插到可见历史最前（见 M6 spec §4）。
