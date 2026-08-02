import type {
  Definition,
  DefType,
  Identity,
  ImportSummary,
  Message,
  OnConflict,
  Pack,
  PackIdentity,
  PromptNode,
  Session,
  SessionPanel,
  SessionState,
  Template,
  VarDecl,
  AgentSettings,
  AgentSettingsView,
} from './types'

// BASE (the embedded server origin, injected by Tauri / build-time for web) is
// static for the process lifetime. The session TOKEN is NOT — it changes on
// login/logout, so it is read live via a registered accessor set up in main.ts
// (see setAuthAccessor) rather than captured once at module load.
const RT = (globalThis as { __SHIRITA_RUNTIME__?: { base?: string; token?: string } }).__SHIRITA_RUNTIME__
const BASE = RT?.base ?? import.meta.env.VITE_API_BASE ?? ''

let tokenAccessor: () => string | null = () => RT?.token ?? null
let onUnauthorized: (() => void) | null = null

/** Wire up the live token source + the 401 handler. Called once from main.ts
 *  after the auth store exists, to avoid a circular import (client ← store). */
export function setAuthAccessor(getToken: () => string | null, on401: () => void) {
  tokenAccessor = getToken
  onUnauthorized = on401
}

function liveToken(): string | null {
  return tokenAccessor()
}

/** `Authorization: Bearer <token>` when a token is held, else `{}` (for the
 *  public login call). */
function authHeader(): Record<string, string> {
  const t = liveToken()
  return t ? { Authorization: `Bearer ${t}` } : {}
}

/** Central JSON fetch against `/api/*`. Injects the live Bearer token and, on a
 *  401, fires the registered handler (clears the session + redirects to login).
 *  Returns the Response regardless; callers still do their own `!res.ok` check
 *  for non-401 failures. */
async function apiFetch(path: string, init: RequestInit = {}): Promise<Response> {
  const headers: Record<string, string> = {
    ...authHeader(),
    ...((init.headers as Record<string, string>) || {}),
  }
  const res = await fetch(`${BASE}/api${path}`, { ...init, headers })
  if (res.status === 401 && onUnauthorized) onUnauthorized()
  return res
}

/** Append `?t=<token>` to an `/assets/` URL so `<img>` (which can't send a
 *  Bearer header) authenticates via the query param the asset middleware
 *  accepts. Non-asset URLs and unauthenticated contexts pass through unchanged. */
function signAsset(url: string): string {
  const t = liveToken()
  if (t && url.includes('/assets/')) {
    return `${url}${url.includes('?') ? '&' : '?'}t=${encodeURIComponent(t)}`
  }
  return url
}

// Build a displayable URL for a stored asset's relative path (e.g. `avatar.png`).
// On the web build, BASE is '' and a host-relative `/assets/<rel>` resolves fine
// against the page's own origin. In Tauri, the webview's page origin is
// `tauri://localhost` (prod) or the dev server origin — neither of which is the
// embedded Axum server's origin (`http://127.0.0.1:<port>`, injected as BASE via
// `window.__SHIRITA_RUNTIME__`). A bare relative URL would resolve against the
// wrong origin and fail to load ("Load failed" / broken image). Always prefix
// with BASE so the request reaches the embedded server regardless of the page's
// own origin, and sign it so the asset gate accepts it.
export function assetUrl(relativePath: string): string {
  if (!relativePath) return ''
  const rel = relativePath.startsWith('/') ? relativePath : `/assets/${relativePath}`
  return signAsset(`${BASE}${rel}`)
}

// --- Auth ---
export interface AuthUser {
  id: string
  username: string
  has_password: boolean
}
export interface LoginResponse {
  token: string
  user: AuthUser
  expires_at: string
}

export async function authLogin(username: string, password: string): Promise<LoginResponse> {
  const res = await apiFetch('/auth/login', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username, password }),
  })
  if (!res.ok) throw new Error(`Login failed: ${res.status}`)
  return res.json()
}

export async function authMe(): Promise<AuthUser> {
  const res = await apiFetch('/auth/me')
  if (!res.ok) throw new Error(`me failed: ${res.status}`)
  return res.json()
}

/** Best-effort logout — ignore network failure, the local token is cleared
 *  regardless. */
export async function authLogout(): Promise<void> {
  try {
    await apiFetch('/auth/logout', { method: 'POST' })
  } catch {
    /* best-effort */
  }
}

