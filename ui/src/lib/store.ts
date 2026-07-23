import { type CanvasNode, type CanvasNote, type Credential, type NodeManifest, type Wire, type HistoryRun, type HistoryNode } from './types'
import { toast } from 'sonner'
import dagre from 'dagre'
import * as yaml from 'js-yaml'

type Snapshot = { nodes: CanvasNode[]; wires: Wire[]; notes: CanvasNote[] }
type Listener = () => void

let nodes: CanvasNode[] = []
let wires: Wire[] = []
let notes: CanvasNote[] = []
let selectedIds: string[] = []
let selectedWireId: string | null = null
let selectedNoteId: string | null = null
let flowName = 'untitled'
let filename = ''
let running = false
let stepMode = false
let stepReady = false
let currentFlowId = ''
let savedFlows: { name: string; modified: string }[] = []
let showFlowList = false
let draggingWire: { fromNodeId: string; fromPort: string; mouseX: number; mouseY: number } | null = null
let reconnectingWire: { wireId: string; side: 'from' | 'to'; mouseX: number; mouseY: number } | null = null
let selectionBox: { x1: number; y1: number; x2: number; y2: number } | null = null
let ws: WebSocket | null = null
let panX = 0
let panY = 0
let zoom = 1
let skillsMap: Record<string, NodeManifest> = {}
let currentPage: 'editor' | 'credentials' | 'history' = 'editor'
let credentialsList: Credential[] = []
let credentialSpecs: { id: string; label: string; auth_type: string; manifest: NodeManifest }[] = []
let historyRuns: HistoryRun[] = []
let historyRunDetail: { flow: HistoryRun; nodes: HistoryNode[] } | null = null
let undoStack: Snapshot[] = []
let redoStack: Snapshot[] = []
let listeners: Set<Listener> = new Set()
let _v = 0

function notify() { _v++; listeners.forEach(fn => fn()) }

function snapshot(): Snapshot {
  return { nodes: JSON.parse(JSON.stringify(nodes)), wires: JSON.parse(JSON.stringify(wires)), notes: JSON.parse(JSON.stringify(notes)) }
}

function pushUndo() {
  undoStack = [...undoStack.slice(-49), snapshot()]
  redoStack = []
}

function connectWs(): Promise<void> {
  if (ws?.readyState === WebSocket.OPEN) return Promise.resolve()
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:'
  ws = new WebSocket(`${proto}//${location.host}/ws`)
  const opened = new Promise<void>(resolve => { ws!.onopen = () => resolve() })
  ws.onmessage = event => {
    try {
      const msg = JSON.parse(event.data)
      if (msg.type === 'node_input_ready') {
        const n = nodes.find(n => n.id === msg.node_id); if (n) { n.status = 'running'; n.input = msg.output }
      } else if (msg.type === 'node_started') {
        const n = nodes.find(n => n.id === msg.node_id); if (n) n.status = 'running'
      } else if (msg.type === 'node_completed') {
        const n = nodes.find(n => n.id === msg.node_id); if (n) { n.status = 'done'; n.output = msg.output }
      } else if (msg.type === 'node_failed') {
        const n = nodes.find(n => n.id === msg.node_id); if (n) { n.status = 'failed'; n.error = msg.error }
      } else if (msg.type === 'node_skipped') {
        const n = nodes.find(n => n.id === msg.node_id); if (n) n.status = 'done'
      } else if (msg.type === 'flow_started') {
        for (const n of nodes) { n.status = 'pending'; delete n.input; delete n.output; delete n.error }
      } else if (msg.type === 'step_ready') { stepReady = true }
      else if (msg.type === 'flow_completed') { running = false; stepReady = false; stepMode = false }
      else if (msg.type === 'flow_failed') { running = false; stepReady = false; stepMode = false; if (msg.error) toast.error(msg.error) }
      notify()
    } catch { }
  }
  ws.onclose = () => { ws = null; notify() }
  return opened
}

