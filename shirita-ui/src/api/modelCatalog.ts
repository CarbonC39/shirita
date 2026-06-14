// Per-source fallback model lists used when no API key is present (so we can't
// query the provider's live /models endpoint). These are convenience defaults —
// the Model field is always free-text, so anything missing can still be typed.
export const fallbackModels: Record<string, string[]> = {
  openai: ['gpt-4o', 'gpt-4o-mini', 'gpt-4.1', 'gpt-4.1-mini', 'o3', 'o4-mini', 'gpt-4-turbo', 'gpt-3.5-turbo'],
  anthropic: ['claude-opus-4-8', 'claude-sonnet-4-6', 'claude-haiku-4-5', 'claude-3-7-sonnet-latest', 'claude-3-5-haiku-latest'],
  google: ['gemini-2.5-pro', 'gemini-2.5-flash', 'gemini-2.0-flash', 'gemini-1.5-pro', 'gemini-1.5-flash'],
  openrouter: ['anthropic/claude-opus-4-8', 'openai/gpt-4o', 'google/gemini-2.5-pro', 'meta-llama/llama-3.3-70b-instruct', 'deepseek/deepseek-chat'],
  mistral: ['mistral-large-latest', 'mistral-medium-latest', 'mistral-small-latest', 'open-mistral-nemo', 'codestral-latest'],
  deepseek: ['deepseek-chat', 'deepseek-reasoner'],
  groq: ['llama-3.3-70b-versatile', 'llama-3.1-8b-instant', 'mixtral-8x7b-32768', 'gemma2-9b-it'],
  xai: ['grok-4', 'grok-3', 'grok-3-mini', 'grok-2-1212'],
  cohere: ['command-a', 'command-r-plus', 'command-r'],
  together: ['meta-llama/Llama-3.3-70B-Instruct-Turbo', 'meta-llama/Meta-Llama-3.1-8B-Instruct-Turbo', 'mistralai/Mixtral-8x7B-Instruct-v0.1'],
  perplexity: ['sonar', 'sonar-pro', 'sonar-reasoning', 'sonar-reasoning-pro'],
  custom: [],
}
