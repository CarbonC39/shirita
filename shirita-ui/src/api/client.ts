import type { Definition, Message, PromptNode, Session, Template } from './types'

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

// --- Sessions ---
export async function createSession(name: string, templateId?: string | null): Promise<Session> {
  const res = await fetch(`${BASE}/api/sessions`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ name, template_id: templateId || undefined }),
  })
  if (!res.ok) throw new Error(`Create session failed: ${res.status}`)
  return res.json()
}

// --- Definitions ---
export function listDefinitions(type?: string): Promise<Definition[]> {
  const qs = type ? `?type=${encodeURIComponent(type)}` : ''
  return apiGet<Definition[]>(`/definitions${qs}`)
}

export async function createDefinition(body: { type: string; name: string; content: string; meta?: Record<string, unknown> }): Promise<Definition> {
  const res = await fetch(`${BASE}/api/definitions`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Create definition failed: ${res.status}`)
  return res.json()
}

export async function updateDefinition(id: string, body: { type?: string; name?: string; content?: string; meta?: Record<string, unknown> }): Promise<Definition> {
  const res = await fetch(`${BASE}/api/definitions/${id}`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Update definition failed: ${res.status}`)
  return res.json()
}

export async function deleteDefinition(id: string): Promise<void> {
  const res = await fetch(`${BASE}/api/definitions/${id}`, { method: 'DELETE', headers: authHeaders() })
  if (!res.ok) throw new Error(`Delete definition failed: ${res.status}`)
}

// --- Templates ---
export function listTemplates(): Promise<Template[]> { return apiGet<Template[]>('/templates') }

export async function createTemplate(name: string): Promise<Template> {
  const res = await fetch(`${BASE}/api/templates`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ name }),
  })
  if (!res.ok) throw new Error(`Create template failed: ${res.status}`)
  return res.json()
}

export async function deleteTemplate(id: string): Promise<void> {
  const res = await fetch(`${BASE}/api/templates/${id}`, { method: 'DELETE', headers: authHeaders() })
  if (!res.ok) throw new Error(`Delete template failed: ${res.status}`)
}

export async function duplicateTemplate(id: string): Promise<Template> {
  const res = await fetch(`${BASE}/api/templates/${id}/duplicate`, { method: 'POST', headers: authHeaders() })
  if (!res.ok) throw new Error(`Duplicate template failed: ${res.status}`)
  return res.json()
}

// --- Prompt Nodes ---
export function listNodes(ownerKind: string, ownerId: string): Promise<PromptNode[]> {
  return apiGet<PromptNode[]>(`/templates/${ownerId}/nodes?owner_kind=${ownerKind}`)
}

export async function createNode(ownerKind: string, ownerId: string, body: { parent_id?: string | null; kind: string; tag?: string; definition_id?: string }): Promise<PromptNode> {
  const res = await fetch(`${BASE}/api/templates/${ownerId}/nodes?owner_kind=${ownerKind}`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Create node failed: ${res.status}`)
  return res.json()
}

export async function updateNode(nodeId: string, body: { parent_id?: string | null; sort_order?: number; tag?: string; definition_id?: string; enabled?: boolean }): Promise<PromptNode> {
  const res = await fetch(`${BASE}/api/nodes/${nodeId}`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Update node failed: ${res.status}`)
  return res.json()
}

export async function deleteNode(nodeId: string): Promise<void> {
  const res = await fetch(`${BASE}/api/nodes/${nodeId}`, { method: 'DELETE', headers: authHeaders() })
  if (!res.ok) throw new Error(`Delete node failed: ${res.status}`)
}

// --- Settings ---
export async function getSettings(): Promise<Record<string, unknown>> {
  return apiGet<Record<string, unknown>>('/settings')
}

export async function updateSettings(settings: Record<string, unknown>): Promise<void> {
  const res = await fetch(`${BASE}/api/settings`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify(settings),
  })
  if (!res.ok) throw new Error(`Update settings failed: ${res.status}`)
}

export async function testProviderConnection(): Promise<{ ok: boolean; error?: string }> {
  const res = await fetch(`${BASE}/api/provider/test`, { method: 'POST', headers: authHeaders() })
  if (!res.ok) throw new Error(`Provider test failed: ${res.status}`)
  return res.json()
}
