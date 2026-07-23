import { useEffect, lazy, Suspense } from 'react'
import { Toaster, toast } from 'sonner'
import { store } from './lib/store'
import Toolbar from './components/Toolbar'
import NodePalette from './components/NodePalette'
import FlowCanvas from './components/FlowCanvas'
import PropertyPanel from './components/PropertyPanel'
import { useStore } from './lib/useStore'

const CredentialsPage = lazy(() => import('./components/CredentialsPage'))
const HistoryPage = lazy(() => import('./components/HistoryPage'))

export default function App() {
  const page = useStore(s => s.currentPage)

  useEffect(() => {
    store.loadSample()
    store.fetchSkills()
    store.fetchCredentials()
  }, [])

  useEffect(() => {
    const params = new URLSearchParams(window.location.search)
    const success = params.get('oauth_success')
    const error = params.get('oauth_error')
    if (success) {
      toast.success(`OAuth credential created (${success})`)
      store.fetchCredentials()
      store.navigateTo('credentials')
      window.history.replaceState({}, '', '/')
    } else if (error) {
      toast.error(`OAuth failed: ${error}`)
      window.history.replaceState({}, '', '/')
    }
  }, [])

  if (page === 'credentials') return <Suspense fallback={<div className="h-screen flex items-center justify-center text-muted-foreground">Loading...</div>}><CredentialsPage /></Suspense>
  if (page === 'history') return <Suspense fallback={<div className="h-screen flex items-center justify-center text-muted-foreground">Loading...</div>}><HistoryPage /></Suspense>

  return (
    <div className="h-screen flex flex-col bg-background text-foreground">
      <Toaster richColors position="top-right" />
      <Toolbar />
      <div className="flex flex-1 overflow-hidden">
        <NodePalette />
        <FlowCanvas />
        <PropertyPanel />
      </div>
    </div>
  )
}
