import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { Label } from '@/components/ui/label'
import { Button } from '@/components/ui/button'
import { store } from '@/lib/store'
import { useStore } from '@/lib/useStore'

export default function PropertyPanel() {
  const selectedIds = useStore(s => s.selectedIds)
  const selectedNoteId = useStore(s => s.selectedNoteId)
  const credentials = useStore(s => s.credentials)

  const node = selectedIds.length > 0 ? store.nodes.find(n => n.id === selectedIds[selectedIds.length - 1]) : undefined
  const note = selectedNoteId ? store.notes.find(n => n.id === selectedNoteId) : undefined
  const manifest = node ? store.skillsMap[node.use] : undefined
  const nodeCredSpecs = manifest?.credentials ?? []

  function updateNode(field: string, value: string) {
    const n = store.nodes.find(n => n.id === node?.id)
    if (!n) return
    if (field === 'id') n.id = value
    else if (field === 'use') n.use = value
    else if (field === 'when') n.when = value || undefined
    else if (field === 'on_error') n.on_error = value || undefined
  }

  function updateWith(text: string) {
    const n = store.nodes.find(n => n.id === node?.id)
    if (!n) return
    try { n.with = JSON.parse(text) } catch { }
  }

  function addInput() {
    const n = store.nodes.find(n => n.id === node?.id)
    if (!n) return
    if (!n.inputs) n.inputs = {}
    const key = `input${Object.keys(n.inputs).length + 1}`
    ;(n.inputs as Record<string, string>)[key] = ''
  }

  function removeInput(key: string) {
    const n = store.nodes.find(n => n.id === node?.id)
    if (!n || !n.inputs) return
    delete (n.inputs as Record<string, string>)[key]
  }

  function updateInputKey(oldKey: string, newKey: string) {
    const n = store.nodes.find(n => n.id === node?.id)
    if (!n || !n.inputs) return
    const val = (n.inputs as Record<string, string>)[oldKey]
    delete (n.inputs as Record<string, string>)[oldKey]
    if (newKey) (n.inputs as Record<string, string>)[newKey] = val
  }

  function updateInputVal(key: string, val: string) {
    const n = store.nodes.find(n => n.id === node?.id)
    if (!n || !n.inputs) return
    ;(n.inputs as Record<string, string>)[key] = val
  }

  function matchingCredentials(specId: string) { return credentials.filter(c => c.credential_spec_id === specId) }

  function selectCredential(specId: string, credentialId: string) {
    const n = store.nodes.find(n => n.id === node?.id)
    if (!n) return
    if (!n.with) n.with = {}
    const spec = nodeCredSpecs.find(s => s.id === specId)
    if (spec && spec.fields.length > 0) (n.with as Record<string, unknown>)[spec.fields[0].key] = `vault://${credentialId}`
  }

  function clearCredential(specId: string) {
    const n = store.nodes.find(n => n.id === node?.id)
    if (!n || !n.with) return
    const spec = nodeCredSpecs.find(s => s.id === specId)
    if (spec && spec.fields.length > 0) delete (n.with as Record<string, unknown>)[spec.fields[0].key]
  }

  function getCredentialRef(specId: string): string {
    if (!node?.with) return ''
    const spec = nodeCredSpecs.find(s => s.id === specId)
    if (!spec || spec.fields.length === 0) return ''
    const val = (node.with as Record<string, unknown>)[spec.fields[0].key]
    return typeof val === 'string' ? val : ''
  }

  return (
    <aside className="w-56 border-l bg-card p-2 overflow-y-auto text-xs space-y-2">
      {note ? (
        <>
          <h3 className="text-xs text-primary uppercase tracking-wider font-semibold">Note Properties</h3>
          <div>
            <Label htmlFor="prop-note-id">id</Label>
            <Input id="prop-note-id" className="h-7 text-xs" value={note.id} disabled />
          </div>
          <div>
            <Label>position</Label>
            <div className="text-[11px] text-muted-foreground">x: {Math.round(note.position.x)}, y: {Math.round(note.position.y)}</div>
          </div>
          <div>
            <Label>size</Label>
            <div className="flex gap-2 items-center">
              <Input className="w-16 h-7 text-xs" type="number" value={note.width}
                onChange={e => store.updateNote(note.id, { width: parseInt(e.target.value) || 200 })} />
              <span className="text-muted-foreground/60">×</span>
              <Input className="w-16 h-7 text-xs" type="number" value={note.height}
                onChange={e => store.updateNote(note.id, { height: parseInt(e.target.value) || 120 })} />
            </div>
          </div>
        </>
      ) : node ? (
        <>
          <h3 className="text-xs text-primary uppercase tracking-wider font-semibold">Properties</h3>
          <div>
            <Label htmlFor="prop-node-id">id</Label>
            <Input id="prop-node-id" className="h-7 text-xs" value={node.id} onChange={e => updateNode('id', e.target.value)} />
          </div>
          <div>
            <Label htmlFor="prop-node-use">use</Label>
            <Input id="prop-node-use" className="h-7 text-xs" value={node.use} onChange={e => updateNode('use', e.target.value)} />
          </div>
          <div>
            <Label htmlFor="prop-node-when">when</Label>
            <Input id="prop-node-when" className="h-7 text-xs" value={node.when ?? ''} placeholder="optional" onChange={e => updateNode('when', e.target.value)} />
          </div>
          <div>
            <Label htmlFor="prop-node-onerror">on_error</Label>
            <Input id="prop-node-onerror" className="h-7 text-xs" value={node.on_error ?? ''} placeholder="optional" onChange={e => updateNode('on_error', e.target.value)} />
          </div>
          <div>
            <Label htmlFor="prop-node-with">with (config)</Label>
            <Textarea id="prop-node-with" className="min-h-[60px] text-xs" value={JSON.stringify(node.with ?? {}, null, 2)} onChange={e => updateWith(e.target.value)} />
          </div>
          <div>
            <Label>inputs</Label>
            {Object.entries(node.inputs ?? {}).map(([k, v]) => (
              <div key={k} className="flex gap-1 items-center mb-0.5">
                <Input className="flex-1 h-6 text-[11px]" value={k} onChange={e => updateInputKey(k, e.target.value)} placeholder="key" />
                <span className="text-[10px] text-muted-foreground/60">←</span>
                <Input className="flex-1 h-6 text-[11px]" value={v} onChange={e => updateInputVal(k, e.target.value)} placeholder="node.output" />
                <Button variant="ghost" size="icon-xs" className="text-destructive shrink-0" onClick={() => removeInput(k)}>x</Button>
              </div>
            ))}
            <Button variant="outline" size="xs" className="w-full mt-1" onClick={addInput}>+ Add input</Button>
          </div>

          {nodeCredSpecs.length > 0 && (
            <div className="border-t pt-2 mt-2">
              <h4 className="text-[10px] text-primary uppercase tracking-wider mb-2">Credentials</h4>
              {nodeCredSpecs.map(spec => (
                <div key={spec.id} className="mb-2">
                  <Label htmlFor={`prop-cred-${spec.id}`}>{spec.label}</Label>
                  <select id={`prop-cred-${spec.id}`}
                    className="flex h-7 w-full rounded-md border border-input bg-background px-2 text-xs text-foreground font-mono"
                    value={getCredentialRef(spec.id) ? getCredentialRef(spec.id).replace('vault://', '') : ''}
                    onChange={e => {
                      const val = e.target.value
                      if (val === '__add_new__') store.navigateTo('credentials')
                      else if (val === '') clearCredential(spec.id)
                      else selectCredential(spec.id, val)
                    }}>
                    <option value="">-- Select --</option>
                    {matchingCredentials(spec.id).map(cred => (
                      <option key={cred.id} value={cred.id}>{cred.label}</option>
                    ))}
                    <option value="__add_new__">+ Add new credential</option>
                  </select>
                  {getCredentialRef(spec.id) && (
                    <div className="text-[10px] text-green-600 mt-0.5">vault://{getCredentialRef(spec.id).replace('vault://', '')}</div>
                  )}
                </div>
              ))}
            </div>
          )}

          {manifest && (
            <div className="border-t pt-2 mt-2">
              <h4 className="text-[10px] text-primary uppercase tracking-wider mb-1">Ports</h4>
              <div className="text-[10px] text-muted-foreground">
                <span className="text-green-600 font-medium">Outputs:</span> {Object.keys(manifest.outputs).join(', ') || 'output'}
              </div>
              <div className="text-[10px] text-muted-foreground">
                <span className="text-primary font-medium">Inputs:</span> {Object.keys(manifest.inputs).join(', ')}
              </div>
            </div>
          )}

          {(node.input || node.output || node.error || node.status) && (
            <div className="border-t pt-2 mt-2">
              <h4 className="text-[10px] text-primary uppercase tracking-wider mb-1">Preview</h4>
              {node.status && <div className="text-[10px] text-muted-foreground mb-1">status: {node.status}</div>}
              {node.error && (
                <div className="mb-1">
                  <span className="text-[10px] text-destructive uppercase">error</span>
                  <pre className="mt-0.5 px-1.5 py-1 bg-muted rounded text-destructive/80 text-[10px] whitespace-pre-wrap font-mono">{node.error}</pre>
                </div>
              )}
              {node.input && (
                <div className="mb-1">
                  <span className="text-[10px] text-amber-600 uppercase">input</span>
                  <pre className="mt-0.5 px-1.5 py-1 bg-muted rounded text-foreground/80 text-[10px] whitespace-pre-wrap font-mono overflow-x-auto">{JSON.stringify(node.input, null, 2)}</pre>
                </div>
              )}
              {node.output && (
                <div className="mb-1">
                  <span className="text-[10px] text-green-600 uppercase">output</span>
                  <pre className="mt-0.5 px-1.5 py-1 bg-muted rounded text-foreground/80 text-[10px] whitespace-pre-wrap font-mono overflow-x-auto">{JSON.stringify(node.output, null, 2)}</pre>
                </div>
              )}
            </div>
          )}
        </>
      ) : selectedIds.length > 1 ? (
        <>
          <h3 className="text-xs text-primary uppercase tracking-wider font-semibold">Multi-selection</h3>
          <p className="text-muted-foreground text-[11px]">{selectedIds.length} nodes selected</p>
          <p className="text-muted-foreground/60 text-[10px] mt-1">Use Ctrl+D to duplicate or Delete to remove</p>
        </>
      ) : (
        <p className="text-muted-foreground/50 italic text-center mt-8">Select a node, note, or wire</p>
      )}
    </aside>
  )
}
