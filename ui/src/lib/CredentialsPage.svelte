<script lang="ts">
  import { getStore } from './store.svelte.js'
  import type { CredentialSpec } from './types'

  const store = getStore()
  let showModal = $state(false)
  let selectedSpecId = $state('')
  let newLabel = $state('')
  let formValues = $state<Record<string, string>>({})
  let testResult = $state<{ id: string; ok: boolean; message: string } | null>(null)
  let deletingId = $state<string | null>(null)
  let creating = $state(false)

  $effect(() => {
    if (store.currentPage === 'credentials') {
      store.fetchCredentials()
    }
  })

  function openAddModal() {
    selectedSpecId = ''
    newLabel = ''
    formValues = {}
    showModal = true
  }

  function closeModal() {
    showModal = false
  }

  function selectSpec(id: string) {
    selectedSpecId = id
    formValues = {}
    const spec = findSpec(id)
    if (spec) {
      for (const f of spec.fields) {
        formValues[f.key] = ''
      }
    }
  }

  function findSpec(id: string): CredentialSpec | undefined {
    for (const s of store.credentialSpecs) {
      if (s.id === id) return s as unknown as CredentialSpec
    }
    return undefined
  }

  function findSpecByManifest(): CredentialSpec | undefined {
    return findSpec(selectedSpecId)
  }

  async function handleCreate() {
    const spec = findSpecByManifest()
    if (!spec || !newLabel.trim()) return
    creating = true
    const data: Record<string, unknown> = {
      credential_spec_id: selectedSpecId,
      label: newLabel.trim(),
      auth_type: spec.auth_type,
      data: { ...formValues },
    }
    const ok = await store.createCredential(data)
    creating = false
    if (ok) closeModal()
  }

  async function handleDelete(id: string) {
    if (!confirm('Delete this credential?')) return
    deletingId = id
    await store.deleteCredential(id)
    deletingId = null
  }

  async function handleTest(id: string) {
    testResult = { id, ok: false, message: 'Testing...' }
    const result = await store.testCredential(id)
    testResult = result ? { id, ...result } : { id, ok: false, message: 'Test failed' }
  }

  function isOAuth(specId: string): boolean {
    const spec = findSpec(specId)
    return spec?.auth_type === 'oauth2'
  }

  function specLabel(specId: string): string {
    const spec = findSpec(specId)
    if (spec) return spec.label
    return specId
  }

  function fieldLabel(specId: string, key: string): string {
    const spec = findSpec(specId)
    const field = spec?.fields.find(f => f.key === key)
    return field?.label || key
  }

  function fieldType(specId: string, key: string): string {
    const spec = findSpec(specId)
    const field = spec?.fields.find(f => f.key === key)
    if (field?.input_type === 'password') return 'password'
    if (field?.input_type === 'textarea') return 'textarea'
    return 'text'
  }

  function fieldRequired(specId: string, key: string): boolean {
    const spec = findSpec(specId)
    const field = spec?.fields.find(f => f.key === key)
    return field?.required ?? false
  }

  function authTypeBadge(authType: string): string {
    if (authType === 'oauth2') return 'bg-blue-600'
    if (authType === 'api_key') return 'bg-green-700'
    if (authType === 'custom') return 'bg-purple-700'
    return 'bg-gray-600'
  }
</script>

