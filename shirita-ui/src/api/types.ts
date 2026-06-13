export type Role = 'system' | 'user' | 'assistant'

export interface Session {
  id: string
  name: string
  avatar: string | null
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
