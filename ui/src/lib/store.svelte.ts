import dagre from 'dagre'
import * as yaml from 'js-yaml'
import { type CanvasNode, type CanvasNote, type Credential, type NodeManifest, type Wire } from './types'

let nodes = $state<CanvasNode[]>([])
let wires = $state<Wire[]>([])
let notes = $state<CanvasNote[]>([])
let selectedIds = $state<string[]>([])
let selectedWireId = $state<string | null>(null)
let selectedNoteId = $state<string | null>(null)
let flowName = $state('untitled')
let filename = $state('')
let running = $state(false)
let stepMode = $state(false)
let stepReady = $state(false)
let currentFlowId = $state('')
let savedFlows = $state<{ name: string; modified: string }[]>([])
let showFlowList = $state(false)
let draggingWire = $state<{ fromNodeId: string; fromPort: string; mouseX: number; mouseY: number } | null>(null)
let reconnectingWire = $state<{ wireId: string; side: 'from' | 'to'; mouseX: number; mouseY: number } | null>(null)
let selectionBox = $state<{ x1: number; y1: number; x2: number; y2: number } | null>(null)
let ws: WebSocket | null = $state(null)
let panX = $state(0)
let panY = $state(0)
let zoom = $state(1)
let skillsMap = $state<Record<string, NodeManifest>>({})
let currentPage = $state<'editor' | 'credentials' | 'history'>('editor')
let credentials = $state<Credential[]>([])
let credentialSpecs = $state<{ id: string; label: string; auth_type: string; manifest: NodeManifest }[]>([])
let oauthMessage = $state('')
let oauthType = $state<'success' | 'error'>('success')
let historyRuns = $state<Record<string, unknown>[]>([])
let historyRunDetail = $state<Record<string, unknown> | null>(null)

type Snapshot = { nodes: CanvasNode[]; wires: Wire[]; notes: CanvasNote[] }
let undoStack = $state<Snapshot[]>([])
let redoStack = $state<Snapshot[]>([])

