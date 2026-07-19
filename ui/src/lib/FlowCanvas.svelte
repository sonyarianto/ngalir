<script lang="ts">
  import { getStore } from './store.svelte.js'
  import NodeBlock from './NodeBlock.svelte'

  const store = getStore()
</script>

<div class="canvas" onclick={() => store.selectNode(null)}>
  <svg class="wires">
    {#each store.wires as wire}
      <line
        x1={100} y1={100}
        x2={300} y2={100}
        stroke="#7c3aed"
        stroke-width="2"
        stroke-dasharray="4"
      />
    {/each}
  </svg>
  {#each store.nodes as node (node.id)}
    <NodeBlock {node} />
  {/each}
</div>

<style>
  .canvas {
    flex: 1;
    position: relative;
    background: #0f0f23;
    background-image:
      radial-gradient(circle at 1px 1px, #1e1e3a 1px, transparent 0);
    background-size: 24px 24px;
    overflow: hidden;
  }
  .wires {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
  }
</style>
