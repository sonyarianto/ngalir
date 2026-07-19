export interface FlowSpec {
  version: number
  name: string
  description?: string
  concurrency?: number
  nodes: NodeSpec[]
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
  streaming: boolean
  idempotent: boolean
  output_mode: string | null
  use_cases: string[]
  see_also: string[]
}

export const CATEGORIES = [
  { name: 'Core', nodes: ['echo', 'file', 'http', 'jsonpath', 'vault'] },
  { name: 'Database', nodes: ['db-postgres', 'db-mysql', 'db-sqlite'] },
  { name: 'Data', nodes: ['csv', 'excel', 'google-sheets'] },
  { name: 'AI', nodes: ['llm'] },
  { name: 'Triggers', nodes: ['webhook', 'schedule', 'email'] },
] as const
