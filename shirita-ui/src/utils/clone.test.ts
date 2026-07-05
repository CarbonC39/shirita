import { describe, it, expect } from 'vitest'
import { deepClone } from './clone'

describe('deepClone', () => {
  it('severs nested references so mutating the clone does not affect the source', () => {
    const source = { meta: { trigger: { keys: ['a'] } }, content: 'x' }
    const copy = deepClone(source)
    copy.meta.trigger.keys.push('b')
    copy.content = 'y'
    expect(source.meta.trigger.keys).toEqual(['a'])
    expect(source.content).toBe('x')
  })
})
