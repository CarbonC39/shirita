import { describe, it, expect } from 'vitest'
import { metaToRule, scopeFlagsToMeta } from './regexRule'
import type { Definition } from '../api/types'

function def(meta: Record<string, unknown>): Definition {
  return { id: 'r1', type: 'regex_rule', name: 'R', content: '', meta }
}

describe('metaToRule (canonical meta -> editor view)', () => {
  it('treats a rule as enabled unless meta.disabled is true', () => {
    expect(metaToRule(def({})).enabled).toBe(true)
    expect(metaToRule(def({ disabled: false })).enabled).toBe(true)
    expect(metaToRule(def({ disabled: true })).enabled).toBe(false)
  })

  it('reads pattern/replacement with empty-string fallbacks', () => {
    const r = metaToRule(def({ pattern: 'a', replacement: 'b' }))
    expect(r.pattern).toBe('a')
    expect(r.replacement).toBe('b')
    expect(metaToRule(def({})).pattern).toBe('')
  })

  it('maps scope string to the display/both/prompt phase', () => {
    expect(metaToRule(def({})).scope.phase).toBe('display')
    expect(metaToRule(def({ scope: 'display' })).scope.phase).toBe('display')
    expect(metaToRule(def({ scope: 'both' })).scope.phase).toBe('both')
    expect(metaToRule(def({ scope: 'prompt' })).scope.phase).toBe('prompt')
  })

  it('reads phase back from meta', () => {
    const rule = metaToRule(def({ pattern: 'x', replacement: '', scope: 'both', targets: ['ai_output'] }))
    expect(rule.scope).toEqual({ ai_output: true, user_input: false, phase: 'both' })
  })

  it('treats missing/empty targets as ai_output (broad)', () => {
    expect(metaToRule(def({})).scope.ai_output).toBe(true)
    expect(metaToRule(def({ targets: [] })).scope.ai_output).toBe(true)
    expect(metaToRule(def({ targets: ['user_input'] })).scope.ai_output).toBe(false)
  })

  it('reads user_input from targets', () => {
    expect(metaToRule(def({ targets: ['user_input'] })).scope.user_input).toBe(true)
    expect(metaToRule(def({ targets: ['ai_output'] })).scope.user_input).toBe(false)
  })
})

describe('scopeFlagsToMeta (editor view -> canonical meta)', () => {
  it('passes the phase through as the scope string', () => {
    expect(scopeFlagsToMeta({ ai_output: true, user_input: false, phase: 'display' }).scope).toBe('display')
    expect(scopeFlagsToMeta({ ai_output: true, user_input: false, phase: 'both' }).scope).toBe('both')
    expect(scopeFlagsToMeta({ ai_output: true, user_input: false, phase: 'prompt' }).scope).toBe('prompt')
  })

  it('maps prompt scope round-trip', () => {
    const meta = scopeFlagsToMeta({ ai_output: false, user_input: true, phase: 'prompt' })
    expect(meta).toEqual({ scope: 'prompt', targets: ['user_input'] })
  })

  it('builds the targets array from the checkboxes', () => {
    expect(scopeFlagsToMeta({ ai_output: true, user_input: true, phase: 'display' }).targets).toEqual(['ai_output', 'user_input'])
    expect(scopeFlagsToMeta({ ai_output: false, user_input: true, phase: 'display' }).targets).toEqual(['user_input'])
    expect(scopeFlagsToMeta({ ai_output: false, user_input: false, phase: 'display' }).targets).toEqual([])
  })

  it('round-trips through metaToRule', () => {
    const flags = { ai_output: false, user_input: true, phase: 'both' as const }
    const m = scopeFlagsToMeta(flags)
    const back = metaToRule(def({ scope: m.scope, targets: m.targets })).scope
    expect(back).toEqual(flags)
  })
})
