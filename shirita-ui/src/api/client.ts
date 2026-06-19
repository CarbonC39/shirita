import type {
  Definition,
  DefType,
  Identity,
  ImportSummary,
  Message,
  OnConflict,
  PromptNode,
  Session,
  SessionState,
  Template,
  VarDecl,
} from './types'

const RT = (globalThis as { __SHIRITA_RUNTIME__?: { base?: string; token?: string } }).__SHIRITA_RUNTIME__
const BASE = RT?.base ?? import.meta.env.VITE_API_BASE ?? ''
const TOKEN = RT?.token ?? import.meta.env.VITE_API_TOKEN ?? ''

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

export function getSession(id: string): Promise<Session> {
  return apiGet<Session>(`/sessions/${id}`)
}

export function getSessionState(id: string): Promise<SessionState> {
  return apiGet<SessionState>(`/sessions/${id}/state`)
}

export function getSessionIdentity(id: string): Promise<Identity> {
  return apiGet<Identity>(`/sessions/${id}/identity`)
}

export async function patchSession(id: string, body: { name?: string; avatar?: string | null }): Promise<Session> {
  const res = await fetch(`${BASE}/api/sessions/${id}`, {
    method: 'PATCH',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Patch session failed: ${res.status}`)
  return res.json()
}

export async function setLocalVariables(sessionId: string, variables: VarDecl[]): Promise<void> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/local-variables`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ variables }),
  })
  if (!res.ok) throw new Error(`Set local variables failed: ${res.status}`)
}

export function listMessages(sessionId: string): Promise<Message[]> {
  return apiGet<Message[]>(`/sessions/${sessionId}/messages`)
}

// --- SSE streaming ---

export type SseEvent =
  | { type: 'delta'; text: string }
  | { type: 'done'; message_id: string }
  | { type: 'error'; message: string }

/** Parse an `data: {...}\n` SSE body into a stream of `SseEvent`s. */
async function* readSse(res: Response): AsyncGenerator<SseEvent> {
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

export async function* sendMessage(
  sessionId: string,
  text: string,
  attachments: string[] = [],
): AsyncGenerator<SseEvent> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/messages`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ text, attachments }),
  })
  if (!res.ok) {
    throw new Error(`POST /sessions/${sessionId}/messages failed: ${res.status}`)
  }
  yield* readSse(res)
}

export async function editMessage(
  sessionId: string,
  msgId: string,
  patch: { content?: string; is_hidden?: boolean },
): Promise<Message> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/messages/${msgId}`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify(patch),
  })
  if (!res.ok) throw new Error(`Edit message failed: ${res.status}`)
  return res.json()
}

export async function setActiveLeaf(sessionId: string, messageId: string): Promise<Session> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/active-leaf`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ message_id: messageId }),
  })
  if (!res.ok) throw new Error(`Set active leaf failed: ${res.status}`)
  return res.json()
}

export async function forkSession(sessionId: string, messageId: string): Promise<Session> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/fork`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ message_id: messageId }),
  })
  if (!res.ok) throw new Error(`Fork failed: ${res.status}`)
  return res.json()
}

// --- copy-on-write (local definition / template overrides) ---
export async function setLocalDefinition(sessionId: string, defId: string, patch: Record<string, unknown>): Promise<void> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/local-definitions/${defId}`, {
    method: 'PUT', headers: { ...authHeaders(), 'Content-Type': 'application/json' }, body: JSON.stringify(patch),
  })
  if (!res.ok) throw new Error(`Set local definition failed: ${res.status}`)
}
export async function clearLocalDefinition(sessionId: string, defId: string): Promise<void> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/local-definitions/${defId}`, { method: 'DELETE', headers: authHeaders() })
  if (!res.ok) throw new Error(`Clear local definition failed: ${res.status}`)
}
export async function promoteLocalDefinition(sessionId: string, defId: string): Promise<void> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/local-definitions/${defId}/promote`, { method: 'POST', headers: authHeaders() })
  if (!res.ok) throw new Error(`Promote failed: ${res.status}`)
}
export async function materializeNodes(sessionId: string): Promise<void> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/materialize-nodes`, { method: 'POST', headers: authHeaders() })
  if (!res.ok) throw new Error(`Materialize nodes failed: ${res.status}`)
}

/** SSE regenerate — same event shape as sendMessage. */
export async function* regenerateMessage(
  sessionId: string,
  msgId: string,
): AsyncGenerator<SseEvent> {
  const res = await fetch(`${BASE}/api/sessions/${sessionId}/messages/${msgId}/regenerate`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: '{}',
  })
  if (!res.ok) throw new Error(`Regenerate failed: ${res.status}`)
  yield* readSse(res)
}

// --- Sessions ---
export async function createSession(name: string, templateId?: string | null, avatar?: string | null): Promise<Session> {
  const res = await fetch(`${BASE}/api/sessions`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ name, template_id: templateId || undefined, avatar: avatar || undefined }),
  })
  if (!res.ok) throw new Error(`Create session failed: ${res.status}`)
  return res.json()
}

export async function deleteSession(id: string): Promise<void> {
  const res = await fetch(`${BASE}/api/sessions/${id}`, { method: 'DELETE', headers: authHeaders() })
  if (!res.ok) throw new Error(`Delete session failed: ${res.status}`)
}

export async function duplicateSession(id: string): Promise<Session> {
  const res = await fetch(`${BASE}/api/sessions/${id}/duplicate`, { method: 'POST', headers: authHeaders() })
  if (!res.ok) throw new Error(`Duplicate session failed: ${res.status}`)
  return res.json()
}

export async function reorderSessions(ids: string[]): Promise<void> {
  const res = await fetch(`${BASE}/api/sessions/reorder`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ ids }),
  })
  if (!res.ok) throw new Error(`Reorder sessions failed: ${res.status}`)
}

