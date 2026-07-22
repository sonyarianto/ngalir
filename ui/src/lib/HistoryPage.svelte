<script lang="ts">
  import { getStore } from './store.svelte.js'

  const store = getStore()
  let selectedRun = $state<string | null>(null)

  function viewRun(flowId: string) {
    selectedRun = flowId
    store.fetchHistoryRun(flowId)
  }

  function back() {
    selectedRun = null
    store.fetchHistory()
  }

  function formatDuration(ms: number | null): string {
    if (ms == null) return '-'
    if (ms < 1000) return `${ms}ms`
    if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`
    return `${Math.floor(ms / 60000)}m ${Math.floor((ms % 60000) / 1000)}s`
  }

  function statusColor(status: string): string {
    if (status === 'completed') return 'text-green-400'
    if (status === 'failed') return 'text-red-400'
    if (status === 'running') return 'text-yellow-400'
    if (status === 'stopped') return 'text-orange-400'
    return 'text-gray-400'
  }

  function formatTimestamp(iso: string): string {
    if (!iso) return '-'
    return iso.replace('T', ' ').replace('Z', '')
  }
</script>

<div class="h-screen flex flex-col bg-[#0f0f23] text-[#e0e0e0]">
  <header class="flex items-center gap-2 px-4 py-2 bg-[#1a1a2e] border-b border-[#333] h-12">
    <span class="font-bold text-lg text-[#7c3aed]">Ngalir</span>
    <span class="text-sm opacity-60">Execution History</span>
    <div class="flex-1"></div>
    <button
      class="px-3 py-1 border border-[#7c3aed] rounded bg-[#3a2a6e] text-sm cursor-pointer hover:bg-[#4a3a7e]"
      onclick={() => store.navigateTo('editor')}
    >
      Back to Editor
    </button>
  </header>

  <div class="flex-1 overflow-y-auto p-6">
    {#if selectedRun && store.historyRunDetail}
      <!-- Run Detail View -->
      <button onclick={back} class="text-sm text-[#7c3aed] hover:text-[#9a5aed] mb-4 cursor-pointer">&larr; Back to list</button>
      <div class="bg-[#1a1a2e] rounded-lg p-4 mb-6">
        <h2 class="text-lg font-bold mb-2">{store.historyRunDetail.flow?.flow_name ?? 'Unknown'}</h2>
        <div class="grid grid-cols-4 gap-4 text-sm">
          <div>
            <span class="text-[#666688]">Status</span>
            <p class={statusColor(store.historyRunDetail.flow?.status ?? '')}>{store.historyRunDetail.flow?.status}</p>
          </div>
          <div>
            <span class="text-[#666688]">Started</span>
            <p>{store.historyRunDetail.flow?.started_at?.replace('T', ' ')?.replace('Z', '') ?? '-'}</p>
          </div>
          <div>
            <span class="text-[#666688]">Duration</span>
            <p>{formatDuration(store.historyRunDetail.flow?.duration_ms)}</p>
          </div>
          <div>
            <span class="text-[#666688]">Nodes</span>
            <p>{store.historyRunDetail.flow?.node_count ?? 0}</p>
          </div>
        </div>
        {#if store.historyRunDetail.flow?.error}
          <div class="mt-2 p-2 bg-red-900/30 rounded text-sm text-red-300">
            {store.historyRunDetail.flow.error}
          </div>
        {/if}
      </div>

      <h3 class="text-sm font-semibold text-[#888] uppercase tracking-wider mb-2">Nodes</h3>
      <div class="space-y-2">
        {#each store.historyRunDetail.nodes ?? [] as node}
          <div class="bg-[#1a1a2e] rounded-lg p-3">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-2">
                <span class="text-sm font-medium">{node.node_id}</span>
                <span class="text-xs text-[#666688]">({node.node_type})</span>
                <span class="text-xs {statusColor(node.status)}">{node.status}</span>
              </div>
              <span class="text-xs text-[#666688]">{formatDuration(node.duration_ms)}</span>
            </div>
            {#if node.error}
              <div class="mt-1 text-xs text-red-400">{node.error}</div>
            {/if}
          </div>
        {/each}
      </div>
    {:else}
      <!-- Run List View -->
      <h2 class="text-sm font-semibold text-[#888] uppercase tracking-wider mb-3">Past Runs</h2>
      {#if store.historyRuns.length === 0}
        <p class="text-sm text-[#555] text-center mt-12">No execution history yet. Run a flow to see it here.</p>
      {:else}
        <div class="space-y-2">
          {#each store.historyRuns as run}
            <button
              onclick={() => viewRun(run.flow_id as string)}
              class="w-full text-left bg-[#1a1a2e] rounded-lg p-3 hover:bg-[#2a2a3e] cursor-pointer transition-colors"
            >
              <div class="flex items-center justify-between">
                <div>
                  <span class="text-sm font-medium">{run.flow_name as string}</span>
                  <span class="ml-2 text-xs {statusColor(run.status as string)}">{run.status as string}</span>
                </div>
                <span class="text-xs text-[#666688]">{formatDuration(run.duration_ms as number | null)}</span>
              </div>
              <div class="flex items-center gap-4 mt-1 text-xs text-[#666688]">
                <span>{formatTimestamp(run.started_at as string)}</span>
                <span>{run.node_count as number} nodes</span>
              </div>
            </button>
          {/each}
        </div>
      {/if}
    {/if}
  </div>
</div>


