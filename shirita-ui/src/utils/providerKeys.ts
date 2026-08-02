// Per-source provider settings keys. The backend stores each provider's config
// under `provider.<source>.<field>` so switching the active source never
// clobbers another's base URL / API key / model (see resolve_provider_config).
export type ProviderField = 'base_url' | 'api_key' | 'model' | 'native_tools'

export const providerKey = (source: string, field: ProviderField): string =>
  `provider.${source}.${field}`

export const defaultProviderBaseUrls: Record<string, string> = {
  openai: 'https://api.openai.com/v1',
  anthropic: 'https://api.anthropic.com',
  google: 'https://generativelanguage.googleapis.com/v1beta',
  openrouter: 'https://openrouter.ai/api/v1',
  mistral: 'https://api.mistral.ai/v1',
  deepseek: 'https://api.deepseek.com/v1',
  groq: 'https://api.groq.com/openai/v1',
  xai: 'https://api.x.ai/v1',
  cohere: 'https://api.cohere.ai/v1',
  together: 'https://api.together.xyz/v1',
  perplexity: 'https://api.perplexity.ai',
  ollama: 'http://localhost:11434/v1',
  custom: '',
}
