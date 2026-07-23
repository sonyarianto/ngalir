import { Button } from '@/components/ui/button'
import { Moon, Sun } from 'lucide-react'
import { useTheme } from 'next-themes'
import { store } from '@/lib/store'
import { useStore } from '@/lib/useStore'

function ThemeToggle() {
  const { setTheme, resolvedTheme } = useTheme()
  const isDark = resolvedTheme === 'dark'
  return (
    <Button variant="ghost" size="icon-sm" onClick={() => setTheme(isDark ? 'light' : 'dark')}>
      {isDark ? <Sun className="size-3.5" /> : <Moon className="size-3.5" />}
    </Button>
  )
}

export default function Toolbar() {
  const flowName = useStore(s => s.flowName)
  const running = useStore(s => s.running)
  const stepMode = useStore(s => s.stepMode)
  const stepReady = useStore(s => s.stepReady)
  const savedFlows = useStore(s => s.savedFlows)
  const showFlowList = useStore(s => s.showFlowList)

  function handleLoad() {
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = '.yaml,.json,.yml'
    input.onchange = async () => {
      const file = input.files?.[0]
      if (!file) return
      store.filename = file.name
      const text = await file.text()
      if (file.name.endsWith('.yaml') || file.name.endsWith('.yml')) store.importYaml(text)
      else store.importFlow(text)
    }
    input.click()
  }

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

  function handleAddNote() {
    store.addNote({ x: 200 + Math.random() * 100, y: 150 + Math.random() * 100 })
  }

  return (
    <>
      <header className="flex items-center gap-2 px-4 py-2 border-b bg-card h-12">
        <span className="font-bold text-lg text-primary">Ngalir</span>
        <span className="text-sm text-muted-foreground">{flowName}</span>
        <Button variant="outline" size="sm" className="border-primary/40 text-primary" onClick={() => store.navigateTo('credentials')}>
          Credentials
        </Button>
        <Button variant="outline" size="sm" className="border-primary/40 text-primary" onClick={() => { store.fetchHistory(); store.navigateTo('history') }}>
          History
        </Button>
        <div className="flex-1" />
        <Button variant="outline" size="sm" onClick={() => store.listFlows()}>Flows</Button>
        <Button variant="outline" size="sm" onClick={handleLoad}>Open</Button>
        <div className="relative group">
          <Button variant="outline" size="sm">Export</Button>
          <div className="absolute top-full right-0 mt-1 hidden group-hover:block z-50 bg-card border rounded-lg shadow-lg min-w-[120px]">
            <Button variant="ghost" size="sm" className="w-full justify-start rounded-none" onClick={handleDownloadYaml}>Export YAML</Button>
            <Button variant="ghost" size="sm" className="w-full justify-start rounded-none" onClick={handleDownloadJson}>Export JSON</Button>
          </div>
        </div>
        <Button variant="outline" size="sm" onClick={() => store.saveFlow()}>Save</Button>
        <Button variant="outline" size="sm" onClick={() => store.loadSample()}>Sample</Button>
        <Button variant="outline" size="sm" onClick={handleAddNote}>Note</Button>
        <Button variant="outline" size="sm" onClick={() => store.selectAll()}>Select All</Button>
        <Button variant="outline" size="sm" onClick={() => store.duplicateSelected()}>Duplicate</Button>
        <Button variant="outline" size="sm" onClick={() => store.autoLayout()}>Layout</Button>
        <span className="w-px h-4 bg-border" />
        <ThemeToggle />
        <Button variant="outline" size="icon-sm" onClick={() => store.undo()}>↩</Button>
        <Button variant="outline" size="icon-sm" onClick={() => store.redo()}>↪</Button>
        {stepReady ? (
          <>
            <Button size="sm" className="bg-green-600 hover:bg-green-700 text-white" onClick={() => store.stepContinue()}>Continue</Button>
            <Button size="sm" className="bg-destructive hover:bg-destructive/90 text-destructive-foreground" onClick={() => store.stepStop()}>Stop</Button>
          </>
        ) : !running ? (
          <>
            <Button size="sm" onClick={() => store.runFlow()}>Run</Button>
            <Button variant="outline" size="sm" className="border-primary/40 text-primary" onClick={() => store.runStepFlow()}>Step</Button>
          </>
        ) : (
          <span className="text-sm text-amber-500">Running{stepMode ? '…' : ''}</span>
        )}
      </header>

      {showFlowList && (
        <div className="absolute top-12 left-2 z-50 bg-card border rounded-lg shadow-xl w-64 max-h-80 overflow-y-auto">
          <div className="flex items-center justify-between px-3 py-2 border-b">
            <span className="text-xs text-primary uppercase tracking-wider font-semibold">Saved Flows</span>
            <Button variant="ghost" size="icon-xs" className="text-muted-foreground" onClick={() => { store.showFlowList = false }}>✕</Button>
          </div>
          {savedFlows.length === 0 ? (
            <p className="px-3 py-4 text-xs text-muted-foreground/50 text-center">No saved flows</p>
          ) : (
            savedFlows.map(f => (
              <div key={f.name} className="flex items-center gap-2 px-3 py-2 border-b border-muted hover:bg-muted/50">
                <Button variant="ghost" size="sm" className="flex-1 h-auto text-xs justify-start text-left" onClick={() => store.loadFlow(f.name)}>
                  {f.name}
                </Button>
                <Button variant="ghost" size="icon-xs" className="text-destructive shrink-0" onClick={() => store.deleteFlow(f.name)}>x</Button>
              </div>
            ))
          )}
        </div>
      )}
    </>
  )
}
