<script lang="ts">
  import { getStore } from './store.svelte.js'

  const store = getStore()

  function handleDownloadYaml() {
    const yamlStr = store.exportYaml()
    const blob = new Blob([yamlStr], { type: 'text/yaml' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = store.filename || `${store.flowName}.yaml`
    a.click()
    URL.revokeObjectURL(url)
  }

  function handleDownloadJson() {
    const json = store.exportFlow()
    const blob = new Blob([json], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = store.filename?.replace(/\.yaml$/, '.json') || `${store.flowName}.json`
    a.click()
    URL.revokeObjectURL(url)
  }

  function handleLoad() {
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = '.yaml,.json,.yml'
    input.onchange = async () => {
      const file = input.files?.[0]
      if (!file) return
      store.filename = file.name
      const text = await file.text()
      if (file.name.endsWith('.yaml') || file.name.endsWith('.yml')) {
        store.importYaml(text)
      } else {
        store.importFlow(text)
      }
    }
    input.click()
  }

  function handleAddNote() {
    store.addNote({ x: 200 + Math.random() * 100, y: 150 + Math.random() * 100 })
  }
</script>

<header class="flex items-center gap-2 px-4 py-2 bg-[#1a1a2e] border-b border-[#333] h-12">
  <span class="font-bold text-lg text-[#7c3aed]">Ngalir</span>
  <span class="text-sm opacity-60">{store.flowName}</span>
  <button
    class="px-3 py-1 border border-[#7c3aed] rounded bg-[#3a2a6e] text-sm cursor-pointer hover:bg-[#4a3a7e]"
    onclick={() => store.navigateTo('credentials')}
  >
    Credentials
  </button>
  <button
    class="px-3 py-1 border border-[#7c3aed] rounded bg-[#3a2a6e] text-sm cursor-pointer hover:bg-[#4a3a7e]"
    onclick={() => { store.fetchHistory(); store.navigateTo('history') }}
  >
    History
  </button>
  <div class="flex-1"></div>
  <button class="px-3 py-1 border border-[#444] rounded bg-[#2a2a3e] text-sm cursor-pointer hover:bg-[#3a3a4e]" onclick={() => store.listFlows()}>Flows</button>
  <button class="px-3 py-1 border border-[#444] rounded bg-[#2a2a3e] text-sm cursor-pointer hover:bg-[#3a3a4e]" onclick={handleLoad}>Open</button>
  <div class="relative group">
    <button class="px-3 py-1 border border-[#444] rounded bg-[#2a2a3e] text-sm cursor-pointer hover:bg-[#3a3a4e]">Export</button>
    <div class="absolute top-full right-0 mt-1 hidden group-hover:block z-50 bg-[#1a1a2e] border border-[#444] rounded shadow-xl">
      <button class="block w-full px-3 py-1.5 text-sm text-left text-[#ccc] hover:bg-[#2a2a3e] cursor-pointer whitespace-nowrap" onclick={handleDownloadYaml}>Export YAML</button>
      <button class="block w-full px-3 py-1.5 text-sm text-left text-[#ccc] hover:bg-[#2a2a3e] cursor-pointer whitespace-nowrap" onclick={handleDownloadJson}>Export JSON</button>
    </div>
  </div>
  <button class="px-3 py-1 border border-[#444] rounded bg-[#2a2a3e] text-sm cursor-pointer hover:bg-[#3a3a4e]" onclick={() => store.saveFlow()}>Save</button>
  <button class="px-3 py-1 border border-[#444] rounded bg-[#2a2a3e] text-sm cursor-pointer hover:bg-[#3a3a4e]" onclick={() => store.loadSample()}>Sample</button>
  <button class="px-3 py-1 border border-[#444] rounded bg-[#2a2a3e] text-sm cursor-pointer hover:bg-[#3a3a4e]" onclick={handleAddNote}>Note</button>
  <button class="px-3 py-1 border border-[#444] rounded bg-[#2a2a3e] text-sm cursor-pointer hover:bg-[#3a3a4e]" onclick={() => store.selectAll()}>Select All</button>
  <button class="px-3 py-1 border border-[#444] rounded bg-[#2a2a3e] text-sm cursor-pointer hover:bg-[#3a3a4e]" onclick={() => store.duplicateSelected()}>Duplicate</button>
  <button class="px-3 py-1 border border-[#444] rounded bg-[#2a2a3e] text-sm cursor-pointer hover:bg-[#3a3a4e]" onclick={() => store.autoLayout()}>Layout</button>
  <span class="w-px h-4 bg-[#444]"></span>
  <button class="px-2 py-1 border border-[#444] rounded bg-[#2a2a3e] text-sm cursor-pointer hover:bg-[#3a3a4e]" onclick={() => store.undo()}>↩</button>
  <button class="px-2 py-1 border border-[#444] rounded bg-[#2a2a3e] text-sm cursor-pointer hover:bg-[#3a3a4e]" onclick={() => store.redo()}>↪</button>
  {#if store.stepReady}
    <button class="px-3 py-1 border border-green-500 rounded bg-green-700 text-sm cursor-pointer hover:bg-green-600" onclick={() => store.stepContinue()}>Continue</button>
    <button class="px-3 py-1 border border-red-500 rounded bg-red-700 text-sm cursor-pointer hover:bg-red-600" onclick={() => store.stepStop()}>Stop</button>
  {:else if !store.running}
    <button class="px-3 py-1 border border-[#7c3aed] rounded bg-[#7c3aed] text-sm cursor-pointer hover:bg-[#6d28d9]" onclick={() => store.runFlow()}>Run</button>
    <button class="px-3 py-1 border border-[#7c3aed] rounded bg-[#3a2a6e] text-sm cursor-pointer hover:bg-[#4a3a7e]" onclick={() => store.runStepFlow()}>Step</button>
  {:else}
    <span class="text-sm text-yellow-400">Running{store.stepMode ? '…' : ''}</span>
  {/if}
</header>

{#if store.showFlowList}
  <div class="absolute top-12 left-2 z-50 bg-[#1a1a2e] border border-[#444] rounded shadow-xl w-64 max-h-80 overflow-y-auto">
    <div class="flex items-center justify-between px-3 py-2 border-b border-[#333]">
      <span class="text-xs text-[#7c3aed] uppercase tracking-wider">Saved Flows</span>
      <button class="text-[#888] text-xs cursor-pointer hover:text-[#ccc]" onclick={() => store.showFlowList = false}>✕</button>
    </div>
    {#if store.savedFlows.length === 0}
      <p class="px-3 py-4 text-xs text-[#555] text-center">No saved flows</p>
    {:else}
      {#each store.savedFlows as f}
        <div class="flex items-center gap-2 px-3 py-2 border-b border-[#2a2a3e] hover:bg-[#2a2a3e]">
          <button class="flex-1 text-left text-xs text-[#ccc] cursor-pointer" onclick={() => store.loadFlow(f.name)}>{f.name}</button>
          <button class="text-[#ef4444] text-xs cursor-pointer hover:text-red-300" onclick={() => store.deleteFlow(f.name)}>x</button>
        </div>
      {/each}
    {/if}
  </div>
{/if}
