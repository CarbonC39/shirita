import { describe, it, expect } from 'vitest'
import { providerKey } from './providerKeys'

describe('providerKey', () => {
  it('namespaces per source', () => {
    expect(providerKey('anthropic', 'api_key')).toBe('provider.anthropic.api_key')
    expect(providerKey('openai', 'model')).toBe('provider.openai.model')
    expect(providerKey('ollama', 'base_url')).toBe('provider.ollama.base_url')
  })
})
