import { useEffect, useRef, useState } from 'react'
import type { CanvasNode } from '@/lib/types'
import { store } from '@/lib/store'

const PORT_SPACING = 20

interface Props { node: CanvasNode }

export default function NodeBlock({ node }: Props) {
  const elRef = useRef<HTMLDivElement>(null)
  const [dragVis, setDragVis] = useState(false)
  const dragging = useRef(false)
  const offsetRef = useRef({ x: 0, y: 0 })
  const nodeIdRef = useRef(node.id)
  nodeIdRef.current = node.id

  const manifest = store.skillsMap[node.use]
  const inputPorts = manifest ? Object.keys(manifest.inputs) : Object.keys(node.inputs ?? {})
  const outputPorts = manifest && Object.keys(manifest.outputs).length > 0 ? Object.keys(manifest.outputs) : ['output']

  function isConnected(port: string): boolean { return !!(node.inputs as Record<string, string>)?.[port] }

  useEffect(() => {
    const el = elRef.current
    function onMouseMove(e: MouseEvent) {
      if (!dragging.current || !el) return
      const parent = el.parentElement
      if (!parent) return
      const pr = parent.getBoundingClientRect()
      const x = e.clientX - pr.left - offsetRef.current.x
      const y = e.clientY - pr.top - offsetRef.current.y
      el.style.left = `${x}px`
      el.style.top = `${y}px`
      store.updateNodePosition(nodeIdRef.current, { x, y })
    }
    function onMouseUp() {
      if (dragging.current) store.pushUndo()
      dragging.current = false
      setDragVis(false)
    }
    window.addEventListener('mousemove', onMouseMove)
    window.addEventListener('mouseup', onMouseUp)
    return () => { window.removeEventListener('mousemove', onMouseMove); window.removeEventListener('mouseup', onMouseUp) }
  }, [])

  function handleMouseDown(e: React.MouseEvent) {
    e.stopPropagation()
    if (e.shiftKey) store.toggleNodeSelection(node.id)
    else if (!store.selectedIds.includes(node.id)) store.selectNode(node.id)
    dragging.current = true
    setDragVis(true)
    const rect = elRef.current?.getBoundingClientRect()
    offsetRef.current = { x: e.clientX - (rect?.left ?? 0), y: e.clientY - (rect?.top ?? 0) }
  }

  function handlePortMouseDown(e: React.MouseEvent, port: string) {
    e.stopPropagation()
    const parent = elRef.current?.parentElement
    if (!parent) return
    const pr = parent.getBoundingClientRect()
    store.startDragWire(node.id, port, e.clientX - pr.left, e.clientY - pr.top)
  }

  function handlePortMouseUp(e: React.MouseEvent, port: string) {
    e.stopPropagation()
    store.endDragWire(node.id, port)
  }

  const statusColor = node.status === 'pending' ? 'bg-amber-400'
    : node.status === 'running' ? 'bg-blue-400'
    : node.status === 'done' ? 'bg-green-400'
    : node.status === 'failed' ? 'bg-red-400'
    : ''

  return (
    <div
      ref={elRef}
      data-node-id={node.id}
      className={`absolute min-w-40 bg-card border rounded-lg cursor-move text-xs z-10 select-none ${node.selected ? 'border-primary shadow-[0_0_8px_hsl(var(--primary)/0.4)]' : 'border-border'} ${dragVis ? 'opacity-85 z-100' : ''}`}
      style={{ left: node.position.x, top: node.position.y }}
      onMouseDown={handleMouseDown}
      role="application"
    >
      <div className="px-2 py-1 bg-muted border-b border-border rounded-t-md font-semibold text-primary font-mono flex items-center gap-2">
        <span className="flex-1 truncate">{node.use}</span>
        {node.status && <span className={`w-2 h-2 rounded-full inline-block ${statusColor}`} role="status" />}
      </div>
      <div className="px-2 py-1 text-muted-foreground min-h-[24px]">
        <span className="block text-[10px] text-muted-foreground/70 mb-1">{node.id}</span>
        {inputPorts.map(port => (
          <div key={port} className="flex items-center gap-1 text-[11px] text-muted-foreground/60 relative" style={{ height: PORT_SPACING }}>
            <span
              className={`w-1.5 h-1.5 rounded-full inline-block cursor-crosshair z-20 ${isConnected(port) ? 'bg-primary' : 'bg-muted-foreground/40'}`}
              data-port-input data-node-id={node.id} data-port={port}
              onMouseUp={e => handlePortMouseUp(e, port)} role="button" tabIndex={0}
            />
            {port} {isConnected(port) ? `← ${(node.inputs as Record<string, string>)?.[port] ?? ''}` : '(unconnected)'}
          </div>
        ))}
        {outputPorts.map(port => (
          <div key={port} className="flex items-center justify-end gap-1 text-[11px] text-muted-foreground/60 relative" style={{ height: PORT_SPACING }}>
            <span className="flex-1" />
            <span role="button" className="w-1.5 h-1.5 rounded-full bg-green-400 inline-block cursor-crosshair z-20"
              data-port-output data-node-id={node.id}
              onMouseDown={e => handlePortMouseDown(e, port)} tabIndex={0} />
          </div>
        ))}
      </div>
    </div>
  )
}
