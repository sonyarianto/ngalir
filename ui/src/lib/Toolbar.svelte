<script lang="ts">
  import { getStore } from './store.svelte.js'

  const store = getStore()

  function handleSave() {
    const yaml = store.exportFlow()
    const blob = new Blob([yaml], { type: 'text/yaml' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = store.filename || `${store.flowName}.yaml`
    a.click()
    URL.revokeObjectURL(url)
  }

  function handleLoad() {
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = '.yaml,.json'
    input.onchange = async () => {
      const file = input.files?.[0]
      if (!file) return
      store.filename = file.name
      store.importFlow(await file.text())
    }
    input.click()
  }
</script>

<header>
  <div class="brand">Ngalir</div>
  <span class="name">{store.flowName}</span>
  <div class="spacer" />
  <button onclick={handleLoad}>Open</button>
  <button onclick={handleSave}>Save</button>
  <button onclick={() => store.loadSample()}>Sample</button>
  <button class="run" onclick={() => alert('Run: connect to orchestrator')}>Run</button>
</header>

<style>
  header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 1rem;
    background: #1a1a2e;
    color: #e0e0e0;
    border-bottom: 1px solid #333;
    height: 48px;
  }
  .brand {
    font-weight: 700;
    font-size: 1.1rem;
    color: #7c3aed;
    margin-right: 0.5rem;
  }
  .name {
    font-size: 0.9rem;
    opacity: 0.7;
  }
  .spacer { flex: 1 }
  button {
    padding: 0.3rem 0.8rem;
    border: 1px solid #444;
    border-radius: 4px;
    background: #2a2a3e;
    color: #e0e0e0;
    cursor: pointer;
    font-size: 0.8rem;
  }
  button:hover { background: #3a3a4e }
  button.run { background: #7c3aed; border-color: #7c3aed }
  button.run:hover { background: #6d28d9 }
</style>
