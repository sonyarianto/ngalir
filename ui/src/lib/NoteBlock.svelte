<script lang="ts">
  import type { CanvasNote } from './types'
  import { getStore } from './store.svelte.js'

  let { note }: { note: CanvasNote } = $props()

  const store = getStore()

  let dragging = $state(false)
  let offsetX = $state(0)
  let offsetY = $state(0)
  let el: HTMLElement | undefined = $state()

  function handleMouseDown(e: MouseEvent) {
    e.stopPropagation()
    store.selectNote(note.id)
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
    store.updateNote(note.id, {
      position: {
        x: e.clientX - pr.left - offsetX,
        y: e.clientY - pr.top - offsetY,
      },
    })
  }

  function handleMouseUp() {
    if (dragging) store.pushUndo()
    dragging = false
  }

  function handleTextInput(e: Event) {
    store.updateNote(note.id, { text: (e.target as HTMLTextAreaElement).value })
  }

  const COLORS = ['#fff3cd', '#f8d7da', '#d1e7dd', '#cfe2ff', '#e2d5f5', '#fff']
</script>

<svelte:window onmousemove={handleMouseMove} onmouseup={handleMouseUp} />

<div
  bind:this={el}
  data-note={note.id}
  class="absolute rounded-lg shadow-md z-20 select-none"
  class:ring-2={note.selected}
  class:ring-[#7c3aed]={note.selected}
  style="left: {note.position.x}px; top: {note.position.y}px; width: {note.width}px; height: {note.height}px; background-color: {note.color};"
  onmousedown={handleMouseDown}
  role="region"
  aria-label="Sticky note"
>
  <div class="flex items-center justify-between px-2 py-1 bg-black/10 rounded-t-lg cursor-move">
    <span class="text-[10px] text-black/50 uppercase tracking-wider">Note</span>
    {#if note.selected}
      <div class="flex gap-1">
        {#each COLORS as c}
          <button
            class="w-3 h-3 rounded-full border border-black/20 cursor-pointer"
            style="background-color: {c};"
            onclick={(e) => { e.stopPropagation(); store.updateNote(note.id, { color: c }) }}
            aria-label="Color {c}"
          ></button>
        {/each}
      </div>
    {/if}
  </div>
  <textarea
    class="w-full h-[calc(100%-24px)] bg-transparent text-xs text-black/80 p-2 resize-none outline-none font-sans"
    value={note.text}
    oninput={handleTextInput}
    onclick={(e) => e.stopPropagation()}
  ></textarea>
</div>
