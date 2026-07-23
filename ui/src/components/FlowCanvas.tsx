import { useCallback, useEffect, useRef, useState } from 'react'
import { store } from '@/lib/store'
import { useStore } from '@/lib/useStore'
import NodeBlock from './NodeBlock'
import NoteBlock from './NoteBlock'

const NODE_W = 160
const HEADER_H = 24
const PORT_SPACING = 20

function screenToCanvas(sx: number, sy: number, panX: number, panY: number, zoom: number, rect: DOMRect) {
  return { x: (sx - rect.left - panX) / zoom, y: (sy - rect.top - panY) / zoom }
}

function nodePortPos(nodeId: string, port: string, side: 'input' | 'output') {
  const n = store.nodes.find(n => n.id === nodeId)
  if (!n) return null
  const manifest = store.skillsMap[n.use]
  const keys = side === 'input'
    ? Object.keys(n.inputs ?? {})
    : (manifest && Object.keys(manifest.outputs).length ? Object.keys(manifest.outputs) : ['output'])
  const idx = keys.indexOf(port)
  if (idx < 0) return null
  return {
    x: side === 'input' ? n.position.x : n.position.x + NODE_W,
    y: n.position.y + HEADER_H + 4 + idx * PORT_SPACING + PORT_SPACING / 2,
  }
}

function wirePath(x1: number, y1: number, x2: number, y2: number) {
  const dx = Math.abs(x2 - x1) * 0.5
  return `M ${x1} ${y1} C ${x1 + dx} ${y1}, ${x2 - dx} ${y2}, ${x2} ${y2}`
}

