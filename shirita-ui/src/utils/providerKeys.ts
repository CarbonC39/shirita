// Per-source provider settings keys. The backend stores each provider's config
// under `provider.<source>.<field>` so switching the active source never
// clobbers another's base URL / API key / model (see resolve_provider_config).
export type ProviderField = 'base_url' | 'api_key' | 'model'

export const providerKey = (source: string, field: ProviderField): string =>
  `provider.${source}.${field}`
