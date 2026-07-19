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

<svelte:window
  onmousemove={handleMouseMove}
  onmouseup={handleMouseUp}
/>

<div
  bind:this={el}
  class="node"
  class:selected={node.selected}
  class:dragging
  style="left: {node.position.x}px; top: {node.position.y}px"
  onmousedown={handleMouseDown}
>
  <div class="header">{node.use}</div>
  <div class="body">
    <span class="id">{node.id}</span>
    {#each Object.entries(node.inputs ?? {}) as [k, v]}
      <div class="io">
        <span class="dot" />{k} ← {v}
      </div>
    {/each}
  </div>
</div>

<style>
  .node {
    position: absolute;
    min-width: 160px;
    background: #1a1a32;
    border: 1px solid #333;
    border-radius: 6px;
    cursor: move;
    font-size: 0.75rem;
    z-index: 10;
    user-select: none;
  }
  .node:hover { border-color: #555 }
  .node.selected { border-color: #7c3aed; box-shadow: 0 0 8px rgba(124,58,237,0.4) }
  .node.dragging { opacity: 0.85; z-index: 100 }
  .header {
    padding: 0.35rem 0.6rem;
    background: #2a2a4a;
    border-bottom: 1px solid #333;
    border-radius: 5px 5px 0 0;
    font-weight: 600;
    color: #7c3aed;
    font-family: monospace;
  }
  .body {
    padding: 0.4rem 0.6rem;
    color: #aaa;
  }
  .id {
    display: block;
    color: #888;
    font-size: 0.65rem;
    margin-bottom: 0.25rem;
  }
  .io {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.7rem;
    color: #999;
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #7c3aed;
    display: inline-block;
  }
</style>
