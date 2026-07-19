import { type CanvasNode, type Wire } from './types'

let nodes = $state<CanvasNode[]>([])
let wires = $state<Wire[]>([])
let selectedId = $state<string | null>(null)
let flowName = $state('untitled')
let filename = $state('')
let running = $state(false)
let stepMode = $state(false)
let stepReady = $state(false)
let currentFlowId = $state('')
let ws: WebSocket | null = $state(null)

function connectWs() {
  if (ws?.readyState === WebSocket.OPEN) return
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:'
  ws = new WebSocket(`${proto}//${location.host}/ws`)
  ws.onmessage = (event) => {
    try {
      const msg = JSON.parse(event.data)
      if (msg.type === 'node_input_ready') {
        const n = nodes.find((n) => n.id === msg.node_id)
        if (n) {
          n.status = 'running'
          n.input = msg.output
        }
      } else if (msg.type === 'node_started') {
        const n = nodes.find((n) => n.id === msg.node_id)
        if (n) n.status = 'running'
      } else if (msg.type === 'node_completed') {
        const n = nodes.find((n) => n.id === msg.node_id)
        if (n) {
          n.status = 'done'
          n.output = msg.output
        }
      } else if (msg.type === 'node_failed') {
        const n = nodes.find((n) => n.id === msg.node_id)
        if (n) {
          n.status = 'failed'
          n.error = msg.error
        }
      } else if (msg.type === 'node_skipped') {
        const n = nodes.find((n) => n.id === msg.node_id)
        if (n) n.status = 'done'
      } else if (msg.type === 'flow_started') {
        for (const n of nodes) {
          n.status = 'pending'
          delete n.input
          delete n.output
          delete n.error
        }
      } else if (msg.type === 'step_ready') {
        stepReady = true
      } else if (msg.type === 'flow_completed' || msg.type === 'flow_failed') {
        running = false
        stepReady = false
        stepMode = false
      }
    } catch { /* ignore malformed */ }
  }
  ws.onclose = () => { ws = null }
}

async function runFlow() {
  const flowJson = exportFlow()
  const flow = JSON.parse(flowJson)
  running = true
  stepMode = false
  stepReady = false
  for (const n of nodes) n.status = 'pending'
  connectWs()
  try {
    const res = await fetch('/api/run', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ flow }),
    })
    const data = await res.json()
    currentFlowId = data.flow_id
  } catch {
    running = false
    for (const n of nodes) delete n.status
  }
}

async function runStepFlow() {
  const flowJson = exportFlow()
  const flow = JSON.parse(flowJson)
  running = true
  stepMode = true
  stepReady = false
  for (const n of nodes) n.status = 'pending'
  connectWs()
  try {
    const res = await fetch('/api/run', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ flow, step: true }),
    })
    const data = await res.json()
    currentFlowId = data.flow_id
  } catch {
    running = false
    stepMode = false
    for (const n of nodes) delete n.status
  }
}

function wsSend(action: string) {
  if (!ws || ws.readyState !== WebSocket.OPEN) return
  ws.send(JSON.stringify({ action, flow_id: currentFlowId }))
}

function stepContinue() {
  stepReady = false
  wsSend('continue')
}

function stepStop() {
  stepReady = false
  stepMode = false
  wsSend('stop')
}

function resetStatus() {
  for (const n of nodes) delete n.status
}

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
    get running() { return running },
    get stepMode() { return stepMode },
    get stepReady() { return stepReady },
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
    runFlow,
    runStepFlow,
    stepContinue,
    stepStop,
    resetStatus,
    connectWs,
  }
}
