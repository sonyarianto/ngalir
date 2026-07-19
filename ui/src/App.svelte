<script lang="ts">
  import Toolbar from './lib/Toolbar.svelte'
  import NodePalette from './lib/NodePalette.svelte'
  import FlowCanvas from './lib/FlowCanvas.svelte'
  import PropertyPanel from './lib/PropertyPanel.svelte'
  import { getStore } from './lib/store.svelte.js'

  const store = getStore()
  $effect(() => { store.loadSample() })
</script>

<Toolbar />
<div class="workspace">
  <NodePalette />
  <FlowCanvas />
  <PropertyPanel />
</div>

<style>
  :global(*) {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
  }
  :global(body) {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: #0f0f23;
    color: #e0e0e0;
    overflow: hidden;
  }
  .workspace {
    display: flex;
    height: calc(100vh - 48px);
  }
</style>
