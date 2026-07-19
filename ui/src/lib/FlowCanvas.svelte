<script lang="ts">
  import { getStore } from './store.svelte.js'
  import NodeBlock from './NodeBlock.svelte'

  const store = getStore()

  const NODE_W = 160
  const HEADER_H = 24
  const PORT_SPACING = 20

  let canvasEl: HTMLElement | undefined = $state()
  let panning = $state(false)
  let panStart = $state({ x: 0, y: 0 })

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
    const keys = side === 'input' ? Object.keys(n.inputs ?? {}) : ['output']
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
    if (panning) {
      panning = false
      return
    }
  }

  function handleCanvasClick(e: MouseEvent) {
    if (store.draggingWire || panning) return
    const target = e.target as HTMLElement
    if (target.closest('[data-port-input]') || target.closest('[data-port-output]') || target.closest('[data-node-id]')) return
    if (target.closest('[data-wire]')) return
    store.selectNode(null)
    store.selectWire(null)
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

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Delete' || e.key === 'Backspace') {
      if (store.selectedWireId) {
        store.removeWire(store.selectedWireId)
        e.preventDefault()
      } else if (store.selectedId) {
        store.removeNode(store.selectedId)
        e.preventDefault()
      }
    }
    if (e.key === 'Escape') {
      store.selectNode(null)
      store.selectWire(null)
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
  role="img"
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
        />
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
          />
        {/if}
      {/each}
      {#if draggingPos}
        <path d={wirePath(draggingPos.x1, draggingPos.y1, draggingPos.x2, draggingPos.y2)} stroke="#7c3aed" stroke-width="2" stroke-dasharray="5,5" fill="none" class="pointer-events-none" />
      {/if}
    </svg>
    {#each store.nodes as node (node.id)}
      <NodeBlock {node} />
    {/each}
  </div>
</div>
