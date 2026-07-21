<script lang="ts">
  import { getStore } from './store.svelte.js'
  import NodeBlock from './NodeBlock.svelte'
  import NoteBlock from './NoteBlock.svelte'

  const store = getStore()

  const NODE_W = 160
  const HEADER_H = 24
  const PORT_SPACING = 20

  let canvasEl: HTMLElement | undefined = $state()
  let panning = $state(false)
  let panStart = $state({ x: 0, y: 0 })
  let boxStart = $state<{ x: number; y: number } | null>(null)

  function screenToCanvas(sx: number, sy: number): { x: number; y: number } {
    const r = canvasEl?.getBoundingClientRect()
    if (!r) return { x: sx, y: sy }
    return {
      x: (sx - r.left - store.panX) / store.zoom,
      y: (sy - r.top - store.panY) / store.zoom,
    }
  }

  function nodePortPos(nodeId: string, port: string, side: 'input' | 'output'): { x: number; y: number } | null {
    const n = store.nodes.find((n) => n.id === nodeId)
    if (!n) return null
    const manifest = store.skillsMap[n.use]
    const keys = side === 'input' ? Object.keys(n.inputs ?? {}) : (manifest ? Object.keys(manifest.outputs).length ? Object.keys(manifest.outputs) : ['output'] : ['output'])
    const idx = keys.indexOf(port)
    if (idx < 0) return null
    const x = side === 'input' ? n.position.x : n.position.x + NODE_W
    const y = n.position.y + HEADER_H + 4 + idx * PORT_SPACING + PORT_SPACING / 2
    return { x, y }
  }

  function wirePath(x1: number, y1: number, x2: number, y2: number): string {
    const dx = Math.abs(x2 - x1) * 0.5
    const cx1 = x1 + dx
    const cx2 = x2 - dx
    return `M ${x1} ${y1} C ${cx1} ${y1}, ${cx2} ${y2}, ${x2} ${y2}`
  }

  function handleCanvasMouseMove(e: MouseEvent) {
    if (store.draggingWire) {
      const pos = screenToCanvas(e.clientX, e.clientY)
      store.updateDragWire(pos.x, pos.y)
      return
    }
    if (store.reconnectingWire) {
      const pos = screenToCanvas(e.clientX, e.clientY)
      store.updateReconnectWire(pos.x, pos.y)
      return
    }
    if (boxStart) {
      const pos = screenToCanvas(e.clientX, e.clientY)
      store.setSelectionBox({ x1: boxStart.x, y1: boxStart.y, x2: pos.x, y2: pos.y })
      return
    }
    if (panning) {
      store.setPan(store.panX + e.clientX - panStart.x, store.panY + e.clientY - panStart.y)
      panStart = { x: e.clientX, y: e.clientY }
      return
    }
  }

  function handleCanvasMouseDown(e: MouseEvent) {
    if (e.button === 1 || (e.button === 0 && e.ctrlKey)) {
      e.preventDefault()
      panning = true
      panStart = { x: e.clientX, y: e.clientY }
      return
    }
    if (e.button === 0 && !e.shiftKey) {
      const target = e.target as HTMLElement
      if (!target.closest('[data-port-input]') && !target.closest('[data-port-output]') && !target.closest('[data-node-id]') && !target.closest('[data-wire]') && !target.closest('[data-note]') && !target.closest('[data-wire-endpoint]')) {
        const pos = screenToCanvas(e.clientX, e.clientY)
        boxStart = pos
        store.setSelectionBox({ x1: pos.x, y1: pos.y, x2: pos.x, y2: pos.y })
      }
    }
  }

  function handleCanvasMouseUp(e: MouseEvent) {
    if (store.draggingWire) {
      const target = (e.target as HTMLElement).closest('[data-port-input]')
      if (target) {
        const nodeId = target.getAttribute('data-node-id')
        const port = target.getAttribute('data-port')
        if (nodeId && port) {
          store.endDragWire(nodeId, port)
          return
        }
      }
      store.endDragWire()
      return
    }
    if (store.reconnectingWire) {
      const target = (e.target as HTMLElement).closest('[data-port-input], [data-port-output]')
      if (target) {
        const nodeId = target.getAttribute('data-node-id')
        const port = target.getAttribute('data-port')
        if (nodeId && port) {
          store.endReconnectWire(nodeId, port)
          return
        }
      }
      store.endReconnectWire()
      return
    }
    if (boxStart) {
      store.applySelectionBox()
      boxStart = null
      return
    }
    if (panning) {
      panning = false
      return
    }
  }

  function handleCanvasClick(e: MouseEvent) {
    if (store.draggingWire || store.reconnectingWire || panning || boxStart) return
    const target = e.target as HTMLElement
    if (target.closest('[data-port-input]') || target.closest('[data-port-output]') || target.closest('[data-node-id]')) return
    if (target.closest('[data-wire]')) return
    if (target.closest('[data-note]')) return
    store.selectNode(null)
    store.selectWire(null)
    store.selectNote(null)
  }

  function handleCanvasWheel(e: WheelEvent) {
    e.preventDefault()
    const rect = canvasEl?.getBoundingClientRect()
    if (!rect) return
    const sx = e.clientX - rect.left
    const sy = e.clientY - rect.top
    const ccx = (sx - store.panX) / store.zoom
    const ccy = (sy - store.panY) / store.zoom
    const delta = e.deltaY > 0 ? 0.9 : 1.1
    const newZoom = Math.max(0.1, Math.min(3, store.zoom * delta))
    store.setPan(sx - ccx * newZoom, sy - ccy * newZoom)
    store.setZoom(newZoom)
  }

  let wireHitPaths = $derived(
    store.wires.map((wire) => {
      const from = nodePortPos(wire.from.nodeId, wire.from.port, 'output')
      const to = nodePortPos(wire.to.nodeId, wire.to.port, 'input')
      if (!from || !to) return null
      return { id: wire.id, path: wirePath(from.x, from.y, to.x, to.y) }
    }).filter(Boolean) as { id: string; path: string }[]
  )

  let draggingPos = $derived(
    store.draggingWire
      ? (() => {
          const from = nodePortPos(store.draggingWire.fromNodeId, store.draggingWire.fromPort, 'output')
          return {
            x1: from?.x ?? store.draggingWire.mouseX,
            y1: from?.y ?? store.draggingWire.mouseY,
            x2: store.draggingWire.mouseX,
            y2: store.draggingWire.mouseY,
          }
        })()
      : null,
  )

  let reconnectPos = $derived(
    store.reconnectingWire
      ? (() => {
          const wire = store.wires.find((w) => w.id === store.reconnectingWire!.wireId)
          if (!wire) return null
          const fixedSide = store.reconnectingWire!.side === 'from' ? 'to' : 'from'
          const fixedPort = store.reconnectingWire!.side === 'from' ? wire.to : wire.from
          const fixed = nodePortPos(fixedPort.nodeId, fixedPort.port, fixedSide === 'to' ? 'input' : 'output')
          if (!fixed) return null
          return {
            x1: fixed.x, y1: fixed.y,
            x2: store.reconnectingWire!.mouseX, y2: store.reconnectingWire!.mouseY,
          }
        })()
      : null,
  )

  function handleReconnectEndpointMouseDown(e: MouseEvent, wireId: string, side: 'from' | 'to') {
    e.stopPropagation()
    const pos = screenToCanvas(e.clientX, e.clientY)
    store.startReconnectWire(wireId, side, pos.x, pos.y)
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Delete' || e.key === 'Backspace') {
      if (store.selectedWireId) {
        store.removeWire(store.selectedWireId)
        e.preventDefault()
      } else if (store.selectedNoteId) {
        store.removeNote(store.selectedNoteId)
        e.preventDefault()
      } else if (store.selectedIds.length > 0) {
        const ids = [...store.selectedIds]
        store.selectNode(null)
        for (const id of ids) store.removeNode(id)
        e.preventDefault()
      }
    }
    if (e.key === 'Escape') {
      store.selectNode(null)
      store.selectWire(null)
      store.selectNote(null)
    }
    if ((e.ctrlKey || e.metaKey) && e.key === 's') {
      e.preventDefault()
      store.saveFlow()
    }
    if ((e.ctrlKey || e.metaKey) && e.key === 'z' && !e.shiftKey) {
      e.preventDefault()
      store.undo()
    }
    if ((e.ctrlKey || e.metaKey) && e.key === 'z' && e.shiftKey) {
      e.preventDefault()
      store.redo()
    }
    if ((e.ctrlKey || e.metaKey) && e.key === 'y') {
      e.preventDefault()
      store.redo()
    }
    if ((e.ctrlKey || e.metaKey) && e.key === 'a') {
      e.preventDefault()
      store.selectAll()
    }
    if ((e.ctrlKey || e.metaKey) && e.key === 'd') {
      e.preventDefault()
      store.duplicateSelected()
    }
  }
