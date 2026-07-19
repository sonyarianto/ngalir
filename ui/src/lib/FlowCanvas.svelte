<script lang="ts">
  import { getStore } from './store.svelte.js'
  import NodeBlock from './NodeBlock.svelte'

  const store = getStore()

  const NODE_W = 160
  const HEADER_H = 24
  const PORT_SPACING = 20
  const PORT_INSET = 16

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
    const cx = (x1 + x2) / 2
    return `M ${x1} ${y1} C ${cx} ${y1}, ${cx} ${y2}, ${x2} ${y2}`
  }

  function handleCanvasMouseMove(e: MouseEvent) {
    if (!store.draggingWire) return
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
    store.updateDragWire(e.clientX - rect.left, e.clientY - rect.top)
  }

  function handleCanvasMouseUp(e: MouseEvent) {
    if (!store.draggingWire) return
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
  }

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
</script>

<div
  class="flex-1 relative bg-[#0f0f23] overflow-hidden"
  style="background-image: radial-gradient(circle at 1px 1px, #1e1e3a 1px, transparent 0); background-size: 24px 24px;"
  onclick={() => store.selectNode(null)}
  onmousemove={handleCanvasMouseMove}
  onmouseup={handleCanvasMouseUp}
>
  <svg class="absolute inset-0 w-full h-full pointer-events-none">
    {#each store.wires as wire}
      {@const from = nodePortPos(wire.from.nodeId, wire.from.port, 'output')}
      {@const to = nodePortPos(wire.to.nodeId, wire.to.port, 'input')}
      {#if from && to}
        <path d={wirePath(from.x, from.y, to.x, to.y)} stroke="#7c3aed" stroke-width="2" fill="none" />
      {/if}
    {/each}
    {#if draggingPos}
      <path d={wirePath(draggingPos.x1, draggingPos.y1, draggingPos.x2, draggingPos.y2)} stroke="#7c3aed" stroke-width="2" stroke-dasharray="5,5" fill="none" />
    {/if}
  </svg>
  {#each store.nodes as node (node.id)}
    <NodeBlock {node} />
  {/each}
</div>
