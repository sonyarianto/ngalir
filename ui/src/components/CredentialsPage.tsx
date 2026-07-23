import { useState, useEffect } from 'react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Badge } from '@/components/ui/badge'
import { Card } from '@/components/ui/card'
import {
  Dialog,
  DialogPortal,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog'
import { store } from '@/lib/store'
import { useStore } from '@/lib/useStore'
import type { CredentialSpec } from '@/lib/types'

export default function CredentialsPage() {
  const credentials = useStore(s => s.credentials)
  const credentialSpecs = useStore(s => s.credentialSpecs)

  const [showModal, setShowModal] = useState(false)
  const [selectedSpecId, setSelectedSpecId] = useState('')
  const [newLabel, setNewLabel] = useState('')
  const [formValues, setFormValues] = useState<Record<string, string>>({})
  const [testResult, setTestResult] = useState<{ id: string; ok: boolean; message: string } | null>(null)
  const [deletingId, setDeletingId] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)

  useEffect(() => { store.fetchCredentials() }, [])

  const selectedSpec = findSpec(selectedSpecId)

  function findSpec(id: string): CredentialSpec | undefined {
    return credentialSpecs.find(s => s.id === id) as CredentialSpec | undefined
  }

  function openAddModal() {
    setSelectedSpecId(''); setNewLabel(''); setFormValues({}); setShowModal(true)
  }

  function selectSpec(id: string) {
    setSelectedSpecId(id)
    const spec = findSpec(id)
    if (spec) {
      const fv: Record<string, string> = {}
      for (const f of spec.fields) fv[f.key] = ''
      setFormValues(fv)
    }
  }

  async function handleCreate() {
    if (!selectedSpec || !newLabel.trim()) return
    setCreating(true)
    const ok = await store.createCredential({
      credential_spec_id: selectedSpecId,
      label: newLabel.trim(),
      auth_type: selectedSpec.auth_type,
      data: { ...formValues },
    })
    setCreating(false)
    if (ok) setShowModal(false)
  }

  async function handleDelete(id: string) {
    if (!confirm('Delete this credential?')) return
    setDeletingId(id)
    await store.deleteCredential(id)
    setDeletingId(null)
  }

  async function handleTest(id: string) {
    setTestResult({ id, ok: false, message: 'Testing...' })
    const result = await store.testCredential(id)
    setTestResult(result ? { id, ...result } : { id, ok: false, message: 'Test failed' })
  }

  function authTypeClass(authType: string): string {
    if (authType === 'oauth2') return 'bg-blue-500'
    if (authType === 'api_key') return 'bg-emerald-600'
    if (authType === 'custom') return 'bg-purple-600'
    return 'bg-muted-foreground'
  }

  return (
    <div className="h-screen flex flex-col bg-background text-foreground">
      <div className="flex items-center justify-between px-4 py-3 border-b bg-card">
        <div className="flex items-center gap-4">
          <Button variant="link" className="text-sm text-muted-foreground p-0" onClick={() => store.navigateTo('editor')}>
            &larr; Back to Editor
          </Button>
          <h1 className="text-lg font-semibold">Credentials</h1>
        </div>
        <Button onClick={openAddModal}>+ Add Credential</Button>
      </div>

      <div className="flex-1 overflow-auto p-4">
        {credentials.length === 0 ? (
          <div className="text-center text-muted-foreground mt-20">
            <p className="text-lg mb-2">No credentials stored</p>
            <p className="text-sm">Click "Add Credential" to store your first credential</p>
          </div>
        ) : (
          <div className="space-y-2">
            {credentials.map(cred => (
              <Card key={cred.id} className="flex items-center justify-between px-4 py-3">
                <div className="flex items-center gap-3 flex-1 min-w-0">
                  <Badge variant="secondary" className={`text-xs font-mono text-white ${authTypeClass(cred.auth_type)}`}>
                    {cred.auth_type}
                  </Badge>
                  <div className="min-w-0">
                    <div className="font-medium truncate">{cred.label}</div>
                    <div className="text-xs text-muted-foreground truncate">
                      {credentialSpecs.find(s => s.id === cred.credential_spec_id)?.label ?? cred.credential_spec_id}
                      <span className="mx-1">&middot;</span>
                      id: {cred.id}
                    </div>
                  </div>
                </div>
                <div className="flex items-center gap-2 shrink-0">
                  {testResult?.id === cred.id && (
                    <span className={`text-xs ${testResult.ok ? 'text-green-600' : 'text-destructive'}`}>
                      {testResult.ok ? 'OK' : testResult.message}
                    </span>
                  )}
                  <Button variant="outline" size="sm" onClick={() => handleTest(cred.id)}>Test</Button>
                  <Button variant="destructive" size="sm" onClick={() => handleDelete(cred.id)} disabled={deletingId === cred.id}>
                    {deletingId === cred.id ? '...' : 'Delete'}
                  </Button>
                </div>
              </Card>
            ))}
          </div>
        )}
      </div>

      <Dialog open={showModal} onOpenChange={o => { if (!o) setShowModal(false) }}>
        <DialogPortal>
          <DialogContent className="sm:max-w-[500px]">
            <DialogHeader>
              <DialogTitle>Add Credential</DialogTitle>
            </DialogHeader>
            <div className="space-y-4 py-2">
              <div className="space-y-1">
                <Label htmlFor="cred-service-type">Service Type</Label>
                <select id="cred-service-type"
                  className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm shadow-sm"
                  value={selectedSpecId}
                  onChange={e => selectSpec(e.target.value)}>
                  <option value="">-- Select --</option>
                  {credentialSpecs.map(spec => (
                    <option key={spec.id} value={spec.id}>{spec.label} ({spec.manifest.name})</option>
                  ))}
                </select>
              </div>

              {selectedSpecId && (
                <>
                  <div className="space-y-1">
                    <Label htmlFor="cred-label">Label</Label>
                    <Input id="cred-label" value={newLabel} onChange={e => setNewLabel(e.target.value)} placeholder="e.g. My Google Sheets SA" />
                  </div>

                  {selectedSpec?.fields.map(field => (
                    <div key={field.key} className="space-y-1">
                      <Label htmlFor={`cred-field-${field.key}`}>
                        {field.label}
                        {field.required && <span className="text-destructive">*</span>}
                      </Label>
                      {field.input_type === 'textarea' ? (
                        <Textarea id={`cred-field-${field.key}`} value={formValues[field.key] ?? ''}
                          onChange={e => setFormValues(prev => ({ ...prev, [field.key]: e.target.value }))} placeholder={field.label} />
                      ) : (
                        <Input id={`cred-field-${field.key}`}
                          type={field.input_type === 'password' ? 'password' : 'text'}
                          value={formValues[field.key] ?? ''}
                          onChange={e => setFormValues(prev => ({ ...prev, [field.key]: e.target.value }))}
                          placeholder={field.label} />
                      )}
                    </div>
                  ))}

                  {selectedSpec?.auth_type === 'oauth2' && (
                    <div className="pt-2">
                      <Button className="w-full" onClick={() => window.location.href = `/api/oauth/${selectedSpecId}/authorize`}>
                        Connect with OAuth
                      </Button>
                      <p className="text-xs text-muted-foreground mt-1">Redirects to authorize with the service</p>
                    </div>
                  )}
                </>
              )}
            </div>
            <DialogFooter>
              <Button variant="outline" onClick={() => setShowModal(false)}>Cancel</Button>
              <Button onClick={handleCreate} disabled={creating || !selectedSpecId || !newLabel.trim()}>
                {creating ? 'Creating...' : 'Create'}
              </Button>
            </DialogFooter>
          </DialogContent>
        </DialogPortal>
      </Dialog>
    </div>
  )
}
