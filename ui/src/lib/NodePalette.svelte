<script lang="ts">
  import { CATEGORIES } from './types'
  import { getStore } from './store.svelte.js'

  const store = getStore()

  function addToCanvas(type: string) {
    store.addNode(type, { x: 200 + Math.random() * 100, y: 150 + Math.random() * 100 })
  }
</script>

<aside>
  <h3>Nodes</h3>
  {#each CATEGORIES as cat}
    <h4>{cat.name}</h4>
    {#each cat.nodes as type}
      <button onclick={() => addToCanvas(type)}>
        na-{type}
      </button>
    {/each}
  {/each}
</aside>

<style>
  aside {
    width: 160px;
    background: #16162a;
    border-right: 1px solid #333;
    padding: 0.5rem;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  h3 {
    font-size: 0.85rem;
    color: #7c3aed;
    margin: 0 0 0.5rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  h4 {
    font-size: 0.7rem;
    color: #888;
    margin: 0.5rem 0 0.25rem;
    text-transform: uppercase;
  }
  button {
    padding: 0.35rem 0.6rem;
    border: 1px solid #333;
    border-radius: 4px;
    background: #1e1e36;
    color: #ccc;
    cursor: pointer;
    text-align: left;
    font-size: 0.8rem;
    font-family: monospace;
  }
  button:hover {
    background: #2e2e4e;
    border-color: #7c3aed;
  }
</style>