export async function authChangePassword(current: string, next: string): Promise<void> {
  const res = await apiFetch('/auth/password', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ current, new: next }),
  })
  if (!res.ok) throw new Error(`Change password failed: ${res.status}`)
}

export async function apiGet<T>(path: string): Promise<T> {
  const res = await apiFetch(path)
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
  const res = await apiFetch(`/sessions/${id}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Patch session failed: ${res.status}`)
  return res.json()
}

export async function setLocalVariables(sessionId: string, variables: VarDecl[]): Promise<void> {
  const res = await apiFetch(`/sessions/${sessionId}/local-variables`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
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
  | { type: 'stopped'; message_id?: string }
  | { type: 'activity'; round: number; message: string }
  | { type: 'run_start'; run_id: string }
  | { type: 'tool_start'; call_id: string; name: string }
  | { type: 'tool_result'; call_id: string; name: string; status: string }
  | { type: 'finish'; run_id: string }
  | { type: 'status'; message: string; visibility?: 'internal' | 'user' }
  | { type: 'usage'; input_tokens: number; output_tokens: number }
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
  signal?: AbortSignal,
): AsyncGenerator<SseEvent> {
  const res = await apiFetch(`/sessions/${sessionId}/messages`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ text, attachments }),
    signal,
  })
  if (!res.ok) {
    throw new Error(`POST /sessions/${sessionId}/messages failed: ${res.status}`)
  }
  yield* readSse(res)
}

/** Cooperatively stop the in-flight generation for a session (Stop button /
 *  navigate-away). The backend persists partial text before ending the stream. */
export async function abortSession(sessionId: string): Promise<void> {
  try {
    await apiFetch(`/sessions/${sessionId}/abort`, { method: 'POST' })
  } catch {
    // Best-effort: the SSE stream's own AbortController is the real kill switch.
  }
}

export async function editMessage(
  sessionId: string,
  msgId: string,
  patch: { content?: string; is_hidden?: boolean },
): Promise<Message> {
  const res = await apiFetch(`/sessions/${sessionId}/messages/${msgId}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(patch),
  })
  if (!res.ok) throw new Error(`Edit message failed: ${res.status}`)
  return res.json()
}

/** Delete a message and its subtree; returns the session's new active leaf id (may be null). */
export async function deleteMessage(sessionId: string, msgId: string): Promise<{ activeLeafId: string | null }> {
  const res = await apiFetch(`/sessions/${sessionId}/messages/${msgId}`, { method: 'DELETE' })
  if (!res.ok) throw new Error(`Delete message failed: ${res.status}`)
  return res.json()
}

export async function setActiveLeaf(sessionId: string, messageId: string): Promise<Session> {
  const res = await apiFetch(`/sessions/${sessionId}/active-leaf`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ message_id: messageId }),
  })
  if (!res.ok) throw new Error(`Set active leaf failed: ${res.status}`)
  return res.json()
}

export async function forkSession(sessionId: string, messageId: string): Promise<Session> {
  const res = await apiFetch(`/sessions/${sessionId}/fork`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ message_id: messageId }),
  })
  if (!res.ok) throw new Error(`Fork failed: ${res.status}`)
  return res.json()
}

// --- copy-on-write (local definition / template overrides) ---
export async function setLocalDefinition(sessionId: string, defId: string, patch: Record<string, unknown>): Promise<void> {
  const res = await apiFetch(`/sessions/${sessionId}/local-definitions/${defId}`, {
    method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(patch),
  })
  if (!res.ok) throw new Error(`Set local definition failed: ${res.status}`)
}
export async function clearLocalDefinition(sessionId: string, defId: string): Promise<void> {
  const res = await apiFetch(`/sessions/${sessionId}/local-definitions/${defId}`, { method: 'DELETE' })
  if (!res.ok) throw new Error(`Clear local definition failed: ${res.status}`)
}
export async function promoteLocalDefinition(sessionId: string, defId: string): Promise<void> {
  const res = await apiFetch(`/sessions/${sessionId}/local-definitions/${defId}/promote`, { method: 'POST' })
  if (!res.ok) throw new Error(`Promote failed: ${res.status}`)
}
export async function materializeNodes(sessionId: string): Promise<void> {
  const res = await apiFetch(`/sessions/${sessionId}/materialize-nodes`, { method: 'POST' })
  if (!res.ok) throw new Error(`Materialize nodes failed: ${res.status}`)
}

export async function materializePackNodes(sessionId: string, packId: string): Promise<void> {
  const res = await apiFetch(`/sessions/${sessionId}/materialize-pack`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ pack_id: packId }),
  })
  if (!res.ok) throw new Error(`Materialize pack nodes failed: ${res.status}`)
}

/** SSE regenerate — same event shape as sendMessage. */
export async function* regenerateMessage(
  sessionId: string,
  msgId: string,
  signal?: AbortSignal,
): AsyncGenerator<SseEvent> {
  const res = await apiFetch(`/sessions/${sessionId}/messages/${msgId}/regenerate`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: '{}',
    signal,
  })
  if (!res.ok) throw new Error(`Regenerate failed: ${res.status}`)
  yield* readSse(res)
}

// --- Sessions ---
export async function createSession(
  name: string,
  templateId?: string | null,
  avatar?: string | null,
  packIds: string[] = [],
): Promise<Session> {
  const res = await apiFetch('/sessions', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name, template_id: templateId || undefined, avatar: avatar || undefined, pack_ids: packIds }),
  })
  if (!res.ok) throw new Error(`Create session failed: ${res.status}`)
  return res.json()
}

export async function deleteSession(id: string): Promise<void> {
  const res = await apiFetch(`/sessions/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error(`Delete session failed: ${res.status}`)
}

export async function duplicateSession(id: string): Promise<Session> {
  const res = await apiFetch(`/sessions/${id}/duplicate`, { method: 'POST' })
  if (!res.ok) throw new Error(`Duplicate session failed: ${res.status}`)
  return res.json()
}

export async function reorderSessions(ids: string[]): Promise<void> {
  const res = await apiFetch('/sessions/reorder', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ids }),
  })
  if (!res.ok) throw new Error(`Reorder sessions failed: ${res.status}`)
}

