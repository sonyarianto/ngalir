import { type CanvasNode, type Wire } from './types'

let nodes = $state<CanvasNode[]>([])
let wires = $state<Wire[]>([])
let selectedId = $state<string | null>(null)
let flowName = $state('untitled')
let filename = $state('')

function addNode(type: string, position: { x: number; y: number }) {
  const id = `${type}-${nodes.length + 1}`
  nodes.push({
    id,
    use: type,
    with: {},
    inputs: {},
    position,
    selected: false,
  })
  selectNode(id)
}

function removeNode(id: string) {
  nodes = nodes.filter((n) => n.id !== id)
  wires = wires.filter((w) => w.from.nodeId !== id && w.to.nodeId !== id)
  if (selectedId === id) selectedId = null
}

function selectNode(id: string | null) {
  for (const n of nodes) n.selected = n.id === id
  selectedId = id
}

function updateNodePosition(id: string, position: { x: number; y: number }) {
  const n = nodes.find((n) => n.id === id)
  if (n) n.position = position
}

function updateNodeProp(id: string, key: string, value: unknown) {
  const n = nodes.find((n) => n.id === id)
  if (n) {
    if (!n.with) n.with = {}
    ;(n.with as Record<string, unknown>)[key] = value
  }
}

function addWire(wire: Wire) {
  wires = [...wires, wire]
}

function removeWire(id: string) {
  wires = wires.filter((w) => w.id !== id)
}

function exportFlow(): string {
  const flow = {
    version: 1,
    name: flowName,
    nodes: nodes.map(({ id, use, with: w, inputs, when, on_error, exit }) => ({
      id,
      use,
      ...(w && Object.keys(w).length ? { with: w } : {}),
      ...(inputs && Object.keys(inputs).length ? { inputs } : {}),
      ...(when ? { when } : {}),
      ...(on_error ? { on_error } : {}),
      ...(exit ? { exit } : {}),
    })),
  }
  return JSON.stringify(flow, null, 2)
}

function importFlow(text: string) {
  try {
    const flow = JSON.parse(text)
    flowName = flow.name || 'untitled'
    nodes = (flow.nodes || []).map((n: CanvasNode, i: number) => ({
      ...n,
      position: n.position || { x: 100 + i * 40, y: 100 + i * 80 },
      with: n.with || {},
      inputs: n.inputs || {},
    }))
    wires = []
  } catch {
    alert('Invalid flow JSON')
  }
}

function loadSample() {
  flowName = 'etl-demo'
  nodes = [
    { id: 'src', use: 'db-postgres', with: { connection: 'vault://db/prod', query: 'SELECT * FROM orders' }, inputs: {}, position: { x: 60, y: 80 } },
    { id: 'transform', use: 'jsonpath', with: { filter: '[] | {id, amount}' }, inputs: { data: 'src.rows' }, position: { x: 420, y: 80 } },
    { id: 'notify', use: 'email', with: { to: 'ops@example.com', subject: 'ETL done' }, inputs: { body: 'transform.result' }, when: '{{ transform.result | length > 0 }}', position: { x: 780, y: 80 } },
  ]
  wires = [
    { id: 'w1', from: { nodeId: 'src', port: 'rows', label: 'rows', type: 'output' }, to: { nodeId: 'transform', port: 'data', label: 'data', type: 'input' } },
    { id: 'w2', from: { nodeId: 'transform', port: 'result', label: 'result', type: 'output' }, to: { nodeId: 'notify', port: 'body', label: 'body', type: 'input' } },
  ]
}

export function getStore() {
  return {
    get nodes() { return nodes },
    get wires() { return wires },
    get selectedId() { return selectedId },
    get flowName() { return flowName },
    get filename() { return filename },
    set flowName(v: string) { flowName = v },
    set filename(v: string) { filename = v },
    addNode,
    removeNode,
    selectNode,
    updateNodePosition,
    updateNodeProp,
    addWire,
    removeWire,
    exportFlow,
    importFlow,
    loadSample,
  }
}