export function exportSession(id: string): Promise<unknown> {
  return apiGet<unknown>(`/sessions/${id}/export`)
}

export async function importSession(body: unknown): Promise<Session> {
  const res = await fetch(`${BASE}/api/sessions/import`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Import session failed: ${res.status}`)
  return res.json()
}

// --- Definitions ---
export function listDefinitions(type?: string): Promise<Definition[]> {
  const qs = type ? `?type=${encodeURIComponent(type)}` : ''
  return apiGet<Definition[]>(`/definitions${qs}`)
}

export function getRegexScopes(): Promise<import('./types').RegexScope[]> {
  return apiGet<import('./types').RegexScope[]>('/regex-rules/scopes')
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

export async function updateTemplate(id: string, name: string, meta?: Record<string, unknown>): Promise<Template> {
  const res = await fetch(`${BASE}/api/templates/${id}`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify(meta === undefined ? { name } : { name, meta }),
  })
  if (!res.ok) throw new Error(`Update template failed: ${res.status}`)
  return res.json()
}

export async function deleteTemplate(id: string, deleteOrphans = false): Promise<void> {
  const qs = deleteOrphans ? '?delete_orphans=true' : ''
  const res = await fetch(`${BASE}/api/templates/${id}${qs}`, { method: 'DELETE', headers: authHeaders() })
  if (!res.ok) throw new Error(`Delete template failed: ${res.status}`)
}

export async function getOrphanDefinitions(templateId: string): Promise<Definition[]> {
  const res = await fetch(`${BASE}/api/templates/${templateId}/orphan-definitions`, { headers: authHeaders() })
  if (!res.ok) throw new Error(`Get orphan definitions failed: ${res.status}`)
  return res.json()
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

export async function updateNode(nodeId: string, body: { parent_id?: string | null; sort_order?: number; tag?: string; definition_id?: string; enabled?: boolean; meta?: Record<string, unknown> }): Promise<PromptNode> {
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

// --- Types (container type registry) ---
export function listTypes(): Promise<DefType[]> { return apiGet<DefType[]>('/types') }

export async function createType(body: { id: string; label: string; sort?: number }): Promise<DefType> {
  const res = await fetch(`${BASE}/api/types`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Create type failed: ${res.status}`)
  return res.json()
}

export async function deleteType(id: string): Promise<void> {
  const res = await fetch(`${BASE}/api/types/${id}`, { method: 'DELETE', headers: authHeaders() })
  if (!res.ok) throw new Error(`Delete type failed: ${res.status}`)
}

export async function reorderNodes(ownerKind: string, ownerId: string, orderedIds: string[]): Promise<void> {
  const res = await fetch(`${BASE}/api/templates/${ownerId}/nodes/reorder?owner_kind=${ownerKind}`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ ordered_ids: orderedIds }),
  })
  if (!res.ok) throw new Error(`Reorder nodes failed: ${res.status}`)
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

export async function fetchProviderModels(): Promise<{ data?: Array<{ id: string }>; error?: string }> {
  const res = await fetch(`${BASE}/api/provider/models`, { headers: authHeaders() })
  if (!res.ok) throw new Error(`Fetch models failed: ${res.status}`)
  return res.json()
}

// --- Media library (assets) ---
export interface Asset { id: string; name: string; path: string; kind: string; url: string }

export function listAssets(kind?: string): Promise<Asset[]> {
  return apiGet<Asset[]>('/assets' + (kind ? '?kind=' + kind : ''))
}

// Upload an image (or any file) to the library; returns the new asset record.
// `kind` determines the library it belongs to ("avatar" or "background").
export async function uploadAsset(file: File, kind = 'background'): Promise<Asset> {
  const form = new FormData()
  form.append('file', file)
  const qs = kind ? `?kind=${kind}` : ''
  const res = await fetch(`${BASE}/api/assets${qs}`, { method: 'POST', headers: authHeaders(), body: form })
  if (!res.ok) throw new Error(`Asset upload failed: ${res.status}`)
  return res.json()
}

export async function renameAsset(id: string, name: string): Promise<void> {
  const res = await fetch(`${BASE}/api/assets/${id}`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ name }),
  })
  if (!res.ok) throw new Error(`Rename asset failed: ${res.status}`)
}

export async function deleteAsset(id: string): Promise<void> {
  const res = await fetch(`${BASE}/api/assets/${id}`, { method: 'DELETE', headers: authHeaders() })
  if (!res.ok) throw new Error(`Delete asset failed: ${res.status}`)
}

export async function importFile(file: File, onConflict: OnConflict = 'skip'): Promise<ImportSummary> {
  const form = new FormData()
  form.append('file', file)
  const res = await fetch(`${BASE}/api/import?on_conflict=${onConflict}`, {
    method: 'POST',
    headers: authHeaders(), // 不要手动设 Content-Type：浏览器会带 boundary
    body: form,
  })
  if (!res.ok) throw new Error(`import failed: ${res.status}`)
  return res.json()
}

// 带鉴权地拉取一个导出端点并触发浏览器下载（鉴权走 header，不能直接 window.open）。
export async function downloadExport(path: string, filename: string): Promise<void> {
  const res = await fetch(`${BASE}/api${path}`, { headers: authHeaders() })
  if (!res.ok) throw new Error(`export failed: ${res.status}`)
  const blob = await res.blob()
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  document.body.appendChild(a)
  a.click()
  a.remove()
  URL.revokeObjectURL(url)
}

export function exportDefinitionPath(id: string): string {
  return `/definitions/${id}/export`
}

export function exportTemplatePath(id: string): string {
  return `/templates/${id}/export`
}