function snapshot(): Snapshot {
  return {
    nodes: JSON.parse(JSON.stringify(nodes)),
    wires: JSON.parse(JSON.stringify(wires)),
    notes: JSON.parse(JSON.stringify(notes)),
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
  notes = JSON.parse(JSON.stringify(prev.notes))
  selectedIds = []
  selectedWireId = null
  selectedNoteId = null
}

function redo() {
  if (redoStack.length === 0) return
  undoStack = [...undoStack, snapshot()]
  const next = redoStack[redoStack.length - 1]
  redoStack = redoStack.slice(0, -1)
  nodes = JSON.parse(JSON.stringify(next.nodes))
  wires = JSON.parse(JSON.stringify(next.wires))
  notes = JSON.parse(JSON.stringify(next.notes))
  selectedIds = []
  selectedWireId = null
  selectedNoteId = null
}

function setPan(x: number, y: number) {
  panX = x; panY = y
}

function setZoom(z: number) {
  zoom = Math.max(0.1, Math.min(3, z))
}

function selectWire(id: string | null) {
  selectedWireId = id
  selectedIds = []
  selectedNoteId = null
  if (id) {
    for (const n of nodes) n.selected = false
  }
}

function selectNote(id: string | null) {
  selectedNoteId = id
  selectedWireId = null
  selectedIds = []
  for (const n of nodes) n.selected = false
  for (const nt of notes) nt.selected = nt.id === id
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
  notes = (flow.notes || []).map((nt: CanvasNote) => ({ ...nt, selected: false }))
  wires = reconstructWires(nodes)
  selectedIds = []
  selectedWireId = null
  selectedNoteId = null
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
  selectedIds = selectedIds.filter((sid) => sid !== id)
}

function selectNode(id: string | null) {
  selectedIds = id ? [id] : []
  selectedWireId = null
  selectedNoteId = null
  for (const n of nodes) n.selected = n.id === id
}

function toggleNodeSelection(id: string) {
  selectedWireId = null
  selectedNoteId = null
  if (selectedIds.includes(id)) {
    selectedIds = selectedIds.filter((sid) => sid !== id)
  } else {
    selectedIds = [...selectedIds, id]
  }
  for (const n of nodes) n.selected = selectedIds.includes(n.id)
}

function selectAll() {
  selectedIds = nodes.map((n) => n.id)
  selectedWireId = null
  selectedNoteId = null
  for (const n of nodes) n.selected = true
}

function duplicateSelected() {
  if (selectedIds.length === 0) return
  pushUndo()
  const idMap = new Map<string, string>()
  const newNodes: CanvasNode[] = []
  for (const n of nodes) {
    if (selectedIds.includes(n.id)) {
      const newId = `${n.id}-copy`
      idMap.set(n.id, newId)
      newNodes.push({
        ...JSON.parse(JSON.stringify(n)),
        id: newId,
        position: { x: n.position.x + 40, y: n.position.y + 40 },
        selected: false,
      })
    }
  }
  const newWires: Wire[] = []
  for (const w of wires) {
    if (selectedIds.includes(w.from.nodeId) && selectedIds.includes(w.to.nodeId)) {
      newWires.push({
        ...JSON.parse(JSON.stringify(w)),
        id: `${w.id}-copy`,
        from: { ...w.from, nodeId: idMap.get(w.from.nodeId)! },
        to: { ...w.to, nodeId: idMap.get(w.to.nodeId)! },
      })
    }
  }
  nodes = [...nodes, ...newNodes]
  wires = [...wires, ...newWires]
  selectedIds = newNodes.map((n) => n.id)
  for (const n of nodes) n.selected = selectedIds.includes(n.id)
}

function updateNodePosition(id: string, position: { x: number; y: number }) {
  const n = nodes.find((n) => n.id === id)
  if (!n) return
  const dx = position.x - n.position.x
  const dy = position.y - n.position.y
  n.position = position
  if (selectedIds.includes(id)) {
    for (const other of nodes) {
      if (other.id !== id && selectedIds.includes(other.id)) {
        other.position = { x: other.position.x + dx, y: other.position.y + dy }
      }
    }
  }
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

function startReconnectWire(wireId: string, side: 'from' | 'to', mouseX: number, mouseY: number) {
  pushUndo()
  reconnectingWire = { wireId, side, mouseX, mouseY }
}

function updateReconnectWire(mouseX: number, mouseY: number) {
  if (reconnectingWire) reconnectingWire = { ...reconnectingWire, mouseX, mouseY }
}

function endReconnectWire(targetNodeId?: string, targetPort?: string) {
  const rw = reconnectingWire
  if (!rw) return
  const wire = wires.find((w) => w.id === rw.wireId)
  if (!wire) { reconnectingWire = null; return }
  if (targetNodeId && targetPort) {
    const side = rw.side
    removeWire(wire.id)
    if (side === 'from') {
      addWire({
        id: `${targetNodeId}-${targetPort}-${wire.to.nodeId}-${wire.to.port}`,
        from: { nodeId: targetNodeId, port: targetPort, label: targetPort, type: 'output' },
        to: wire.to,
      })
    } else {
      addWire({
        id: `${wire.from.nodeId}-${wire.from.port}-${targetNodeId}-${targetPort}`,
        from: wire.from,
        to: { nodeId: targetNodeId, port: targetPort, label: targetPort, type: 'input' },
      })
    }
  } else {
    removeWire(wire.id)
  }
  reconnectingWire = null
}

function setSelectionBox(box: { x1: number; y1: number; x2: number; y2: number } | null) {
  selectionBox = box
}

function applySelectionBox() {
  if (!selectionBox) return
  const x1 = Math.min(selectionBox.x1, selectionBox.x2)
  const y1 = Math.min(selectionBox.y1, selectionBox.y2)
  const x2 = Math.max(selectionBox.x1, selectionBox.x2)
  const y2 = Math.max(selectionBox.y1, selectionBox.y2)
  selectedIds = nodes
    .filter((n) => n.position.x >= x1 && n.position.x + 160 <= x2 && n.position.y >= y1 && n.position.y + 100 <= y2)
    .map((n) => n.id)
  selectedWireId = null
  selectedNoteId = null
  for (const n of nodes) n.selected = selectedIds.includes(n.id)
  selectionBox = null
}

function addNote(position: { x: number; y: number }) {
  pushUndo()
  const id = `note-${notes.length + 1}`
  notes.push({
    id,
    text: 'Type here...',
    position,
    width: 200,
    height: 120,
    color: '#fff3cd',
    selected: false,
  })
  selectNote(id)
}

function removeNote(id: string) {
  pushUndo()
  notes = notes.filter((n) => n.id !== id)
  if (selectedNoteId === id) selectedNoteId = null
}

function updateNote(id: string, props: Partial<CanvasNote>) {
  const note = notes.find((n) => n.id === id)
  if (note) Object.assign(note, props)
}

function reconstructWires(nodesList: CanvasNode[]): Wire[] {
  const result: Wire[] = []
  for (const node of nodesList) {
    if (node.inputs) {
      for (const [portName, ref] of Object.entries(node.inputs)) {
        const dot = ref.lastIndexOf('.')
        if (dot !== -1) {
          const fromNodeId = ref.slice(0, dot)
          const fromPort = ref.slice(dot + 1)
          result.push({
            id: `${fromNodeId}-${fromPort}-${node.id}-${portName}`,
            from: { nodeId: fromNodeId, port: fromPort, label: fromPort, type: 'output' },
            to: { nodeId: node.id, port: portName, label: portName, type: 'input' },
          })
        }
      }
    }
  }
  return result
}

function exportFlow(): string {
  const flow: Record<string, unknown> = {
    version: 1,
    name: flowName,
    nodes: nodes.map(({ id, use, with: w, inputs, when, on_error, exit, position }) => ({
      id,
      use,
      ...(w && Object.keys(w).length ? { with: w } : {}),
      ...(inputs && Object.keys(inputs).length ? { inputs } : {}),
      ...(when ? { when } : {}),
      ...(on_error ? { on_error } : {}),
      ...(exit ? { exit } : {}),
      ...(position ? { position } : {}),
    })),
  }
  if (notes.length > 0) {
    flow.notes = notes.map(({ id, text, position, width, height, color }) => ({
      id, text, position, width, height, color,
    }))
  }
  return JSON.stringify(flow, null, 2)
}

function importFlowText(text: string) {
  try {
    pushUndo()
    const data = JSON.parse(text)
    flowName = data.name || 'untitled'
    nodes = (data.nodes || []).map((n: CanvasNode, i: number) => ({
      ...n,
      position: n.position || { x: 100 + i * 40, y: 100 + i * 80 },
      with: n.with || {},
      inputs: n.inputs || {},
    }))
    notes = (data.notes || []).map((nt: CanvasNote) => ({ ...nt, selected: false }))
    wires = reconstructWires(nodes)
    selectedIds = []
    selectedWireId = null
    selectedNoteId = null
  } catch {
    alert('Invalid flow JSON')
  }
}

function exportYaml(): string {
  const flow: Record<string, unknown> = {
    version: 1,
    name: flowName,
    nodes: nodes.map(({ id, use, with: w, inputs, when, on_error, exit, position }) => ({
      id,
      use,
      ...(w && Object.keys(w).length ? { with: w } : {}),
      ...(inputs && Object.keys(inputs).length ? { inputs } : {}),
      ...(when ? { when } : {}),
      ...(on_error ? { on_error } : {}),
      ...(exit ? { exit } : {}),
      position: { x: Math.round(position.x), y: Math.round(position.y) },
    })),
  }
  if (notes.length > 0) {
    flow.notes = notes.map(({ id, text, position, width, height, color }) => ({
      id, text, position, width, height, color,
    }))
  }
  return yaml.dump(flow, { indent: 2, lineWidth: 120, noRefs: true })
}

function importYaml(text: string) {
  try {
    pushUndo()
    const data = yaml.load(text) as Record<string, unknown>
    flowName = (data.name as string) || 'untitled'
    const rawNodes = (data.nodes as Array<Record<string, unknown>>) || []
    nodes = rawNodes.map((n: Record<string, unknown>, i: number) => {
      const pos = n.position as { x: number; y: number } | undefined
      return {
        id: n.id as string,
        use: n.use as string,
        with: (n.with as Record<string, unknown>) || {},
        inputs: (n.inputs as Record<string, string>) || {},
        when: n.when as string | undefined,
        on_error: n.on_error as string | undefined,
        exit: n.exit as boolean | undefined,
        position: pos || { x: 100 + i * 40, y: 100 + i * 80 },
        selected: false,
      } as CanvasNode
    })
    const rawNotes = (data.notes as Array<Record<string, unknown>>) || []
    notes = rawNotes.map((nt: Record<string, unknown>) => ({
      id: nt.id as string,
      text: nt.text as string || '',
      position: nt.position as { x: number; y: number },
      width: (nt.width as number) || 200,
      height: (nt.height as number) || 120,
      color: (nt.color as string) || '#fff3cd',
      selected: false,
    })) as CanvasNote[]
    wires = []
    selectedIds = []
    selectedWireId = null
    selectedNoteId = null
  } catch {
    alert('Invalid flow YAML')
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
  notes = []
  selectedIds = []
  selectedWireId = null
  selectedNoteId = null
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

async function fetchSkills() {
  try {
    const res = await fetch('/api/skills')
    if (!res.ok) return
    const data: NodeManifest[] = await res.json()
    const map: Record<string, NodeManifest> = {}
    const specs: { id: string; label: string; auth_type: string; manifest: NodeManifest }[] = []
    for (const entry of data) {
      map[entry.name] = entry
      const cs = entry.credentials || []
      for (const c of cs) {
        specs.push({ id: c.id, label: c.label, auth_type: c.auth_type, manifest: entry })
      }
    }
    skillsMap = map
    credentialSpecs = specs
  } catch { /* ignore */ }
}

async function fetchHistory() {
  try {
    const res = await fetch('/api/history')
    if (!res.ok) return
    const data = await res.json()
    historyRuns = data.runs || []
  } catch { /* ignore */ }
}

async function fetchHistoryRun(flowId: string) {
  try {
    const res = await fetch(`/api/history/${encodeURIComponent(flowId)}`)
    if (!res.ok) { historyRunDetail = null; return }
    historyRunDetail = await res.json()
  } catch { historyRunDetail = null }
}

function navigateTo(page: 'editor' | 'credentials' | 'history') {
  currentPage = page
}

async function fetchCredentials() {
  try {
    const res = await fetch('/api/credentials')
    if (!res.ok) return
    const data = await res.json()
    credentials = data.credentials || []
  } catch { /* ignore */ }
}

async function createCredential(data: Record<string, unknown>): Promise<boolean> {
  try {
    const res = await fetch('/api/credentials', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data),
    })
    if (res.ok) {
      await fetchCredentials()
      return true
    }
  } catch { /* ignore */ }
  return false
}

async function deleteCredential(id: string): Promise<boolean> {
  try {
    const res = await fetch(`/api/credentials/${encodeURIComponent(id)}`, { method: 'DELETE' })
    if (res.ok) {
      await fetchCredentials()
      return true
    }
  } catch { /* ignore */ }
  return false
}

async function testCredential(id: string): Promise<{ ok: boolean; message: string } | null> {
  try {
    const res = await fetch(`/api/credentials/${encodeURIComponent(id)}/test`, { method: 'POST' })
    if (!res.ok) return null
    return await res.json()
  } catch { return null }
}

async function getCredential(id: string): Promise<Credential | null> {
  try {
    const res = await fetch(`/api/credentials/${encodeURIComponent(id)}`)
    if (!res.ok) return null
    return await res.json()
  } catch { return null }
}

export function getStore() {
  return {
    get nodes() { return nodes },
    get wires() { return wires },
    get notes() { return notes },
    get selectedIds() { return selectedIds },
    get selectedWireId() { return selectedWireId },
    get selectedNoteId() { return selectedNoteId },
    get flowName() { return flowName },
    get filename() { return filename },
    get running() { return running },
    get stepMode() { return stepMode },
    get stepReady() { return stepReady },
    get savedFlows() { return savedFlows },
    get showFlowList() { return showFlowList },
    get draggingWire() { return draggingWire },
    get reconnectingWire() { return reconnectingWire },
    get selectionBox() { return selectionBox },
    get panX() { return panX },
    get panY() { return panY },
    get zoom() { return zoom },
    get skillsMap() { return skillsMap },
    get currentPage() { return currentPage },
    get credentials() { return credentials },
    get credentialSpecs() { return credentialSpecs },
    get historyRuns() { return historyRuns },
    get historyRunDetail() { return historyRunDetail },
    get oauthMessage() { return oauthMessage },
    get oauthType() { return oauthType },
    set oauthMessage(v: string) { oauthMessage = v },
    set oauthType(v: 'success' | 'error') { oauthType = v },
    set flowName(v: string) { flowName = v },
    set filename(v: string) { filename = v },
    set showFlowList(v: boolean) { showFlowList = v },
    addNode,
    removeNode,
    selectNode,
    toggleNodeSelection,
    selectAll,
    duplicateSelected,
    selectWire,
    selectNote,
    updateNodePosition,
    updateNodeProp,
    addWire,
    removeWire,
    startDragWire,
    updateDragWire,
    endDragWire,
    startReconnectWire,
    updateReconnectWire,
    endReconnectWire,
    setSelectionBox,
    applySelectionBox,
    addNote,
    removeNote,
    updateNote,
    setPan,
    setZoom,
    undo,
    redo,
    pushUndo,
    autoLayout,
    exportFlow,
    exportYaml,
    importFlow: importFlowText,
    importYaml,
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
    fetchSkills,
    fetchCredentials,
    createCredential,
    deleteCredential,
    testCredential,
    getCredential,
    fetchHistory,
    fetchHistoryRun,
    navigateTo,
  }
}
