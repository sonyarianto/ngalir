import dagre from 'dagre'
import { type CanvasNode, type Wire } from './types'

let nodes = $state<CanvasNode[]>([])
let wires = $state<Wire[]>([])
let selectedId = $state<string | null>(null)
let selectedWireId = $state<string | null>(null)
let flowName = $state('untitled')
let filename = $state('')
let running = $state(false)
let stepMode = $state(false)
let stepReady = $state(false)
let currentFlowId = $state('')
let savedFlows = $state<{ name: string; modified: string }[]>([])
let showFlowList = $state(false)
let draggingWire = $state<{ fromNodeId: string; fromPort: string; mouseX: number; mouseY: number } | null>(null)
let ws: WebSocket | null = $state(null)
let panX = $state(0)
let panY = $state(0)
let zoom = $state(1)
type Snapshot = { nodes: CanvasNode[]; wires: Wire[] }
let undoStack = $state<Snapshot[]>([])
let redoStack = $state<Snapshot[]>([])

function snapshot(): Snapshot {
  return {
    nodes: JSON.parse(JSON.stringify(nodes)),
    wires: JSON.parse(JSON.stringify(wires)),
  }
}

function pushUndo() {
  undoStack = [...undoStack.slice(-49), snapshot()]
  redoStack = []
}

function undo() {
  if (undoStack.length === 0) return
  redoStack = [...redoStack, snapshot()]
  const prev = undoStack[undoStack.length - 1]
  undoStack = undoStack.slice(0, -1)
  nodes = JSON.parse(JSON.stringify(prev.nodes))
  wires = JSON.parse(JSON.stringify(prev.wires))
  selectedId = null
  selectedWireId = null
}

function redo() {
  if (redoStack.length === 0) return
  undoStack = [...undoStack, snapshot()]
  const next = redoStack[redoStack.length - 1]
  redoStack = redoStack.slice(0, -1)
  nodes = JSON.parse(JSON.stringify(next.nodes))
  wires = JSON.parse(JSON.stringify(next.wires))
  selectedId = null
  selectedWireId = null
}

function setPan(x: number, y: number) {
  panX = x; panY = y
}

function setZoom(z: number) {
  zoom = Math.max(0.1, Math.min(3, z))
}

function selectWire(id: string | null) {
  selectedWireId = id
  if (id) {
    for (const n of nodes) n.selected = false
    selectedId = null
  }
}

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

async function saveFlow() {
  const flow = JSON.parse(exportFlow())
  const res = await fetch('/api/flows', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(flow),
  })
  if (res.ok) {
    const data = await res.json()
    flowName = data.name
    await listFlows()
  }
}

async function loadFlow(name: string) {
  const res = await fetch(`/api/flows/${encodeURIComponent(name)}`)
  if (!res.ok) return
  const flow = await res.json()
  flowName = flow.name || name
  nodes = (flow.nodes || []).map((n: CanvasNode, i: number) => ({
    ...n,
    position: n.position || { x: 100 + i * 40, y: 100 + i * 80 },
    with: n.with || {},
    inputs: n.inputs || {},
  }))
  wires = []
  showFlowList = false
}

async function deleteFlow(name: string) {
  const res = await fetch(`/api/flows/${encodeURIComponent(name)}`, { method: 'DELETE' })
  if (res.ok) await listFlows()
}

async function listFlows() {
  const res = await fetch('/api/flows')
  if (!res.ok) return
  const data = await res.json()
  savedFlows = data.flows || []
  showFlowList = true
}

function addNode(type: string, position: { x: number; y: number }) {
  pushUndo()
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
  pushUndo()
  nodes = nodes.filter((n) => n.id !== id)
  wires = wires.filter((w) => w.from.nodeId !== id && w.to.nodeId !== id)
  if (selectedId === id) selectedId = null
}

function selectNode(id: string | null) {
  for (const n of nodes) n.selected = n.id === id
  selectedId = id
  if (id) selectedWireId = null
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
  pushUndo()
  const target = nodes.find((n) => n.id === wire.to.nodeId)
  if (target) {
    if (!target.inputs) target.inputs = {}
    ;(target.inputs as Record<string, string>)[wire.to.port] = `${wire.from.nodeId}.${wire.from.port}`
  }
  wires = [...wires, wire]
}

