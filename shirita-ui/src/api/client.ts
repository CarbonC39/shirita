import type { Message, Session } from './types'

const BASE = import.meta.env.VITE_API_BASE ?? ''
const TOKEN = import.meta.env.VITE_API_TOKEN ?? ''

function authHeaders(extra: Record<string, string> = {}): Record<string, string> {
  return { Authorization: `Bearer ${TOKEN}`, ...extra }
}

export async function apiGet<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}/api${path}`, { headers: authHeaders() })
  if (!res.ok) {
    throw new Error(`GET ${path} failed: ${res.status}`)
  }
  return (await res.json()) as T
}

export function listSessions(): Promise<Session[]> {
  return apiGet<Session[]>('/sessions')
}

export function listMessages(sessionId: string): Promise<Message[]> {
  return apiGet<Message[]>(`/sessions/${sessionId}/messages`)
}

// --- SSE streaming ---

export type SseEvent =
  | { type: 'delta'; text: string }
  | { type: 'done'; message_id: string }
  | { type: 'error'; message: string }

export async function* sendMessage(
  sessionId: string,
  text: string,
): AsyncGenerator<SseEvent> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/messages`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ text }),
  })
  if (!res.ok) {
    throw new Error(`POST /sessions/${sessionId}/messages failed: ${res.status}`)
  }
  if (!res.body) {
    throw new Error('No response body for SSE stream')
  }

  const reader = res.body.getReader()
  const decoder = new TextDecoder()
  let buffer = ''

  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      buffer += decoder.decode(value, { stream: true })
      const lines = buffer.split('\n')
      buffer = lines.pop() || ''
      for (const line of lines) {
        if (line.startsWith('data: ')) {
          yield JSON.parse(line.slice(6)) as SseEvent
        }
      }
    }
  } finally {
    reader.releaseLock()
  }
}