function wsSend(action: string) {
  if (!ws || ws.readyState !== WebSocket.OPEN) return
  ws.send(JSON.stringify({ action, flow_id: currentFlowId }))
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
            from: { nodeId: fromNodeId, port: fromPort, label: fromPort, type: 'output' as const },
            to: { nodeId: node.id, port: portName, label: portName, type: 'input' as const },
          })
        }
      }
    }
  }
  return result
}

function exportFlowJSON(): Record<string, unknown> {
  return {
    version: 1,
    name: flowName,
    nodes: nodes.map(({ id, use, with: w, inputs, when, on_error, exit, position }) => ({
      id, use,
      ...(w && Object.keys(w).length ? { with: w } : {}),
      ...(inputs && Object.keys(inputs).length ? { inputs } : {}),
      ...(when ? { when } : {}),
      ...(on_error ? { on_error } : {}),
      ...(exit ? { exit } : {}),
      ...(position ? { position } : {}),
    })),
    ...(notes.length > 0 ? {
      notes: notes.map(({ id, text, position, width, height, color }) => ({ id, text, position, width, height, color }))
    } : {}),
  }
}

export const store = {
  get v() { return _v },
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
  get credentials() { return credentialsList },
  get credentialSpecs() { return credentialSpecs },
  get historyRuns() { return historyRuns },
  get historyRunDetail() { return historyRunDetail },
  get undoStack() { return undoStack },

  set flowName(v: string) { flowName = v; notify() },
  set filename(v: string) { filename = v; notify() },
  set showFlowList(v: boolean) { showFlowList = v; notify() },

  subscribe(fn: Listener) { listeners.add(fn); return () => { listeners.delete(fn) } },

  pushUndo,
  undo() {
    if (undoStack.length === 0) return
    redoStack = [...redoStack, snapshot()]
    const prev = undoStack[undoStack.length - 1]
    undoStack = undoStack.slice(0, -1)
    nodes = JSON.parse(JSON.stringify(prev.nodes))
    wires = JSON.parse(JSON.stringify(prev.wires))
    notes = JSON.parse(JSON.stringify(prev.notes))
    selectedIds = []; selectedWireId = null; selectedNoteId = null
    notify()
  },
  redo() {
    if (redoStack.length === 0) return
    undoStack = [...undoStack, snapshot()]
    const next = redoStack[redoStack.length - 1]
    redoStack = redoStack.slice(0, -1)
    nodes = JSON.parse(JSON.stringify(next.nodes))
    wires = JSON.parse(JSON.stringify(next.wires))
    notes = JSON.parse(JSON.stringify(next.notes))
    selectedIds = []; selectedWireId = null; selectedNoteId = null
    notify()
  },

  setPan(x: number, y: number) { panX = x; panY = y; notify() },
  setZoom(z: number) { zoom = Math.max(0.1, Math.min(3, z)); notify() },

  selectWire(id: string | null) {
    selectedWireId = id; selectedIds = []; selectedNoteId = null
    for (const n of nodes) n.selected = false
    notify()
  },
  selectNote(id: string | null) {
    selectedNoteId = id; selectedWireId = null; selectedIds = []
    for (const n of nodes) n.selected = false
    for (const nt of notes) nt.selected = nt.id === id
    notify()
  },

  addNode(type: string, position: { x: number; y: number }) {
    pushUndo()
    const id = `${type}-${nodes.length + 1}`
    nodes.push({ id, use: type, with: {}, inputs: {}, position, selected: false })
    this.selectNode(id)
  },
  removeNode(id: string) {
    pushUndo()
    nodes = nodes.filter(n => n.id !== id)
    wires = wires.filter(w => w.from.nodeId !== id && w.to.nodeId !== id)
    selectedIds = selectedIds.filter(sid => sid !== id)
    notify()
  },
  selectNode(id: string | null) {
    selectedIds = id ? [id] : []; selectedWireId = null; selectedNoteId = null
    for (const n of nodes) n.selected = n.id === id
    notify()
  },
  toggleNodeSelection(id: string) {
    selectedWireId = null; selectedNoteId = null
    if (selectedIds.includes(id)) selectedIds = selectedIds.filter(sid => sid !== id)
    else selectedIds = [...selectedIds, id]
    for (const n of nodes) n.selected = selectedIds.includes(n.id)
    notify()
  },
  selectAll() {
    selectedIds = nodes.map(n => n.id); selectedWireId = null; selectedNoteId = null
    for (const n of nodes) n.selected = true
    notify()
  },
  duplicateSelected() {
    if (selectedIds.length === 0) return
    pushUndo()
    const idMap = new Map<string, string>()
    const newNodes: CanvasNode[] = []
    for (const n of nodes) {
      if (selectedIds.includes(n.id)) {
        const newId = `${n.id}-copy`
        idMap.set(n.id, newId)
        newNodes.push({ ...JSON.parse(JSON.stringify(n)), id: newId, position: { x: n.position.x + 40, y: n.position.y + 40 }, selected: false })
      }
    }
    const newWires: Wire[] = []
    for (const w of wires) {
      if (selectedIds.includes(w.from.nodeId) && selectedIds.includes(w.to.nodeId)) {
        newWires.push({ ...JSON.parse(JSON.stringify(w)), id: `${w.id}-copy`, from: { ...w.from, nodeId: idMap.get(w.from.nodeId)! }, to: { ...w.to, nodeId: idMap.get(w.to.nodeId)! } })
      }
    }
    nodes = [...nodes, ...newNodes]
    wires = [...wires, ...newWires]
    selectedIds = newNodes.map(n => n.id)
    for (const n of nodes) n.selected = selectedIds.includes(n.id)
    notify()
  },
  updateNodePosition(id: string, position: { x: number; y: number }) {
    const n = nodes.find(n => n.id === id)
    if (!n) return
    const dx = position.x - n.position.x
    const dy = position.y - n.position.y
    n.position = position
    if (selectedIds.includes(id)) {
      for (const other of nodes) {
        if (other.id !== id && selectedIds.includes(other.id)) other.position = { x: other.position.x + dx, y: other.position.y + dy }
      }
    }
    notify()
  },
  updateNodeProp(id: string, key: string, value: unknown) {
    const n = nodes.find(n => n.id === id)
    if (n) { if (!n.with) n.with = {}; (n.with as Record<string, unknown>)[key] = value; notify() }
  },

  addWire(wire: Wire) {
    pushUndo()
    const target = nodes.find(n => n.id === wire.to.nodeId)
    if (target) { if (!target.inputs) target.inputs = {}; (target.inputs as Record<string, string>)[wire.to.port] = `${wire.from.nodeId}.${wire.from.port}` }
    wires = [...wires, wire]
    notify()
  },
  removeWire(id: string) {
    pushUndo()
    const wire = wires.find(w => w.id === id)
    if (wire) {
      const target = nodes.find(n => n.id === wire.to.nodeId)
      if (target && target.inputs) delete (target.inputs as Record<string, string>)[wire.to.port]
    }
    wires = wires.filter(w => w.id !== id)
    if (selectedWireId === id) selectedWireId = null
    notify()
  },
  startDragWire(fromNodeId: string, fromPort: string, mouseX: number, mouseY: number) {
    draggingWire = { fromNodeId, fromPort, mouseX, mouseY }; notify()
  },
  updateDragWire(mouseX: number, mouseY: number) {
    if (draggingWire) draggingWire = { ...draggingWire, mouseX, mouseY }; notify()
  },
  endDragWire(targetNodeId?: string, targetPort?: string) {
    if (!draggingWire) return
    if (targetNodeId && targetPort && targetNodeId !== draggingWire.fromNodeId) {
      const id = `${draggingWire.fromNodeId}-${draggingWire.fromPort}-${targetNodeId}-${targetPort}`
      this.addWire({ id, from: { nodeId: draggingWire.fromNodeId, port: draggingWire.fromPort, label: draggingWire.fromPort, type: 'output' }, to: { nodeId: targetNodeId, port: targetPort, label: targetPort, type: 'input' } })
    }
    draggingWire = null; notify()
  },
  startReconnectWire(wireId: string, side: 'from' | 'to', mouseX: number, mouseY: number) {
    pushUndo(); reconnectingWire = { wireId, side, mouseX, mouseY }; notify()
  },
  updateReconnectWire(mouseX: number, mouseY: number) {
    if (reconnectingWire) reconnectingWire = { ...reconnectingWire, mouseX, mouseY }; notify()
  },
  endReconnectWire(targetNodeId?: string, targetPort?: string) {
    const rw = reconnectingWire
    if (!rw) return
    const wire = wires.find(w => w.id === rw.wireId)
    if (!wire) { reconnectingWire = null; notify(); return }
    if (targetNodeId && targetPort) {
      this.removeWire(wire.id)
      if (rw.side === 'from') this.addWire({ id: `${targetNodeId}-${targetPort}-${wire.to.nodeId}-${wire.to.port}`, from: { nodeId: targetNodeId, port: targetPort, label: targetPort, type: 'output' }, to: wire.to })
      else this.addWire({ id: `${wire.from.nodeId}-${wire.from.port}-${targetNodeId}-${targetPort}`, from: wire.from, to: { nodeId: targetNodeId, port: targetPort, label: targetPort, type: 'input' } })
    } else this.removeWire(wire.id)
    reconnectingWire = null; notify()
  },

  setSelectionBox(box: { x1: number; y1: number; x2: number; y2: number } | null) { selectionBox = box; notify() },
  applySelectionBox() {
    if (!selectionBox) return
    const x1 = Math.min(selectionBox.x1, selectionBox.x2)
    const y1 = Math.min(selectionBox.y1, selectionBox.y2)
    const x2 = Math.max(selectionBox.x1, selectionBox.x2)
    const y2 = Math.max(selectionBox.y1, selectionBox.y2)
    selectedIds = nodes.filter(n => n.position.x >= x1 && n.position.x + 160 <= x2 && n.position.y >= y1 && n.position.y + 100 <= y2).map(n => n.id)
    selectedWireId = null; selectedNoteId = null
    for (const n of nodes) n.selected = selectedIds.includes(n.id)
    selectionBox = null; notify()
  },

  addNote(position: { x: number; y: number }) {
    pushUndo()
    const id = `note-${notes.length + 1}`
    notes.push({ id, text: 'Type here...', position, width: 200, height: 120, color: '#fff3cd', selected: false })
    this.selectNote(id)
  },
  removeNote(id: string) {
    pushUndo(); notes = notes.filter(n => n.id !== id); if (selectedNoteId === id) selectedNoteId = null; notify()
  },
  updateNote(id: string, props: Partial<CanvasNote>) {
    const note = notes.find(n => n.id === id)
    if (note) Object.assign(note, props); notify()
  },

  exportFlow: (): string => JSON.stringify(exportFlowJSON(), null, 2),
  exportYaml: (): string => yaml.dump(exportFlowJSON(), { indent: 2, lineWidth: 120, noRefs: true }),

  importFlow(text: string) {
    try {
      pushUndo()
      const data = JSON.parse(text)
      flowName = data.name || 'untitled'
      nodes = (data.nodes || []).map((n: CanvasNode, i: number) => ({ ...n, position: n.position || { x: 100 + i * 40, y: 100 + i * 80 }, with: n.with || {}, inputs: n.inputs || {} }))
      notes = (data.notes || []).map((nt: CanvasNote) => ({ ...nt, selected: false }))
      wires = reconstructWires(nodes)
      selectedIds = []; selectedWireId = null; selectedNoteId = null
      notify()
    } catch { alert('Invalid flow JSON') }
  },
  importYaml(text: string) {
    try {
      pushUndo()
      const data = yaml.load(text) as Record<string, unknown>
      flowName = (data.name as string) || 'untitled'
      const rawNodes = (data.nodes as Array<Record<string, unknown>>) || []
      nodes = rawNodes.map((n, i) => {
        const pos = n.position as { x: number; y: number } | undefined
        return { id: n.id as string, use: n.use as string, with: (n.with as Record<string, unknown>) || {}, inputs: (n.inputs as Record<string, string>) || {}, when: n.when as string | undefined, on_error: n.on_error as string | undefined, exit: n.exit as boolean | undefined, position: pos || { x: 100 + i * 40, y: 100 + i * 80 }, selected: false } as CanvasNode
      })
      notes = ((data.notes as Array<Record<string, unknown>>) || []).map(nt => ({ id: nt.id as string, text: nt.text as string || '', position: nt.position as { x: number; y: number }, width: (nt.width as number) || 200, height: (nt.height as number) || 120, color: (nt.color as string) || '#fff3cd', selected: false })) as CanvasNote[]
      wires = []; selectedIds = []; selectedWireId = null; selectedNoteId = null
      notify()
    } catch { alert('Invalid flow YAML') }
  },
  loadSample() {
    pushUndo()
    flowName = 'etl-demo'
    nodes = [
      { id: 'src', use: 'db-postgres', with: { connection: 'vault://db/prod', query: 'SELECT * FROM orders' }, inputs: {}, position: { x: 60, y: 80 }, selected: false },
      { id: 'transform', use: 'jsonpath', with: { filter: '[] | {id, amount}' }, inputs: { data: 'src.rows' }, position: { x: 420, y: 80 }, selected: false },
      { id: 'notify', use: 'email', with: { to: 'ops@example.com', subject: 'ETL done' }, inputs: { body: 'transform.result' }, when: '{{ transform.result | length > 0 }}', position: { x: 780, y: 80 }, selected: false },
    ]
    wires = [
      { id: 'w1', from: { nodeId: 'src', port: 'rows', label: 'rows', type: 'output' }, to: { nodeId: 'transform', port: 'data', label: 'data', type: 'input' } },
      { id: 'w2', from: { nodeId: 'transform', port: 'result', label: 'result', type: 'output' }, to: { nodeId: 'notify', port: 'body', label: 'body', type: 'input' } },
    ]
    notes = []; selectedIds = []; selectedWireId = null; selectedNoteId = null
    notify()
  },

  autoLayout() {
    const g = new dagre.graphlib.Graph()
    g.setDefaultEdgeLabel(() => ({}))
    g.setGraph({ rankdir: 'LR', nodesep: 60, ranksep: 120, marginx: 40, marginy: 40 })
    for (const n of nodes) g.setNode(n.id, { width: 160, height: 100 })
    for (const w of wires) g.setEdge(w.from.nodeId, w.to.nodeId)
    dagre.layout(g)
    pushUndo()
    for (const n of nodes) { const dag = g.node(n.id); if (dag) n.position = { x: dag.x - 80, y: dag.y - 50 } }
    notify()
  },

  async fetchSkills() {
    try {
      const res = await fetch('/api/skills')
      if (!res.ok) return
      const data: NodeManifest[] = await res.json()
      const map: Record<string, NodeManifest> = {}
      const specs: { id: string; label: string; auth_type: string; manifest: NodeManifest }[] = []
      for (const entry of data) {
        map[entry.name] = entry
        for (const c of entry.credentials || []) specs.push({ id: c.id, label: c.label, auth_type: c.auth_type, manifest: entry })
      }
      skillsMap = map; credentialSpecs = specs; notify()
    } catch { }
  },
  async fetchCredentials() {
    try { const res = await fetch('/api/credentials'); if (!res.ok) return; const data = await res.json(); credentialsList = data.credentials || []; notify() } catch { }
  },
  async createCredential(data: Record<string, unknown>): Promise<boolean> {
    try { const res = await fetch('/api/credentials', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data) }); if (res.ok) { await this.fetchCredentials(); return true } } catch { }
    return false
  },
  async deleteCredential(id: string): Promise<boolean> {
    try { const res = await fetch(`/api/credentials/${encodeURIComponent(id)}`, { method: 'DELETE' }); if (res.ok) { await this.fetchCredentials(); return true } } catch { }
    return false
  },
  async testCredential(id: string): Promise<{ ok: boolean; message: string } | null> {
    try { const res = await fetch(`/api/credentials/${encodeURIComponent(id)}/test`, { method: 'POST' }); if (!res.ok) return null; return await res.json() } catch { return null }
  },

  async fetchHistory() {
    try { const res = await fetch('/api/history'); if (!res.ok) return; const data = await res.json(); historyRuns = data.runs || []; notify() } catch { }
  },
  async fetchHistoryRun(flowId: string) {
    try { const res = await fetch(`/api/history/${encodeURIComponent(flowId)}`); if (!res.ok) { historyRunDetail = null; return }; historyRunDetail = await res.json(); notify() } catch { historyRunDetail = null; notify() }
  },

  navigateTo(page: 'editor' | 'credentials' | 'history') { currentPage = page; notify() },

  async runFlow() {
    running = true; stepMode = false; stepReady = false
    for (const n of nodes) n.status = 'pending'
    notify()
    await connectWs()
    try {
      const res = await fetch('/api/run', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ flow: exportFlowJSON() }) })
      const data = await res.json(); currentFlowId = data.flow_id
    } catch { running = false; for (const n of nodes) delete n.status; notify() }
  },
  async runStepFlow() {
    running = true; stepMode = true; stepReady = false
    for (const n of nodes) n.status = 'pending'
    notify()
    await connectWs()
    try {
      const res = await fetch('/api/run', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ flow: exportFlowJSON(), step: true }) })
      const data = await res.json(); currentFlowId = data.flow_id
    } catch { running = false; stepMode = false; for (const n of nodes) delete n.status; notify() }
  },
  stepContinue() { stepReady = false; notify(); wsSend('continue') },
  stepStop() { stepReady = false; stepMode = false; notify(); wsSend('stop') },
  resetStatus() { for (const n of nodes) delete n.status; notify() },

  async saveFlow() {
    const res = await fetch('/api/flows', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(exportFlowJSON()) })
    if (res.ok) { const data = await res.json(); flowName = data.name; await this.listFlows(); notify() }
  },
  async loadFlow(name: string) {
    const res = await fetch(`/api/flows/${encodeURIComponent(name)}`)
    if (!res.ok) return
    const flow = await res.json()
    flowName = flow.name || name
    nodes = (flow.nodes || []).map((n: CanvasNode, i: number) => ({ ...n, position: n.position || { x: 100 + i * 40, y: 100 + i * 80 }, with: n.with || {}, inputs: n.inputs || {} }))
    notes = (flow.notes || []).map((nt: CanvasNote) => ({ ...nt, selected: false }))
    wires = reconstructWires(nodes)
    selectedIds = []; selectedWireId = null; selectedNoteId = null; showFlowList = false
    notify()
  },
  async deleteFlow(name: string) {
    const res = await fetch(`/api/flows/${encodeURIComponent(name)}`, { method: 'DELETE' })
    if (res.ok) await this.listFlows()
  },
  async listFlows() {
    const res = await fetch('/api/flows')
    if (!res.ok) return; const data = await res.json(); savedFlows = data.flows || []; showFlowList = true; notify()
  },
}
