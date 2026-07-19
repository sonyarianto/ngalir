<script lang="ts">
  import { getStore } from './store.svelte.js'
  import NodeBlock from './NodeBlock.svelte'

  const store = getStore()
</script>

<div
  class="flex-1 relative bg-[#0f0f23] overflow-hidden"
  style="background-image: radial-gradient(circle at 1px 1px, #1e1e3a 1px, transparent 0); background-size: 24px 24px;"
  onclick={() => store.selectNode(null)}
>
  <svg class="absolute inset-0 w-full h-full pointer-events-none">
    {#each store.wires as wire}
      <line x1={100} y1={100} x2={300} y2={100} stroke="#7c3aed" stroke-width="2" stroke-dasharray="4" />
    {/each}
  </svg>
  {#each store.nodes as node (node.id)}
    <NodeBlock {node} />
  {/each}
</div>