</script>

<svelte:window onkeydown={handleKeyDown} />

<div
  bind:this={canvasEl}
  class="flex-1 relative bg-[#0f0f23] overflow-hidden"
  style="background-image: radial-gradient(circle at 1px 1px, #1e1e3a 1px, transparent 0); background-size: 24px 24px;"
  onclick={handleCanvasClick}
  onmousedown={handleCanvasMouseDown}
  onmousemove={handleCanvasMouseMove}
  onmouseup={handleCanvasMouseUp}
  onwheel={handleCanvasWheel}
  onkeydown={handleKeyDown}
  tabindex="0"
  role="application"
  aria-label="Flow canvas"
>
  <div
    class="absolute inset-0"
    style="transform: translate({store.panX}px, {store.panY}px) scale({store.zoom}); transform-origin: 0 0;"
  >
    <svg class="absolute inset-0 w-full h-full pointer-events-none">
      <defs>
        <filter id="wire-glow">
          <feDropShadow dx="0" dy="0" stdDeviation="2" flood-color="#7c3aed" flood-opacity="0.5" />
        </filter>
      </defs>
      {#each wireHitPaths as wp}
        <path
          d={wp.path}
          stroke="transparent"
          stroke-width="14"
          fill="none"
          class="pointer-events-auto cursor-pointer"
          onclick={(e) => { e.stopPropagation(); store.selectWire(wp.id) }}
          role="button"
          aria-label="Select wire"
        ></path>
      {/each}
      {#each store.wires as wire}
        {@const from = nodePortPos(wire.from.nodeId, wire.from.port, 'output')}
        {@const to = nodePortPos(wire.to.nodeId, wire.to.port, 'input')}
        {#if from && to}
          <path
            d={wirePath(from.x, from.y, to.x, to.y)}
            stroke={wire.id === store.selectedWireId ? '#a78bfa' : '#7c3aed'}
            stroke-width={wire.id === store.selectedWireId ? 3 : 2}
            fill="none"
            class="pointer-events-none"
            filter={wire.id === store.selectedWireId ? 'url(#wire-glow)' : undefined}
          ></path>
        {/if}
      {/each}
      {#each store.wires as wire}
        {@const from = nodePortPos(wire.from.nodeId, wire.from.port, 'output')}
        {@const to = nodePortPos(wire.to.nodeId, wire.to.port, 'input')}
        {#if from && to && wire.id === store.selectedWireId}
          <circle cx={from.x} cy={from.y} r="6" fill="#a78bfa" stroke="#7c3aed" stroke-width="2" class="pointer-events-auto cursor-grab" data-wire-endpoint={wire.id} data-wire-side="from" onmousedown={(e) => handleReconnectEndpointMouseDown(e, wire.id, 'from')} role="button" aria-label="Drag to reconnect wire"></circle>
          <circle cx={to.x} cy={to.y} r="6" fill="#a78bfa" stroke="#7c3aed" stroke-width="2" class="pointer-events-auto cursor-grab" data-wire-endpoint={wire.id} data-wire-side="to" onmousedown={(e) => handleReconnectEndpointMouseDown(e, wire.id, 'to')} role="button" aria-label="Drag to reconnect wire"></circle>
        {/if}
      {/each}
      {#if draggingPos}
        <path d={wirePath(draggingPos.x1, draggingPos.y1, draggingPos.x2, draggingPos.y2)} stroke="#7c3aed" stroke-width="2" stroke-dasharray="5,5" fill="none" class="pointer-events-none"></path>
      {/if}
      {#if reconnectPos}
        <path d={wirePath(reconnectPos.x1, reconnectPos.y1, reconnectPos.x2, reconnectPos.y2)} stroke="#f59e0b" stroke-width="2" stroke-dasharray="5,5" fill="none" class="pointer-events-none"></path>
      {/if}
    </svg>
    {#each store.nodes as node (node.id)}
      <NodeBlock {node} />
    {/each}
    {#each store.notes as note (note.id)}
      <NoteBlock {note} />
    {/each}
    {#if store.selectionBox}
      <div
        class="absolute border border-[#7c3aed] bg-[#7c3aed]/10 pointer-events-none"
        style="left: {Math.min(store.selectionBox.x1, store.selectionBox.x2)}px; top: {Math.min(store.selectionBox.y1, store.selectionBox.y2)}px; width: {Math.abs(store.selectionBox.x2 - store.selectionBox.x1)}px; height: {Math.abs(store.selectionBox.y2 - store.selectionBox.y1)}px;"
      ></div>
    {/if}
  </div>
</div>
