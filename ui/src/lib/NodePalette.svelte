<script lang="ts">
  import { CATEGORIES } from './types'
  import { getStore } from './store.svelte.js'

  const store = getStore()

  function addToCanvas(type: string) {
    store.addNode(type, { x: 200 + Math.random() * 100, y: 150 + Math.random() * 100 })
  }
</script>

<aside class="w-40 bg-[#16162a] border-r border-[#333] p-2 overflow-y-auto flex flex-col gap-1">
  <h3 class="text-xs text-[#7c3aed] uppercase tracking-wider mb-2">Nodes</h3>
  {#each CATEGORIES as cat}
    <h4 class="text-[10px] text-[#888] uppercase mt-2 mb-1">{cat.name}</h4>
    {#each cat.nodes as type}
      <button
        class="px-2 py-1 border border-[#333] rounded bg-[#1e1e36] text-xs text-[#ccc] text-left font-mono cursor-pointer hover:border-[#7c3aed] hover:bg-[#2e2e4e]"
        onclick={() => addToCanvas(type)}
      >
        na-{type}
      </button>
    {/each}
  {/each}
</aside>
