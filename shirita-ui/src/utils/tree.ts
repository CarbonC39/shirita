import type { Message, PromptNode } from '../api/types'

function newest(messages: Message[]): Message | null {
  if (messages.length === 0) return null
  return messages.reduce((a, b) =>
    a.created_at > b.created_at || (a.created_at === b.created_at && a.id > b.id) ? a : b,
  )
}

/** Root→active-leaf branch. Falls back to the newest message when leaf unknown. */
export function activePath(messages: Message[], activeLeafId: string | null): Message[] {
  const byId = new Map(messages.map((m) => [m.id, m]))
  let cur: Message | null = (activeLeafId ? byId.get(activeLeafId) : undefined) ?? newest(messages)
  const path: Message[] = []
  while (cur) {
    path.push(cur)
    cur = cur.parent_id ? byId.get(cur.parent_id) ?? null : null
  }
  return path.reverse()
}

/** When `nodeId` (a ref) is about to be enabled inside a `select=one` folder,
 *  the ids of its currently-enabled ref siblings that should be turned off.
 *  Empty unless the parent is a folder with `meta.select === 'one'`. */
export function selectOneSiblingsToDisable(nodes: PromptNode[], nodeId: string): string[] {
  const node = nodes.find((n) => n.id === nodeId)
  if (!node || !node.parent_id) return []
  const parent = nodes.find((n) => n.id === node.parent_id)
  if (!parent || parent.kind !== 'folder') return []
  if ((parent.meta as Record<string, unknown>).select !== 'one') return []
  return nodes
    .filter((n) => n.parent_id === parent.id && n.id !== nodeId && n.kind === 'ref' && n.enabled)
    .map((n) => n.id)
}

/** Same-parent siblings of `msg`, ordered created_at asc then id (swipe order). */
export function siblings(messages: Message[], msg: Message): Message[] {
  return messages
    .filter((m) => (m.parent_id ?? null) === (msg.parent_id ?? null))
    .sort((a, b) => a.created_at.localeCompare(b.created_at) || a.id.localeCompare(b.id))
}