export default function FlowCanvas() {
  const canvasRef = useRef<HTMLDivElement>(null)
  const svgRef = useRef<SVGSVGElement>(null)
  const dragPathRef = useRef<SVGPathElement>(null)
  const reconnectPathRef = useRef<SVGPathElement>(null)
  const [panning, setPanning] = useState(false)
  const panStart = useRef({ x: 0, y: 0 })
  const boxStart = useRef<{ x: number; y: number } | null>(null)

  const panX = useStore(s => s.panX)
  const panY = useStore(s => s.panY)
  const zoom = useStore(s => s.zoom)
  const nodes = useStore(s => s.nodes)
  const wires = useStore(s => s.wires)
  const notes = useStore(s => s.notes)
  const selectedWireId = useStore(s => s.selectedWireId)
  const draggingWire = useStore(s => s.draggingWire)
  const reconnectingWire = useStore(s => s.reconnectingWire)
  const selectionBox = useStore(s => s.selectionBox)

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (e.key === 'Delete' || e.key === 'Backspace') {
      if (store.selectedWireId) { store.removeWire(store.selectedWireId); e.preventDefault() }
      else if (store.selectedNoteId) { store.removeNote(store.selectedNoteId); e.preventDefault() }
      else if (store.selectedIds.length > 0) { const ids = [...store.selectedIds]; store.selectNode(null); ids.forEach(id => store.removeNode(id)); e.preventDefault() }
    }
    if (e.key === 'Escape') { store.selectNode(null); store.selectWire(null); store.selectNote(null) }
    if ((e.ctrlKey || e.metaKey) && e.key === 's') { e.preventDefault(); store.saveFlow() }
    if ((e.ctrlKey || e.metaKey) && e.key === 'z' && !e.shiftKey) { e.preventDefault(); store.undo() }
    if ((e.ctrlKey || e.metaKey) && e.key === 'z' && e.shiftKey) { e.preventDefault(); store.redo() }
    if ((e.ctrlKey || e.metaKey) && e.key === 'y') { e.preventDefault(); store.redo() }
    if ((e.ctrlKey || e.metaKey) && e.key === 'a') { e.preventDefault(); store.selectAll() }
    if ((e.ctrlKey || e.metaKey) && e.key === 'd') { e.preventDefault(); store.duplicateSelected() }
  }, [])

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [handleKeyDown])

  useEffect(() => { updateDragPath() }, [draggingWire])
  useEffect(() => { updateReconnectPath() }, [reconnectingWire])

  function getRect() { return canvasRef.current?.getBoundingClientRect() ?? new DOMRect() }

  function updateDragPath() {
    const dw = store.draggingWire
    if (!dw) return
    const from = nodePortPos(dw.fromNodeId, dw.fromPort, 'output')
    const x1 = from?.x ?? dw.mouseX
    const y1 = from?.y ?? dw.mouseY
    if (dragPathRef.current) dragPathRef.current.setAttribute('d', wirePath(x1, y1, dw.mouseX, dw.mouseY))
  }

  function updateReconnectPath() {
    const rw = store.reconnectingWire
    if (!rw) return
    const wire = store.wires.find(w => w.id === rw.wireId)
    if (!wire) return
    const fixedSide = rw.side === 'from' ? 'to' : 'from'
    const fixedPort = rw.side === 'from' ? wire.to : wire.from
    const fixed = nodePortPos(fixedPort.nodeId, fixedPort.port, fixedSide === 'to' ? 'input' : 'output')
    if (!fixed) return
    if (reconnectPathRef.current) reconnectPathRef.current.setAttribute('d', wirePath(fixed.x, fixed.y, rw.mouseX, rw.mouseY))
  }

  function handleMouseMove(e: React.MouseEvent) {
    const rect = getRect()
    const p = screenToCanvas(e.clientX, e.clientY, panX, panY, zoom, rect)
    if (store.draggingWire) { store.updateDragWire(p.x, p.y); updateDragPath(); return }
    if (store.reconnectingWire) { store.updateReconnectWire(p.x, p.y); updateReconnectPath(); return }
    if (boxStart.current) { store.setSelectionBox({ x1: boxStart.current.x, y1: boxStart.current.y, x2: p.x, y2: p.y }); return }
    if (panning) { store.setPan(panX + e.clientX - panStart.current.x, panY + e.clientY - panStart.current.y); panStart.current = { x: e.clientX, y: e.clientY } }
  }

  function handleMouseDown(e: React.MouseEvent) {
    if (e.button === 1 || (e.button === 0 && e.ctrlKey)) { e.preventDefault(); setPanning(true); panStart.current = { x: e.clientX, y: e.clientY }; return }
    if (e.button === 0 && !e.shiftKey) {
      const target = e.target as HTMLElement
      if (!target.closest('[data-port-input]') && !target.closest('[data-port-output]') && !target.closest('[data-node-id]') && !target.closest('[data-wire]') && !target.closest('[data-note]') && !target.closest('[data-wire-endpoint]')) {
        const pos = screenToCanvas(e.clientX, e.clientY, panX, panY, zoom, getRect())
        boxStart.current = pos
        store.setSelectionBox({ x1: pos.x, y1: pos.y, x2: pos.x, y2: pos.y })
      }
    }
  }

  function handleMouseUp(e: React.MouseEvent) {
    if (store.draggingWire) {
      const target = (e.target as HTMLElement).closest('[data-port-input]')
      if (target) { const nid = target.getAttribute('data-node-id'); const port = target.getAttribute('data-port'); if (nid && port) { store.endDragWire(nid, port); return } }
      store.endDragWire(); return
    }
    if (store.reconnectingWire) {
      const target = (e.target as HTMLElement).closest('[data-port-input], [data-port-output]')
      if (target) { const nid = target.getAttribute('data-node-id'); const port = target.getAttribute('data-port'); if (nid && port) { store.endReconnectWire(nid, port); return } }
      store.endReconnectWire(); return
    }
    if (boxStart.current) { store.applySelectionBox(); boxStart.current = null; return }
    if (panning) { setPanning(false); return }
  }

  function handleClick(e: React.MouseEvent) {
    if (store.draggingWire || store.reconnectingWire || panning || boxStart.current) return
    const target = e.target as HTMLElement
    if (target.closest('[data-port-input]') || target.closest('[data-port-output]') || target.closest('[data-node-id]')) return
    if (target.closest('[data-wire]')) return
    if (target.closest('[data-note]')) return
    store.selectNode(null); store.selectWire(null); store.selectNote(null)
  }

  function handleWheel(e: React.WheelEvent) {
    e.preventDefault()
    const rect = getRect()
    const sx = e.clientX - rect.left; const sy = e.clientY - rect.top
    const ccx = (sx - panX) / zoom; const ccy = (sy - panY) / zoom
    const delta = e.deltaY > 0 ? 0.9 : 1.1
    const newZoom = Math.max(0.1, Math.min(3, zoom * delta))
    store.setPan(sx - ccx * newZoom, sy - ccy * newZoom)
    store.setZoom(newZoom)
  }

  const wirePaths = wires.map(wire => {
    const from = nodePortPos(wire.from.nodeId, wire.from.port, 'output')
    const to = nodePortPos(wire.to.nodeId, wire.to.port, 'input')
    if (!from || !to) return null
    return { id: wire.id, path: wirePath(from.x, from.y, to.x, to.y) }
  }).filter(Boolean) as { id: string; path: string }[]

  return (
    <div
      ref={canvasRef}
      className="flex-1 relative bg-background overflow-hidden"
      style={{ backgroundImage: `url("data:image/svg+xml,%3Csvg width='20' height='20' xmlns='http://www.w3.org/2000/svg'%3E%3Ccircle cx='2' cy='2' r='1' fill='%23a1a1aa' fill-opacity='0.3' /%3E%3C/svg%3E")` }}
      onClick={handleClick}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onWheel={handleWheel}
      tabIndex={0}
      role="application"
      aria-label="Flow canvas"
    >
      <div
        className="absolute inset-0"
        style={{ transform: `translate(${panX}px, ${panY}px) scale(${zoom})`, transformOrigin: '0 0' }}
      >
        <svg ref={svgRef} className="absolute inset-0 w-full h-full pointer-events-none">
          <defs>
            <filter id="wire-glow">
              <feDropShadow dx={0} dy={0} stdDeviation={2} floodColor="#7c3aed" floodOpacity={0.5} />
            </filter>
          </defs>
          {wirePaths.map(wp => (
            <path key={`hit-${wp.id}`} d={wp.path} stroke="transparent" strokeWidth={14} fill="none"
              className="pointer-events-auto cursor-pointer"
              onClick={e => { e.stopPropagation(); store.selectWire(wp.id) }} />
          ))}
          {wires.map(wire => {
            const from = nodePortPos(wire.from.nodeId, wire.from.port, 'output')
            const to = nodePortPos(wire.to.nodeId, wire.to.port, 'input')
            if (!from || !to) return null
            return (
              <path key={wire.id} d={wirePath(from.x, from.y, to.x, to.y)}
                stroke={wire.id === selectedWireId ? '#a78bfa' : '#7c3aed'}
                strokeWidth={wire.id === selectedWireId ? 3 : 2} fill="none"
                filter={wire.id === selectedWireId ? 'url(#wire-glow)' : undefined} />
            )
          })}
          {wires.map(wire => {
            if (wire.id !== selectedWireId) return null
            const from = nodePortPos(wire.from.nodeId, wire.from.port, 'output')
            const to = nodePortPos(wire.to.nodeId, wire.to.port, 'input')
            if (!from || !to) return null
            return (
              <g key={`endpoints-${wire.id}`}>
                <circle cx={from.x} cy={from.y} r={6} fill="#a78bfa" stroke="#7c3aed" strokeWidth={2}
                  className="pointer-events-auto cursor-grab" data-wire-endpoint={wire.id} data-wire-side="from"
                  onMouseDown={e => { e.stopPropagation(); store.startReconnectWire(wire.id, 'from', from.x, from.y) }} />
                <circle cx={to.x} cy={to.y} r={6} fill="#a78bfa" stroke="#7c3aed" strokeWidth={2}
                  className="pointer-events-auto cursor-grab" data-wire-endpoint={wire.id} data-wire-side="to"
                  onMouseDown={e => { e.stopPropagation(); store.startReconnectWire(wire.id, 'to', to.x, to.y) }} />
              </g>
            )
          })}
          <path ref={dragPathRef} d="" stroke="#7c3aed" strokeWidth={2} strokeDasharray="5,5" fill="none" opacity={store.draggingWire ? 1 : 0} />
          <path ref={reconnectPathRef} d="" stroke="#f59e0b" strokeWidth={2} strokeDasharray="5,5" fill="none" opacity={store.reconnectingWire ? 1 : 0} />
        </svg>
        {nodes.map(node => <NodeBlock key={node.id} node={node} />)}
        {notes.map(note => <NoteBlock key={note.id} note={note} />)}
        {selectionBox && (
          <div className="absolute border border-primary bg-primary/10 pointer-events-none"
            style={{
              left: Math.min(selectionBox.x1, selectionBox.x2),
              top: Math.min(selectionBox.y1, selectionBox.y2),
              width: Math.abs(selectionBox.x2 - selectionBox.x1),
              height: Math.abs(selectionBox.y2 - selectionBox.y1),
            }} />
        )}
      </div>
    </div>
  )
}