export function exportSession(id: string): Promise<unknown> {
  return apiGet<unknown>(`/sessions/${id}/export`)
}

export async function importSession(body: unknown): Promise<Session> {
  const res = await apiFetch('/sessions/import', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
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
  const res = await apiFetch('/definitions', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Create definition failed: ${res.status}`)
  return res.json()
}

export async function updateDefinition(id: string, body: { type?: string; name?: string; content?: string; meta?: Record<string, unknown> }): Promise<Definition> {
  const res = await apiFetch(`/definitions/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Update definition failed: ${res.status}`)
  return res.json()
}

export async function deleteDefinition(id: string): Promise<void> {
  const res = await apiFetch(`/definitions/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error(`Delete definition failed: ${res.status}`)
}

// --- Templates ---
export function listTemplates(): Promise<Template[]> { return apiGet<Template[]>('/templates') }

export async function createTemplate(name: string): Promise<Template> {
  const res = await apiFetch('/templates', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name }),
  })
  if (!res.ok) throw new Error(`Create template failed: ${res.status}`)
  return res.json()
}

export async function updateTemplate(id: string, name: string, meta?: Record<string, unknown>): Promise<Template> {
  const res = await apiFetch(`/templates/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(meta === undefined ? { name } : { name, meta }),
  })
  if (!res.ok) throw new Error(`Update template failed: ${res.status}`)
  return res.json()
}

export async function deleteTemplate(id: string, deleteOrphans = false): Promise<void> {
  const qs = deleteOrphans ? '?delete_orphans=true' : ''
  const res = await apiFetch(`/templates/${id}${qs}`, { method: 'DELETE' })
  if (!res.ok) throw new Error(`Delete template failed: ${res.status}`)
}

export async function getOrphanDefinitions(templateId: string): Promise<Definition[]> {
  const res = await apiFetch(`/templates/${templateId}/orphan-definitions`)
  if (!res.ok) throw new Error(`Get orphan definitions failed: ${res.status}`)
  return res.json()
}

export async function duplicateTemplate(id: string): Promise<Template> {
  const res = await apiFetch(`/templates/${id}/duplicate`, { method: 'POST' })
  if (!res.ok) throw new Error(`Duplicate template failed: ${res.status}`)
  return res.json()
}

// --- Prompt Nodes ---
export function listNodes(ownerKind: string, ownerId: string): Promise<PromptNode[]> {
  return apiGet<PromptNode[]>(`/templates/${ownerId}/nodes?owner_kind=${ownerKind}`)
}

export async function createNode(ownerKind: string, ownerId: string, body: { parent_id?: string | null; kind: string; tag?: string; definition_id?: string }): Promise<PromptNode> {
  const res = await apiFetch(`/templates/${ownerId}/nodes?owner_kind=${ownerKind}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Create node failed: ${res.status}`)
  return res.json()
}

export async function updateNode(nodeId: string, body: { parent_id?: string | null; sort_order?: number; tag?: string; definition_id?: string; enabled?: boolean; meta?: Record<string, unknown> }): Promise<PromptNode> {
  const res = await apiFetch(`/nodes/${nodeId}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Update node failed: ${res.status}`)
  return res.json()
}

export async function deleteNode(nodeId: string): Promise<void> {
  const res = await apiFetch(`/nodes/${nodeId}`, { method: 'DELETE' })
  if (!res.ok) throw new Error(`Delete node failed: ${res.status}`)
}

// --- Types (container type registry) ---
export function listTypes(): Promise<DefType[]> { return apiGet<DefType[]>('/types') }

export async function createType(body: { id: string; label: string; sort?: number }): Promise<DefType> {
  const res = await apiFetch('/types', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Create type failed: ${res.status}`)
  return res.json()
}

