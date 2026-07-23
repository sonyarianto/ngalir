export interface FlowSpec {
  version: number
  name: string
  description?: string
  concurrency?: number
  nodes: NodeSpec[]
  notes?: NoteSpec[]
}

export interface NodeSpec {
  id: string
  use: string
  with?: Record<string, unknown>
  inputs?: Record<string, string>
  when?: string
  on_error?: string
  exit?: boolean
}

export interface NoteSpec {
  id: string
  text: string
  position: { x: number; y: number }
  width: number
  height: number
  color: string
}

export interface Position {
  x: number
  y: number
}

export interface CanvasNode extends NodeSpec {
  position: Position
  selected?: boolean
  status?: 'pending' | 'running' | 'done' | 'failed'
  input?: Record<string, unknown>
  output?: Record<string, unknown>
  error?: string
}

export interface CanvasNote extends NoteSpec {
  selected?: boolean
}

export interface Port {
  nodeId: string
  port: string
  label: string
  type: 'input' | 'output'
}

export interface Wire {
  id: string
  from: Port
  to: Port
}

export interface NodeManifest {
  name: string
  version: string
  description: string
  inputs: Record<string, unknown>
  outputs: Record<string, unknown>
  secrets: string[]
  credentials: CredentialSpec[]
  streaming: boolean
  idempotent: boolean
  output_mode: string | null
  use_cases: string[]
  see_also: string[]
}

export interface CredentialSpec {
  id: string
  label: string
  auth_type: 'api_key' | 'basic_auth' | 'oauth2' | 'custom'
  fields: CredentialField[]
  oauth?: OAuthConfig
}

export interface CredentialField {
  key: string
  label: string
  input_type: string
  required: boolean
}

export interface OAuthConfig {
  authorize_url: string
  token_url: string
  scopes: string[]
  client_id_env: string
}

export interface Credential {
  id: string
  credential_spec_id: string
  label: string
  auth_type: string
  data: Record<string, string>
  created_at: string
  updated_at: string
}

export interface HistoryRun {
  flow_id: string
  flow_name: string
  status: string
  started_at: string
  duration_ms: number | null
  node_count: number
  error?: string
}

export interface HistoryNode {
  node_id: string
  node_type: string
  status: string
  duration_ms: number | null
  error?: string
}

export const CATEGORIES = [
  { name: 'Core', nodes: ['echo', 'file', 'http', 'jsonpath', 'vault'] },
  { name: 'Database', nodes: ['db-postgres', 'db-mysql', 'db-sqlite'] },
  { name: 'Data', nodes: ['csv', 'excel', 'google-sheets'] },
  { name: 'AI', nodes: ['llm'] },
  { name: 'Triggers', nodes: ['webhook', 'schedule', 'email'] },
] as const