<div class="h-screen flex flex-col bg-[#0f0f23] text-[#e0e0e0]">
  <!-- Header -->
  <div class="flex items-center justify-between px-4 py-3 border-b border-[#2a2a4a] bg-[#1a1a3e]">
    <div class="flex items-center gap-4">
      <button
        onclick={() => store.navigateTo('editor')}
        class="text-sm text-[#8888cc] hover:text-white transition-colors"
      >
        &larr; Back to Editor
      </button>
      <h1 class="text-lg font-semibold">Credentials</h1>
    </div>
    <button
      onclick={openAddModal}
      class="px-4 py-1.5 text-sm rounded bg-[#4a4ae6] hover:bg-[#5a5af0] text-white transition-colors"
    >
      + Add Credential
    </button>
  </div>

  <!-- Credential List -->
  <div class="flex-1 overflow-auto p-4">
    {#if store.credentials.length === 0}
      <div class="text-center text-[#666688] mt-20">
        <p class="text-lg mb-2">No credentials stored</p>
        <p class="text-sm">Click "Add Credential" to store your first credential</p>
      </div>
    {:else}
      <div class="space-y-2">
        {#each store.credentials as cred (cred.id)}
          <div class="flex items-center justify-between bg-[#1a1a3e] border border-[#2a2a4a] rounded-lg px-4 py-3">
            <div class="flex items-center gap-3 flex-1">
              <span class="text-xs font-mono px-2 py-0.5 rounded text-white {authTypeBadge(cred.auth_type)}">
                {cred.auth_type}
              </span>
              <div>
                <div class="font-medium">{cred.label}</div>
                <div class="text-xs text-[#666688]">
                  {specLabel(cred.credential_spec_id)}
                  <span class="mx-1">&middot;</span>
                  id: {cred.id}
                </div>
              </div>
            </div>
            <div class="flex items-center gap-2">
              {#if testResult?.id === cred.id}
                <span class="text-xs {testResult.ok ? 'text-green-400' : 'text-red-400'}">
                  {testResult.ok ? 'OK' : testResult.message}
                </span>
              {/if}
              <button
                onclick={() => handleTest(cred.id)}
                class="px-3 py-1 text-xs rounded bg-[#2a2a4a] hover:bg-[#3a3a5a] text-[#aaaacc] transition-colors"
              >
                Test
              </button>
              <button
                onclick={() => handleDelete(cred.id)}
                disabled={deletingId === cred.id}
                class="px-3 py-1 text-xs rounded bg-[#4a1a1a] hover:bg-[#5a2a2a] text-[#cc8888] transition-colors disabled:opacity-50"
              >
                {deletingId === cred.id ? '...' : 'Delete'}
              </button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<!-- Add Credential Modal -->
{#if showModal}
  <div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onclick={closeModal}>
    <div class="bg-[#1a1a3e] border border-[#2a2a4a] rounded-lg w-[500px] max-h-[80vh] overflow-auto" onclick={(e) => e.stopPropagation()}>
      <div class="px-6 py-4 border-b border-[#2a2a4a]">
        <h2 class="text-lg font-semibold">Add Credential</h2>
      </div>
      <div class="px-6 py-4 space-y-4">
        <!-- Select Spec -->
        <div>
          <label class="block text-sm text-[#8888cc] mb-1">Service Type</label>
          <select
            bind:value={selectedSpecId}
            onchange={(e) => selectSpec((e.target as HTMLSelectElement).value)}
            class="w-full bg-[#0f0f23] border border-[#2a2a4a] rounded px-3 py-2 text-sm text-white"
          >
            <option value="">-- Select --</option>
            {#each store.credentialSpecs as spec}
              <option value={spec.id}>{spec.label} ({spec.manifest.name})</option>
            {/each}
          </select>
        </div>

        {#if selectedSpecId}
          <!-- Label -->
          <div>
            <label class="block text-sm text-[#8888cc] mb-1">Label</label>
            <input
              bind:value={newLabel}
              placeholder="e.g. My Google Sheets SA"
              class="w-full bg-[#0f0f23] border border-[#2a2a4a] rounded px-3 py-2 text-sm text-white"
            />
          </div>

          <!-- Dynamic Fields -->
          {#each findSpecByManifest()?.fields ?? [] as field}
            <div>
              <label class="block text-sm text-[#8888cc] mb-1">
                {field.label}
                {#if field.required}<span class="text-red-400">*</span>{/if}
              </label>
              {#if field.input_type === 'textarea'}
                <textarea
                  bind:value={formValues[field.key]}
                  placeholder={field.label}
                  rows="4"
                  class="w-full bg-[#0f0f23] border border-[#2a2a4a] rounded px-3 py-2 text-sm text-white font-mono"
                ></textarea>
              {:else}
                <input
                  type={field.input_type === 'password' ? 'password' : 'text'}
                  bind:value={formValues[field.key]}
                  placeholder={field.label}
                  class="w-full bg-[#0f0f23] border border-[#2a2a4a] rounded px-3 py-2 text-sm text-white"
                />
              {/if}
            </div>
          {/each}

          <!-- OAuth Button -->
          {#if isOAuth(selectedSpecId)}
            <div class="pt-2">
              <button
                onclick={() => window.location.href = `/api/oauth/${selectedSpecId}/authorize`}
                class="w-full px-4 py-2 text-sm rounded bg-[#4a4ae6] hover:bg-[#5a5af0] text-white transition-colors"
              >
                Connect with OAuth
              </button>
              <p class="text-xs text-[#666688] mt-1">Redirects to authorize with the service</p>
            </div>
          {/if}
        {/if}
      </div>
      <div class="px-6 py-4 border-t border-[#2a2a4a] flex justify-end gap-2">
        <button
          onclick={closeModal}
          class="px-4 py-1.5 text-sm rounded bg-[#2a2a4a] hover:bg-[#3a3a5a] text-[#aaaacc] transition-colors"
        >
          Cancel
        </button>
        <button
          onclick={handleCreate}
          disabled={creating || !selectedSpecId || !newLabel.trim()}
          class="px-4 py-1.5 text-sm rounded bg-[#4a4ae6] hover:bg-[#5a5af0] text-white transition-colors disabled:opacity-50"
        >
          {creating ? 'Creating...' : 'Create'}
        </button>
      </div>
    </div>
  </div>
{/if}