export async function deleteType(id: string): Promise<void> {
  const res = await apiFetch(`/types/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error(`Delete type failed: ${res.status}`)
}

export async function reorderNodes(ownerKind: string, ownerId: string, orderedIds: string[]): Promise<void> {
  const res = await apiFetch(`/templates/${ownerId}/nodes/reorder?owner_kind=${ownerKind}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ordered_ids: orderedIds }),
  })
  if (!res.ok) throw new Error(`Reorder nodes failed: ${res.status}`)
}

// --- Settings ---
export async function getSettings(): Promise<Record<string, unknown>> {
  return apiGet<Record<string, unknown>>('/settings')
}

export async function updateSettings(settings: Record<string, unknown>): Promise<void> {
  const res = await apiFetch('/settings', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(settings),
  })
  if (!res.ok) throw new Error(`Update settings failed: ${res.status}`)
}

async function writeAgentSettings(path: string, method: 'PUT' | 'DELETE', settings?: AgentSettings): Promise<AgentSettingsView> {
  const res = await apiFetch(path, {
    method,
    ...(settings && { headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(settings) }),
  })
  if (!res.ok) throw new Error(`${method} ${path} failed: ${res.status}`)
  return res.json()
}

export function getAgentSettings(): Promise<AgentSettingsView> {
  return apiGet<AgentSettingsView>('/agent-settings')
}

export function updateAgentSettings(settings: AgentSettings): Promise<AgentSettingsView> {
  return writeAgentSettings('/agent-settings', 'PUT', settings)
}

export function resetAgentSettings(): Promise<AgentSettingsView> {
  return writeAgentSettings('/agent-settings', 'DELETE')
}

export function getSessionAgentSettings(sessionId: string): Promise<AgentSettingsView> {
  return apiGet<AgentSettingsView>(`/sessions/${sessionId}/agent-settings`)
}

export function updateSessionAgentSettings(sessionId: string, settings: AgentSettings): Promise<AgentSettingsView> {
  return writeAgentSettings(`/sessions/${sessionId}/agent-settings`, 'PUT', settings)
}

export function resetSessionAgentSettings(sessionId: string): Promise<AgentSettingsView> {
  return writeAgentSettings(`/sessions/${sessionId}/agent-settings`, 'DELETE')
}

export async function testProviderConnection(): Promise<{ ok: boolean; error?: string }> {
  const res = await apiFetch('/provider/test', { method: 'POST' })
  if (!res.ok) throw new Error(`Provider test failed: ${res.status}`)
  return res.json()
}

export async function fetchProviderModels(): Promise<{ data?: Array<{ id: string }>; error?: string }> {
  const res = await apiFetch('/provider/models')
  if (!res.ok) throw new Error(`Fetch models failed: ${res.status}`)
  return res.json()
}

// --- Media library (assets) ---
export interface Asset { id: string; name: string; path: string; kind: string; url: string }

// The server returns `url` as a host-relative path (e.g. `/assets/<file>`); see
// `assetUrl` above for why this must be rewritten against BASE for Tauri, and
// `signAsset` for why it must carry the session token.
function withAbsoluteUrl(a: Asset): Asset {
  const base = BASE && a.url.startsWith('/') ? `${BASE}${a.url}` : a.url
  return { ...a, url: signAsset(base) }
}

