export type Role = 'system' | 'user' | 'assistant'

export interface Session {
  id: string
  name: string
  avatar: string | null
  template_id?: string | null
  override_config: Record<string, unknown>
  current_state: Record<string, unknown>
  mounted_definitions: string[]
  mounted_packs?: string[]
  created_at?: string
  updated_at?: string
  /** Leaf message of the active branch (set by the message-tree endpoints). */
  active_leaf_id?: string | null
  /** Snippet of the most recent message, supplied by the session list. */
  preview?: string
}

export interface Message {
  id: string
  session_id: string
  parent_id: string | null
  role: Role
  raw_content: string
  display_content: string | null
  is_hidden: boolean
  /** Synthetic anchor turn: kept in the prompt, hidden from the UI. */
  is_anchor: boolean
  /** Asset ids attached to this message (currently images). */
  attachments: string[]
  snapshot_state: Record<string, unknown>
  created_at: string
}

export interface Definition {
  id: string
  type: string
  name: string
  content: string
  meta: Record<string, unknown>
}

export interface Template {
  id: string
  name: string
  meta: Record<string, unknown>
  created_at: string
  updated_at: string
}

export type VarType = 'number' | 'bool' | 'string' | 'list'

export interface VarDecl {
  name: string
  type: VarType
  initial: unknown
  /** 'system' | 'template' | 'local' — for UI grouping. */
  scope?: string
}

export interface SessionState {
  schema: VarDecl[]
  values: Record<string, unknown>
}

export interface PromptNode {
  id: string
  owner_kind: 'template' | 'session' | 'pack'
  owner_id: string
  parent_id: string | null
  sort_order: number
  kind: 'folder' | 'ref' | 'history' | 'content'
  tag: string | null
  definition_id: string | null
  enabled: boolean
  created_at: string
  meta: Record<string, unknown>
}

/** A pack's bound display identity (mirrors core PackIdentity; empty == unset). */
export interface PackIdentity {
  display_name: string | null
  avatar: string | null
}

/** A content bundle: its own node tree plus an optional bound identity. */
export interface Pack {
  id: string
  name: string
  identity: PackIdentity
  meta: Record<string, unknown>
  created_at: string
  updated_at: string
}

/** The non-read capability tiers a panel declares it uses. v1: declared == granted. */
export interface PanelCaps {
  write?: boolean
  insert?: boolean
  send?: boolean
}

/** A Pack's status panel: author HTML + scoped CSS + declared capabilities.
 *  Lives at `pack.meta.panel`; absent == the Pack has no panel. */
export interface Panel {
  html: string
  css: string
  caps: PanelCaps
}

/** A user interaction reported by a panel; the host decides whether to honor it. */
export type PanelAction =
  | { kind: 'diff'; key: string; op: string; value: string | null }
  | { kind: 'insert'; text: string }
  | { kind: 'send'; text: string }

export interface DefType {
  id: string
  label: string
  sort: number
  builtin: boolean
  created_at: string
}

export interface Trigger {
  mode: 'constant' | 'keyword' | 'random'
  keys: string[]
  probability: number
}

/** Read a normalized Trigger out of a definition's meta.trigger (lenient). */
export function triggerFromMeta(meta: Record<string, unknown>): Trigger {
  const t = (meta?.trigger ?? {}) as Partial<Trigger>
  return {
    mode: t.mode === 'keyword' || t.mode === 'random' ? t.mode : 'constant',
    keys: Array.isArray(t.keys) ? t.keys.filter((k): k is string => typeof k === 'string') : [],
    probability: typeof t.probability === 'number' ? t.probability : 100,
  }
}

export interface RegexRule {
  id: string
  name: string
  pattern: string
  replacement: string
  enabled: boolean
  scope: { ai_output: boolean; user_input: boolean; phase: 'display' | 'both' | 'prompt' }
}

export interface RegexScope {
  id: string
  scope: 'global' | 'template'
  template_names: string[]
  pattern_error: string | null
}

export interface SideIdentity {
  name: string | null
  avatar: string | null
}

export interface Identity {
  assistant: SideIdentity
  user: SideIdentity
}

export interface ImportItem {
  kind: 'definition' | 'template' | 'pack'
  id: string
  name: string
}

export interface ImportSummary {
  created: ImportItem[]
  skipped: ImportItem[]
  overwritten: ImportItem[]
}

export type OnConflict = 'skip' | 'overwrite' | 'duplicate'
