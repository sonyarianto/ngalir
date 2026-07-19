<script lang="ts">
  import type { CanvasNode } from './types'
  import { getStore } from './store.svelte.js'

  let { node }: { node: CanvasNode } = $props()

  const store = getStore()

  let dragging = $state(false)
  let offsetX = $state(0)
  let offsetY = $state(0)
  let el: HTMLElement | undefined = $state()

  function handleMouseDown(e: MouseEvent) {
    e.stopPropagation()
    store.selectNode(node.id)
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
    dragging = false
  }
</script>

<svelte:window onmousemove={handleMouseMove} onmouseup={handleMouseUp} />

<div
  bind:this={el}
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
      <span
        class="w-2 h-2 rounded-full inline-block"
        class:bg-yellow-400={node.status === 'pending'}
        class:bg-blue-400={node.status === 'running'}
        class:bg-green-400={node.status === 'done'}
        class:bg-red-400={node.status === 'failed'}
      />
    {/if}
  </div>
  <div class="px-2 py-1 text-[#aaa]">
    <span class="block text-[10px] text-[#888] mb-1">{node.id}</span>
    {#each Object.entries(node.inputs ?? {}) as [k, v]}
      <div class="flex items-center gap-1 text-[11px] text-[#999]">
        <span class="w-1.5 h-1.5 rounded-full bg-[#7c3aed] inline-block" />{k} ← {v}
      </div>
    {/each}
  </div>
</div>