export async function listAssets(kind?: string): Promise<Asset[]> {
  const assets = await apiGet<Asset[]>('/assets' + (kind ? '?kind=' + kind : ''))
  return assets.map(withAbsoluteUrl)
}

// Upload an image (or any file) to the library; returns the new asset record.
// `kind` determines the library it belongs to ("avatar" or "background").
export async function uploadAsset(file: File, kind = 'background'): Promise<Asset> {
  const form = new FormData()
  form.append('file', file)
  const qs = kind ? `?kind=${kind}` : ''
  const res = await apiFetch(`/assets${qs}`, { method: 'POST', body: form })
  if (!res.ok) throw new Error(`Asset upload failed: ${res.status}`)
  return withAbsoluteUrl(await res.json())
}

export async function renameAsset(id: string, name: string): Promise<void> {
  const res = await apiFetch(`/assets/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name }),
  })
  if (!res.ok) throw new Error(`Rename asset failed: ${res.status}`)
}

export async function deleteAsset(id: string): Promise<void> {
  const res = await apiFetch(`/assets/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error(`Delete asset failed: ${res.status}`)
}

export async function importFile(file: File, onConflict: OnConflict = 'skip'): Promise<ImportSummary> {
  const form = new FormData()
  form.append('file', file)
  const res = await apiFetch(`/import?on_conflict=${onConflict}`, {
    method: 'POST',
    body: form, // 不要手动设 Content-Type：浏览器会带 boundary
  })
  if (!res.ok) throw new Error(`import failed: ${res.status}`)
  return res.json()
}

// 带鉴权地拉取一个导出端点并触发浏览器下载（鉴权走 header，不能直接 window.open）。
export async function downloadExport(path: string, filename: string): Promise<void> {
  const res = await apiFetch(path)
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

// Pack export's filename (.zip vs .json) is server-decided, so read it from
// Content-Disposition rather than passing a fixed name; fall back to <name>.zip.
export async function downloadPackExport(id: string, name: string): Promise<void> {
  const res = await apiFetch(`/packs/${id}/export`)
  if (!res.ok) throw new Error(`export failed: ${res.status}`)
  const cd = res.headers.get('content-disposition') ?? ''
  const m = cd.match(/filename="?([^"]+)"?/)
  const filename = m?.[1] ?? `${name || 'pack'}.zip`
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

export async function applyStateUpdates(
  sessionId: string,
  updates: { action: string; key: string; value?: string | null }[],
): Promise<{ values: Record<string, unknown> }> {
  const res = await apiFetch(`/sessions/${sessionId}/state-updates`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ updates }),
  })
  if (!res.ok) throw new Error(`State update failed: ${res.status}`)
  return res.json()
}

// --- Packs ---
export function listPacks(): Promise<Pack[]> { return apiGet<Pack[]>('/packs') }

export function getPack(id: string): Promise<Pack> { return apiGet<Pack>(`/packs/${id}`) }

export async function createPack(body: { name: string; identity?: PackIdentity; meta?: Record<string, unknown> }): Promise<Pack> {
  const res = await apiFetch('/packs', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Create pack failed: ${res.status}`)
  return res.json()
}

export async function updatePack(id: string, body: { name: string; identity?: PackIdentity; meta?: Record<string, unknown> }): Promise<Pack> {
  const res = await apiFetch(`/packs/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`Update pack failed: ${res.status}`)
  return res.json()
}

export async function deletePack(id: string, deleteOrphans = false): Promise<void> {
  const qs = deleteOrphans ? '?delete_orphans=true' : ''
  const res = await apiFetch(`/packs/${id}${qs}`, { method: 'DELETE' })
  if (!res.ok) throw new Error(`Delete pack failed: ${res.status}`)
}

export async function getOrphanDefinitionsForPack(packId: string): Promise<Definition[]> {
  const res = await apiFetch(`/packs/${packId}/orphan-definitions`)
  if (!res.ok) throw new Error(`Get orphan definitions failed: ${res.status}`)
  return res.json()
}

export async function duplicatePack(id: string): Promise<Pack> {
  const res = await apiFetch(`/packs/${id}/duplicate`, { method: 'POST' })
  if (!res.ok) throw new Error(`Duplicate pack failed: ${res.status}`)
  return res.json()
}

// --- Panels ---
export function getSessionPanels(sessionId: string): Promise<SessionPanel[]> {
  return apiGet<SessionPanel[]>(`/sessions/${sessionId}/panels`)
}
