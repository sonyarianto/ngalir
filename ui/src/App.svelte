<script lang="ts">
  import { onMount } from 'svelte'
  import CredentialsPage from './lib/CredentialsPage.svelte'
  import HistoryPage from './lib/HistoryPage.svelte'
  import Toolbar from './lib/Toolbar.svelte'
  import NodePalette from './lib/NodePalette.svelte'
  import FlowCanvas from './lib/FlowCanvas.svelte'
  import PropertyPanel from './lib/PropertyPanel.svelte'
  import { getStore } from './lib/store.svelte.js'
  import Toast from './lib/Toast.svelte'

  const store = getStore()
  let toastMessage = $state('')
  let toastType = $state<'success' | 'error'>('success')

  onMount(() => { store.loadSample(); store.fetchSkills(); store.fetchCredentials() })

  $effect(() => {
    if (store.oauthMessage) {
      toastType = store.oauthType
      toastMessage = store.oauthMessage
      store.oauthMessage = ''
    }
  })

  onMount(() => {
    const params = new URLSearchParams(window.location.search)
    const success = params.get('oauth_success')
    const error = params.get('oauth_error')
    if (success) {
      toastType = 'success'
      toastMessage = `OAuth credential created (${success})`
      store.fetchCredentials()
      store.navigateTo('credentials')
      history.replaceState({}, '', '/')
    } else if (error) {
      toastType = 'error'
      toastMessage = `OAuth failed: ${error}`
      history.replaceState({}, '', '/')
    }
  })
</script>

{#if toastMessage}
  <Toast {toastType} message={toastMessage} onclose={() => toastMessage = ''} />
{/if}

{#if store.currentPage === 'credentials'}
  <CredentialsPage />
{:else if store.currentPage === 'history'}
  <HistoryPage />
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
