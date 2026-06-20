import { describe, it, expect } from 'vitest'
import { activePath, siblings, selectOneSiblingsToDisable } from './tree'
import type { Message, PromptNode } from '../api/types'

function m(id: string, parent: string | null, created: string, role: Message['role'] = 'assistant'): Message {
  return { id, session_id: 's', parent_id: parent, role, raw_content: id, display_content: null, is_hidden: false, is_anchor: false, attachments: [], snapshot_state: {}, created_at: created }
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

function pnode(p: Partial<PromptNode>): PromptNode {
  return { id: 'x', owner_kind: 'pack', owner_id: 'o', parent_id: null, sort_order: 0,
    kind: 'ref', tag: null, definition_id: 'd', enabled: true, created_at: '', meta: {}, ...p }
}

describe('selectOneSiblingsToDisable', () => {
  const folderOne = pnode({ id: 'f', kind: 'folder', definition_id: null, meta: { select: 'one' } })
  const folderAll = pnode({ id: 'g', kind: 'folder', definition_id: null, meta: {} })

  it('returns enabled siblings under a select=one folder', () => {
    const nodes = [folderOne,
      pnode({ id: 'a', parent_id: 'f', enabled: true }),
      pnode({ id: 'b', parent_id: 'f', enabled: true }),
      pnode({ id: 'c', parent_id: 'f', enabled: false })]
    expect(selectOneSiblingsToDisable(nodes, 'a')).toEqual(['b'])
  })

  it('returns nothing for an all-select folder', () => {
    const nodes = [folderAll, pnode({ id: 'a', parent_id: 'g' }), pnode({ id: 'b', parent_id: 'g' })]
    expect(selectOneSiblingsToDisable(nodes, 'a')).toEqual([])
  })

  it('returns nothing for a root node (no parent)', () => {
    const nodes = [pnode({ id: 'a', parent_id: null })]
    expect(selectOneSiblingsToDisable(nodes, 'a')).toEqual([])
  })
})
