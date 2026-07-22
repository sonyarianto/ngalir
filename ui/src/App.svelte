<script lang="ts">
  import CredentialsPage from './lib/CredentialsPage.svelte'
  import Toolbar from './lib/Toolbar.svelte'
  import NodePalette from './lib/NodePalette.svelte'
  import FlowCanvas from './lib/FlowCanvas.svelte'
  import PropertyPanel from './lib/PropertyPanel.svelte'
  import { getStore } from './lib/store.svelte.js'

  const store = getStore()
  $effect(() => { store.loadSample(); store.fetchSkills(); store.fetchCredentials() })
</script>

{#if store.currentPage === 'credentials'}
  <CredentialsPage />
{:else}
  <div class="h-screen flex flex-col bg-[#0f0f23] text-[#e0e0e0]">
    <Toolbar />
    <div class="flex flex-1 overflow-hidden">
      <NodePalette />
      <FlowCanvas />
      <PropertyPanel />
    </div>
  </div>
{/if}
