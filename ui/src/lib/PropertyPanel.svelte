<script lang="ts">
  import { getStore } from './store.svelte.js'

  const store = getStore()

  let node = $derived(store.nodes.find((n) => n.id === store.selectedIds[store.selectedIds.length - 1]))
  let note = $derived(store.notes.find((n) => n.id === store.selectedNoteId))

  function updateWhen(e: Event) {
    const n = store.nodes.find((n) => n.id === node?.id)
    if (!n) return
    n.when = (e.target as HTMLInputElement).value || undefined
  }

  function updateOnError(e: Event) {
    const n = store.nodes.find((n) => n.id === node?.id)
    if (!n) return
    n.on_error = (e.target as HTMLInputElement).value || undefined
  }

  function updateId(e: Event) {
    const n = store.nodes.find((n) => n.id === node?.id)
    if (!n) return
    n.id = (e.target as HTMLInputElement).value
  }

  function updateUse(e: Event) {
    const n = store.nodes.find((n) => n.id === node?.id)
    if (!n) return
    n.use = (e.target as HTMLInputElement).value
  }

  function updateWith(e: Event) {
    const n = store.nodes.find((n) => n.id === node?.id)
    if (!n) return
    try { n.with = JSON.parse((e.target as HTMLTextAreaElement).value) }
    catch {}
  }

  function addInput() {
    const n = store.nodes.find((n) => n.id === node?.id)
    if (!n) return
    if (!n.inputs) n.inputs = {}
    const key = `input${Object.keys(n.inputs).length + 1}`
    ;(n.inputs as Record<string, string>)[key] = ''
  }

  function removeInput(key: string) {
    const n = store.nodes.find((n) => n.id === node?.id)
    if (!n || !n.inputs) return
    delete (n.inputs as Record<string, string>)[key]
  }

  function updateInputKey(oldKey: string, newKey: string) {
    const n = store.nodes.find((n) => n.id === node?.id)
    if (!n || !n.inputs) return
    const val = (n.inputs as Record<string, string>)[oldKey]
    delete (n.inputs as Record<string, string>)[oldKey]
    if (newKey) (n.inputs as Record<string, string>)[newKey] = val
  }

  function updateInputVal(key: string, val: string) {
    const n = store.nodes.find((n) => n.id === node?.id)
    if (!n || !n.inputs) return
    ;(n.inputs as Record<string, string>)[key] = val
  }

  let manifest = $derived(node ? store.skillsMap[node.use] : null)
  let credentialSpecs = $derived(manifest?.credentials ?? [])

  function matchingCredentials(specId: string) {
    return store.credentials.filter(c => c.credential_spec_id === specId)
  }

  function selectCredential(specId: string, credentialId: string) {
    const n = store.nodes.find((n) => n.id === node?.id)
    if (!n) return
    if (!n.with) n.with = {}
    // Find which field this credential spec maps to
    const spec = credentialSpecs.find(s => s.id === specId)
    if (spec && spec.fields.length > 0) {
      // Set the first field to vault:// reference
      ;(n.with as Record<string, unknown>)[spec.fields[0].key] = `vault://${credentialId}`
    }
  }

  function clearCredential(specId: string) {
    const n = store.nodes.find((n) => n.id === node?.id)
    if (!n || !n.with) return
    const spec = credentialSpecs.find(s => s.id === specId)
    if (spec && spec.fields.length > 0) {
      delete (n.with as Record<string, unknown>)[spec.fields[0].key]
    }
  }

  function getCredentialRef(specId: string): string {
    if (!node?.with) return ''
    const spec = credentialSpecs.find(s => s.id === specId)
    if (!spec || spec.fields.length === 0) return ''
    const val = (node.with as Record<string, unknown>)[spec.fields[0].key]
    return typeof val === 'string' ? val : ''
  }

  function goToCredentials() {
    store.navigateTo('credentials')
  }
</script>

