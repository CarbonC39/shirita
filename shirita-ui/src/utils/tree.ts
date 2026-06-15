import type { Message } from '../api/types'

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

/** Same-parent siblings of `msg`, ordered created_at asc then id (swipe order). */
export function siblings(messages: Message[], msg: Message): Message[] {
  return messages
    .filter((m) => (m.parent_id ?? null) === (msg.parent_id ?? null))
    .sort((a, b) => a.created_at.localeCompare(b.created_at) || a.id.localeCompare(b.id))
}
