<script lang="ts">
  import { getStore } from './store.svelte.js'

  const store = getStore()

  let node = $derived(store.nodes.find((n) => n.id === store.selectedId))

  function updateWhen(e: Event) {
    const n = store.nodes.find((n) => n.id === store.selectedId)
    if (!n) return
    const val = (e.target as HTMLInputElement).value
    n.when = val || undefined
  }

  function updateOnError(e: Event) {
    const n = store.nodes.find((n) => n.id === store.selectedId)
    if (!n) return
    const val = (e.target as HTMLInputElement).value
    n.on_error = val || undefined
  }

  function updateId(e: Event) {
    const n = store.nodes.find((n) => n.id === store.selectedId)
    if (!n) return
    n.id = (e.target as HTMLInputElement).value
  }

  function updateUse(e: Event) {
    const n = store.nodes.find((n) => n.id === store.selectedId)
    if (!n) return
    n.use = (e.target as HTMLInputElement).value
  }

  function updateWith(e: Event) {
    const n = store.nodes.find((n) => n.id === store.selectedId)
    if (!n) return
    try { n.with = JSON.parse((e.target as HTMLTextAreaElement).value) }
    catch {}
  }

  function addInput() {
    const n = store.nodes.find((n) => n.id === store.selectedId)
    if (!n) return
    if (!n.inputs) n.inputs = {}
    const key = `input${Object.keys(n.inputs).length + 1}`
    ;(n.inputs as Record<string, string>)[key] = ''
  }

  function removeInput(key: string) {
    const n = store.nodes.find((n) => n.id === store.selectedId)
    if (!n || !n.inputs) return
    delete (n.inputs as Record<string, string>)[key]
  }

  function updateInputKey(oldKey: string, newKey: string) {
    const n = store.nodes.find((n) => n.id === store.selectedId)
    if (!n || !n.inputs) return
    const val = (n.inputs as Record<string, string>)[oldKey]
    delete (n.inputs as Record<string, string>)[oldKey]
    if (newKey) (n.inputs as Record<string, string>)[newKey] = val
  }

  function updateInputVal(key: string, val: string) {
    const n = store.nodes.find((n) => n.id === store.selectedId)
    if (!n || !n.inputs) return
    ;(n.inputs as Record<string, string>)[key] = val
  }
</script>

<aside>
  <h3>Properties</h3>
  {#if node}
    <div class="field">
      <label>id</label>
      <input value={node.id} oninput={updateId} />
    </div>
    <div class="field">
      <label>use</label>
      <input value={node.use} oninput={updateUse} />
    </div>
    <div class="field">
      <label>when</label>
      <input value={node.when ?? ''} placeholder="optional" oninput={updateWhen} />
    </div>
    <div class="field">
      <label>on_error</label>
      <input value={node.on_error ?? ''} placeholder="optional" oninput={updateOnError} />
    </div>
    <div class="field">
      <label>with (config)</label>
      <textarea
        value={JSON.stringify(node.with ?? {}, null, 2)}
        oninput={updateWith}
        rows="4"
      />
    </div>
    <div class="field">
      <label>inputs</label>
      {#each Object.entries(node.inputs ?? {}) as [k, v]}
        <div class="input-row">
          <input
            value={k}
            oninput={(e) => updateInputKey(k, e.currentTarget.value)}
            placeholder="key"
          />
          <span>←</span>
          <input
            value={v}
            oninput={(e) => updateInputVal(k, e.currentTarget.value)}
            placeholder="node.output"
          />
          <button class="remove" onclick={() => removeInput(k)}>x</button>
        </div>
      {/each}
      <button class="add" onclick={addInput}>+ Add input</button>
    </div>
  {:else}
    <p class="hint">Select a node to edit</p>
  {/if}
</aside>

<style>
  aside {
    width: 220px;
    background: #16162a;
    border-left: 1px solid #333;
    padding: 0.5rem;
    overflow-y: auto;
    font-size: 0.75rem;
  }
  h3 {
    font-size: 0.85rem;
    color: #7c3aed;
    margin: 0 0 0.5rem;
    text-transform: uppercase;
  }
  .field {
    margin-bottom: 0.6rem;
  }
  label {
    display: block;
    color: #888;
    font-size: 0.65rem;
    text-transform: uppercase;
    margin-bottom: 0.15rem;
  }
  input, textarea {
    width: 100%;
    padding: 0.3rem;
    border: 1px solid #333;
    border-radius: 3px;
    background: #0f0f23;
    color: #e0e0e0;
    font-size: 0.75rem;
    font-family: monospace;
    box-sizing: border-box;
  }
  textarea { resize: vertical }
  .hint {
    color: #555;
    font-style: italic;
    text-align: center;
    margin-top: 2rem;
  }
  .input-row {
    display: flex;
    gap: 0.25rem;
    align-items: center;
    margin-bottom: 0.25rem;
  }
  .input-row input { flex: 1; min-width: 0 }
  .input-row span { color: #555; font-size: 0.7rem }
  button.add, button.remove {
    padding: 0.15rem 0.4rem;
    border: 1px solid #333;
    border-radius: 3px;
    background: #1e1e36;
    color: #ccc;
    cursor: pointer;
    font-size: 0.7rem;
  }
  button.add { width: 100%; margin-top: 0.25rem }
  button.remove { color: #ef4444 }
  button:hover { background: #2e2e4e }
</style>
