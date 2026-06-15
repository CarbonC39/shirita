import { describe, it, expect } from 'vitest'
import { activePath, siblings } from './tree'
import type { Message } from '../api/types'

function m(id: string, parent: string | null, created: string, role: Message['role'] = 'assistant'): Message {
  return { id, session_id: 's', parent_id: parent, role, raw_content: id, display_content: null, is_hidden: false, snapshot_state: {}, created_at: created }
}

describe('activePath', () => {
  it('walks root to the active leaf', () => {
    const ms = [m('a', null, '1', 'user'), m('b', 'a', '2'), m('b2', 'a', '3')]
    expect(activePath(ms, 'b2').map((x) => x.id)).toEqual(['a', 'b2'])
  })
  it('falls back to the newest message when leaf is null', () => {
    const ms = [m('a', null, '1', 'user'), m('b', 'a', '2')]
    expect(activePath(ms, null).map((x) => x.id)).toEqual(['a', 'b'])
  })
})

describe('siblings', () => {
  it('lists same-parent nodes ordered by created_at', () => {
    const ms = [m('a', null, '1', 'user'), m('b', 'a', '3'), m('b2', 'a', '2')]
    const sib = siblings(ms, ms[1])
    expect(sib.map((x) => x.id)).toEqual(['b2', 'b'])
  })
})
