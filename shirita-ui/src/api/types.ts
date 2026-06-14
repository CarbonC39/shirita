export type Role = 'system' | 'user' | 'assistant'

export interface Session {
  id: string
  name: string
  avatar: string | null
  template_id?: string | null
  override_config: Record<string, unknown>
  current_state: Record<string, unknown>
  mounted_definitions: string[]
}

export interface Message {
  id: string
  session_id: string
  parent_id: string | null
  role: Role
  raw_content: string
  display_content: string | null
  is_hidden: boolean
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

export interface PromptNode {
  id: string
  owner_kind: 'template' | 'session'
  owner_id: string
  parent_id: string | null
  sort_order: number
  kind: 'folder' | 'ref' | 'history'
  tag: string | null
  definition_id: string | null
  enabled: boolean
  created_at: string
}

export interface DefType {
  id: string
  label: string
  sort: number
  builtin: boolean
  created_at: string
}

export interface RegexRule {
  id: string
  name: string
  pattern: string
  replacement: string
  enabled: boolean
  scope: { ai_output: boolean; user_input: boolean; display_only: boolean }
}
