import { describe, it, expect } from 'vitest'
import en from './en'
import zhHans from './zh-Hans'
import zhHant from './zh-Hant'
import ja from './ja'

/** Recursively collect dotted leaf-key paths from a nested message object. */
function leafKeys(obj: Record<string, unknown>, prefix = ''): string[] {
  return Object.entries(obj).flatMap(([k, v]) => {
    const path = prefix ? `${prefix}.${k}` : k
    return v !== null && typeof v === 'object'
      ? leafKeys(v as Record<string, unknown>, path)
      : [path]
  })
}

const enKeys = leafKeys(en).sort()

describe.each([
  ['zh-Hans', zhHans],
  ['zh-Hant', zhHant],
  ['ja', ja],
])('%s key parity with en', (_name, catalog) => {
  it('has exactly the same key set as en', () => {
    expect(leafKeys(catalog as Record<string, unknown>).sort()).toEqual(enKeys)
  })
})
