export type Role = 'system' | 'user' | 'assistant'

export type AgentTransport = 'auto' | 'native' | 'xml'

export interface AgentSettings {
  enabled: boolean
  transport: AgentTransport
  enabled_tools: string[]
  max_rounds: number
  max_tool_calls: number
  tool_timeout_ms: number
  show_activity: boolean
  show_user_status: boolean
  max_identical_call_rounds: number
  system_prompt: string
  unfinished_prompt: string
}

export interface AgentToolSpec {
  name: string
  description: string
  input_schema: Record<string, unknown>
  output_schema: Record<string, unknown> | null
  source: 'builtin' | 'plugin' | 'mcp'
  required: boolean
}

export interface AgentSettingsView {
  global: AgentSettings
  override: AgentSettings | null
  effective: AgentSettings
  limits: {
    hard_max_rounds: number
    hard_max_tool_calls: number
    hard_max_tool_timeout_ms: number
    max_tool_argument_bytes: number
    max_tool_result_bytes: number
    max_finish_response_bytes: number
    max_xml_round_bytes: number
    max_agent_prompt_bytes: number
    max_status_message_bytes: number
    max_random_items: number
    max_random_integer_span: number
    max_math_expression_bytes: number
    max_math_parse_depth: number
  }
  tools: AgentToolSpec[]
}

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

/** A `variables` brick's meta payload: its declared variables. */
export interface VariablesMeta {
  decls: VarDecl[]
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

/** A server-resolved panel for a session: one `panel` folder's combined
 *  html/css plus its caps. Returned by GET /sessions/:id/panels. */
export interface SessionPanel {
  id: string
  name: string
  html: string
  css: string
  caps: PanelCaps
  /** Hide the panel header until the chat has at least this many messages. 0 = always show. */
  min_messages: number
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
  kind: 'definition' | 'template' | 'pack' | 'panel'
  id: string
  name: string
}

export interface ImportSummary {
  created: ImportItem[]
  skipped: ImportItem[]
  overwritten: ImportItem[]
}

export type OnConflict = 'skip' | 'overwrite' | 'duplicate'