<aside class="w-56 bg-[#16162a] border-l border-[#333] p-2 overflow-y-auto text-xs">
  {#if note}
    <h3 class="text-xs text-[#7c3aed] uppercase tracking-wider mb-2">Note Properties</h3>
    <div class="mb-2">
      <label class="block text-[10px] text-[#888] uppercase mb-0.5">id</label>
      <input class="w-full px-1.5 py-1 border border-[#333] rounded bg-[#0f0f23] text-[#e0e0e0] text-xs font-mono box-border" value={note.id} disabled />
    </div>
    <div class="mb-2">
      <label class="block text-[10px] text-[#888] uppercase mb-0.5">position</label>
      <div class="text-[11px] text-[#999]">x: {Math.round(note.position.x)}, y: {Math.round(note.position.y)}</div>
    </div>
    <div class="mb-2">
      <label class="block text-[10px] text-[#888] uppercase mb-0.5">size</label>
      <div class="flex gap-2 items-center">
        <input class="w-16 px-1.5 py-0.5 border border-[#333] rounded bg-[#0f0f23] text-[#e0e0e0] text-xs font-mono box-border" type="number" value={note.width} oninput={(e) => store.updateNote(note.id, { width: parseInt(e.currentTarget.value) || 200 })} />
        <span class="text-[#555]">×</span>
        <input class="w-16 px-1.5 py-0.5 border border-[#333] rounded bg-[#0f0f23] text-[#e0e0e0] text-xs font-mono box-border" type="number" value={note.height} oninput={(e) => store.updateNote(note.id, { height: parseInt(e.currentTarget.value) || 120 })} />
      </div>
    </div>
  {:else if node}
    <h3 class="text-xs text-[#7c3aed] uppercase tracking-wider mb-2">Properties</h3>
    <div class="mb-2">
      <label class="block text-[10px] text-[#888] uppercase mb-0.5">id</label>
      <input class="w-full px-1.5 py-1 border border-[#333] rounded bg-[#0f0f23] text-[#e0e0e0] text-xs font-mono box-border" value={node.id} oninput={updateId} />
    </div>
    <div class="mb-2">
      <label class="block text-[10px] text-[#888] uppercase mb-0.5">use</label>
      <input class="w-full px-1.5 py-1 border border-[#333] rounded bg-[#0f0f23] text-[#e0e0e0] text-xs font-mono box-border" value={node.use} oninput={updateUse} />
    </div>
    <div class="mb-2">
      <label class="block text-[10px] text-[#888] uppercase mb-0.5">when</label>
      <input class="w-full px-1.5 py-1 border border-[#333] rounded bg-[#0f0f23] text-[#e0e0e0] text-xs font-mono box-border" value={node.when ?? ''} placeholder="optional" oninput={updateWhen} />
    </div>
    <div class="mb-2">
      <label class="block text-[10px] text-[#888] uppercase mb-0.5">on_error</label>
      <input class="w-full px-1.5 py-1 border border-[#333] rounded bg-[#0f0f23] text-[#e0e0e0] text-xs font-mono box-border" value={node.on_error ?? ''} placeholder="optional" oninput={updateOnError} />
    </div>
    <div class="mb-2">
      <label class="block text-[10px] text-[#888] uppercase mb-0.5">with (config)</label>
      <textarea class="w-full px-1.5 py-1 border border-[#333] rounded bg-[#0f0f23] text-[#e0e0e0] text-xs font-mono box-border resize-y" value={JSON.stringify(node.with ?? {}, null, 2)} oninput={updateWith} rows="4"></textarea>
    </div>
    <div class="mb-2">
      <label class="block text-[10px] text-[#888] uppercase mb-0.5">inputs</label>
      {#each Object.entries(node.inputs ?? {}) as [k, v]}
        <div class="flex gap-1 items-center mb-0.5">
          <input class="flex-1 min-w-0 px-1.5 py-0.5 border border-[#333] rounded bg-[#0f0f23] text-[#e0e0e0] text-[11px] font-mono box-border" value={k} oninput={(e) => updateInputKey(k, e.currentTarget.value)} placeholder="key" />
          <span class="text-[10px] text-[#555]">←</span>
          <input class="flex-1 min-w-0 px-1.5 py-0.5 border border-[#333] rounded bg-[#0f0f23] text-[#e0e0e0] text-[11px] font-mono box-border" value={v} oninput={(e) => updateInputVal(k, e.currentTarget.value)} placeholder="node.output" />
          <button class="px-1 border border-[#333] rounded bg-[#1e1e36] text-[#ef4444] text-[11px] cursor-pointer hover:bg-[#2e2e4e]" onclick={() => removeInput(k)}>x</button>
        </div>
      {/each}
      <button class="w-full mt-1 px-1 py-0.5 border border-[#333] rounded bg-[#1e1e36] text-[#ccc] text-[11px] cursor-pointer hover:bg-[#2e2e4e]" onclick={addInput}>+ Add input</button>
    </div>
    {#if credentialSpecs.length > 0}
      <div class="border-t border-[#333] pt-2 mt-2">
        <h4 class="text-[10px] text-[#7c3aed] uppercase tracking-wider mb-2">Credentials</h4>
        {#each credentialSpecs as spec}
          <div class="mb-2">
            <label class="block text-[10px] text-[#888] uppercase mb-0.5">{spec.label}</label>
            <div class="flex gap-1 items-center">
              <select
                class="flex-1 min-w-0 px-1.5 py-1 border border-[#333] rounded bg-[#0f0f23] text-[#e0e0e0] text-[11px] font-mono box-border"
                onchange={(e) => {
                  const val = (e.target as HTMLSelectElement).value
                  if (val === '__add_new__') {
                    goToCredentials()
                  } else if (val === '') {
                    clearCredential(spec.id)
                  } else {
                    selectCredential(spec.id, val)
                  }
                }}
              >
                <option value="">-- Select --</option>
                {#each matchingCredentials(spec.id) as cred}
                  <option value={cred.id} selected={getCredentialRef(spec.id) === `vault://${cred.id}`}>
                    {cred.label}
                  </option>
                {/each}
                <option value="__add_new__" disabled={false}>+ Add new credential</option>
              </select>
            </div>
            {#if getCredentialRef(spec.id)}
              <div class="text-[10px] text-green-500 mt-0.5">
                vault://{getCredentialRef(spec.id).replace('vault://', '')}
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    {#if store.skillsMap[node.use]}
      <div class="border-t border-[#333] pt-2 mt-2">
        <h4 class="text-[10px] text-[#7c3aed] uppercase tracking-wider mb-1">Ports (from manifest)</h4>
        <div class="text-[10px] text-[#888]">
          <span class="text-green-400">Outputs:</span> {Object.keys(store.skillsMap[node.use].outputs).join(', ') || 'output'}
        </div>
        <div class="text-[10px] text-[#888]">
          <span class="text-[#7c3aed]">Inputs:</span> {Object.keys(store.skillsMap[node.use].inputs).join(', ')}
        </div>
      </div>
    {/if}
    {#if node.input || node.output || node.error || node.status}
      <div class="border-t border-[#333] pt-2 mt-2">
        <h4 class="text-[10px] text-[#7c3aed] uppercase tracking-wider mb-1">Preview</h4>
        {#if node.status}
          <div class="text-[10px] text-[#888] mb-1">status: {node.status}</div>
        {/if}
        {#if node.error}
          <div class="mb-1">
            <span class="text-[10px] text-red-400 uppercase">error</span>
            <pre class="mt-0.5 px-1.5 py-1 bg-[#1a1a30] rounded text-red-300 text-[10px] whitespace-pre-wrap font-mono">{node.error}</pre>
          </div>
        {/if}
        {#if node.input}
          <div class="mb-1">
            <span class="text-[10px] text-yellow-400 uppercase">input</span>
            <pre class="mt-0.5 px-1.5 py-1 bg-[#1a1a30] rounded text-[#ccc] text-[10px] whitespace-pre-wrap font-mono overflow-x-auto">{JSON.stringify(node.input, null, 2)}</pre>
          </div>
        {/if}
        {#if node.output}
          <div class="mb-1">
            <span class="text-[10px] text-green-400 uppercase">output</span>
            <pre class="mt-0.5 px-1.5 py-1 bg-[#1a1a30] rounded text-[#ccc] text-[10px] whitespace-pre-wrap font-mono overflow-x-auto">{JSON.stringify(node.output, null, 2)}</pre>
          </div>
        {/if}
      </div>
    {/if}
  {:else if store.selectedIds.length > 1}
    <h3 class="text-xs text-[#7c3aed] uppercase tracking-wider mb-2">Multi-selection</h3>
    <p class="text-[#999] text-[11px]">{store.selectedIds.length} nodes selected</p>
    <p class="text-[#666] text-[10px] mt-1">Use Ctrl+D to duplicate or Delete to remove</p>
  {:else}
    <p class="text-[#555] italic text-center mt-8">Select a node, note, or wire</p>
  {/if}
</aside>
