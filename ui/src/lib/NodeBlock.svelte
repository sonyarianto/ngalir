<script lang="ts">
  import type { CanvasNode } from './types'
  import { getStore } from './store.svelte.js'

  let { node }: { node: CanvasNode } = $props()

  const store = getStore()

  let dragging = $state(false)
  let offsetX = $state(0)
  let offsetY = $state(0)
  let el: HTMLElement | undefined = $state()

  const PORT_SPACING = 20
  const HEADER_H = 24
  const NODE_W = 160

  const manifest = $derived(store.skillsMap[node.use])

  const inputPorts = $derived(
    manifest
      ? Object.keys(manifest.inputs)
      : Object.keys(node.inputs ?? {})
  )
  const outputPorts = $derived(
    manifest && Object.keys(manifest.outputs).length > 0
      ? Object.keys(manifest.outputs)
      : ['output']
  )

  function portY(index: number, total: number) {
    return HEADER_H + 4 + index * PORT_SPACING + PORT_SPACING / 2
  }

  function isConnected(port: string): boolean {
    return !!(node.inputs as Record<string, string>)?.[port]
  }

  function handleMouseDown(e: MouseEvent) {
    e.stopPropagation()
    if (e.shiftKey) {
      store.toggleNodeSelection(node.id)
    } else {
      if (!store.selectedIds.includes(node.id)) {
        store.selectNode(node.id)
      }
    }
    dragging = true
    const rect = el?.getBoundingClientRect()
    offsetX = e.clientX - (rect?.left ?? 0)
    offsetY = e.clientY - (rect?.top ?? 0)
  }

  function handleMouseMove(e: MouseEvent) {
    if (!dragging) return
    const parent = el?.parentElement
    if (!parent) return
    const pr = parent.getBoundingClientRect()
    store.updateNodePosition(node.id, {
      x: e.clientX - pr.left - offsetX,
      y: e.clientY - pr.top - offsetY,
    })
  }

  function handleMouseUp() {
    if (dragging) store.pushUndo()
    dragging = false
  }

  function handlePortMouseDown(e: MouseEvent, port: string) {
    e.stopPropagation()
    const parent = el?.parentElement
    if (!parent) return
    const pr = parent.getBoundingClientRect()
    store.startDragWire(node.id, port, e.clientX - pr.left, e.clientY - pr.top)
  }

  function handlePortMouseUp(e: MouseEvent, port: string) {
    e.stopPropagation()
    store.endDragWire(node.id, port)
  }
</script>

<svelte:window onmousemove={handleMouseMove} onmouseup={handleMouseUp} />

<div
  bind:this={el}
  data-node-id={node.id}
  class="absolute min-w-40 bg-[#1a1a32] border rounded-lg cursor-move text-xs z-10 select-none"
  class:border-[#7c3aed]!={node.selected}
  class:shadow-[0_0_8px_rgba(124,58,237,0.4)]={node.selected}
  class:opacity-85={dragging}
  class:z-100={dragging}
  class:border-[#555]={!node.selected}
  style="left: {node.position.x}px; top: {node.position.y}px"
  onmousedown={handleMouseDown}
>
  <div class="px-2 py-1 bg-[#2a2a4a] border-b border-[#333] rounded-t-md font-semibold text-[#7c3aed] font-mono flex items-center gap-2">
      <span class="flex-1">{node.use}</span>
    {#if node.status}
      <span role="status"
        class="w-2 h-2 rounded-full inline-block"
        class:bg-yellow-400={node.status === 'pending'}
        class:bg-blue-400={node.status === 'running'}
        class:bg-green-400={node.status === 'done'}
        class:bg-red-400={node.status === 'failed'}
      ></span>
    {/if}
  </div>
  <div class="px-2 py-1 text-[#aaa] min-h-[24px]">
    <span class="block text-[10px] text-[#888] mb-1">{node.id}</span>
    {#each inputPorts as port, i}
      <div class="flex items-center gap-1 text-[11px] text-[#999] relative">
        <span
          class="w-1.5 h-1.5 rounded-full inline-block cursor-crosshair z-20"
          class:bg-[#7c3aed]={isConnected(port)}
          class:bg-[#555]={!isConnected(port)}
          data-port-input
          data-node-id={node.id}
          data-port={port}
          onmouseup={(e) => handlePortMouseUp(e, port)}
          role="button"></span>{port} {#if isConnected(port)}← {(node.inputs as Record<string, string>)?.[port] ?? ''}{:else}(unconnected){/if}
      </div>
    {/each}
    {#each outputPorts as port, i}
      <div class="flex items-center justify-end gap-1 text-[11px] text-[#999] relative">
        <span class="flex-1"></span>
        <span role="button"
          class="w-1.5 h-1.5 rounded-full bg-green-400 inline-block cursor-crosshair z-20"
          data-port-output
          data-node-id={node.id}
          onmousedown={(e) => handlePortMouseDown(e, port)}
        />
      </div>
    {/each}
  </div>
</div>