function removeWire(id: string) {
  pushUndo()
  const wire = wires.find((w) => w.id === id)
  if (wire) {
    const target = nodes.find((n) => n.id === wire.to.nodeId)
    if (target && target.inputs) {
      delete (target.inputs as Record<string, string>)[wire.to.port]
    }
  }
  wires = wires.filter((w) => w.id !== id)
  if (selectedWireId === id) selectedWireId = null
}

function startDragWire(fromNodeId: string, fromPort: string, mouseX: number, mouseY: number) {
  draggingWire = { fromNodeId, fromPort, mouseX, mouseY }
}

function updateDragWire(mouseX: number, mouseY: number) {
  if (draggingWire) draggingWire = { ...draggingWire, mouseX, mouseY }
}

function endDragWire(targetNodeId?: string, targetPort?: string) {
  if (!draggingWire) return
  if (targetNodeId && targetPort && targetNodeId !== draggingWire.fromNodeId) {
    const id = `${draggingWire.fromNodeId}-${draggingWire.fromPort}-${targetNodeId}-${targetPort}`
    addWire({
      id,
      from: { nodeId: draggingWire.fromNodeId, port: draggingWire.fromPort, label: draggingWire.fromPort, type: 'output' },
      to: { nodeId: targetNodeId, port: targetPort, label: targetPort, type: 'input' },
    })
  }
  draggingWire = null
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
    pushUndo()
    const flow = JSON.parse(text)
    flowName = flow.name || 'untitled'
    nodes = (flow.nodes || []).map((n: CanvasNode, i: number) => ({
      ...n,
      position: n.position || { x: 100 + i * 40, y: 100 + i * 80 },
      with: n.with || {},
      inputs: n.inputs || {},
    }))
    wires = []
    selectedWireId = null
  } catch {
    alert('Invalid flow JSON')
  }
}

function loadSample() {
  pushUndo()
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
  selectedWireId = null
}

function autoLayout() {
  const g = new dagre.graphlib.Graph()
  g.setDefaultEdgeLabel(() => ({}))
  g.setGraph({ rankdir: 'LR', nodesep: 60, ranksep: 120, marginx: 40, marginy: 40 })
  for (const n of nodes) g.setNode(n.id, { width: 160, height: 100 })
  for (const w of wires) g.setEdge(w.from.nodeId, w.to.nodeId)
  dagre.layout(g)
  pushUndo()
  for (const n of nodes) {
    const dag = g.node(n.id)
    if (dag) n.position = { x: dag.x - 80, y: dag.y - 50 }
  }
}

export function getStore() {
  return {
    get nodes() { return nodes },
    get wires() { return wires },
    get selectedId() { return selectedId },
    get selectedWireId() { return selectedWireId },
    get flowName() { return flowName },
    get filename() { return filename },
    get running() { return running },
    get stepMode() { return stepMode },
    get stepReady() { return stepReady },
    get savedFlows() { return savedFlows },
    get showFlowList() { return showFlowList },
    get draggingWire() { return draggingWire },
    get panX() { return panX },
    get panY() { return panY },
    get zoom() { return zoom },
    set flowName(v: string) { flowName = v },
    set filename(v: string) { filename = v },
    set showFlowList(v: boolean) { showFlowList = v },
    addNode,
    removeNode,
    selectNode,
    selectWire,
    updateNodePosition,
    updateNodeProp,
    addWire,
    removeWire,
    startDragWire,
    updateDragWire,
    endDragWire,
    setPan,
    setZoom,
    undo,
    redo,
    pushUndo,
    autoLayout,
    exportFlow,
    importFlow,
    loadSample,
    runFlow,
    runStepFlow,
    stepContinue,
    stepStop,
    resetStatus,
    connectWs,
    saveFlow,
    loadFlow,
    deleteFlow,
    listFlows,
  }
}
