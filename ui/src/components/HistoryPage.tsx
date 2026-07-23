import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { store } from '@/lib/store'
import { useStore } from '@/lib/useStore'

function formatDuration(ms: number | null): string {
  if (ms == null) return '-'
  if (ms < 1000) return `${ms}ms`
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`
  return `${Math.floor(ms / 60000)}m ${Math.floor((ms % 60000) / 1000)}s`
}

function formatTimestamp(iso: string): string {
  if (!iso) return '-'
  return iso.replace('T', ' ').replace('Z', '')
}

function statusBadgeVariant(status: string): 'default' | 'secondary' | 'destructive' | 'outline' {
  if (status === 'completed') return 'default'
  if (status === 'failed') return 'destructive'
  return 'secondary'
}

export default function HistoryPage() {
  const historyRuns = useStore(s => s.historyRuns)
  const historyRunDetail = useStore(s => s.historyRunDetail)
  const [selectedRun, setSelectedRun] = useState<string | null>(null)

  function viewRun(flowId: string) {
    setSelectedRun(flowId)
    store.fetchHistoryRun(flowId)
  }

  function back() {
    setSelectedRun(null)
    store.fetchHistory()
  }

  return (
    <div className="h-screen flex flex-col bg-background text-foreground">
      <header className="flex items-center gap-2 px-4 py-2 border-b bg-card h-12">
        <span className="font-bold text-lg text-primary">Ngalir</span>
        <span className="text-sm text-muted-foreground">Execution History</span>
        <div className="flex-1" />
        <Button variant="outline" size="sm" className="border-primary/40 text-primary" onClick={() => store.navigateTo('editor')}>
          Back to Editor
        </Button>
      </header>

      <div className="flex-1 overflow-y-auto p-6">
        {selectedRun && historyRunDetail ? (
          <>
            <Button variant="link" className="text-sm text-primary p-0 mb-4" onClick={back}>&larr; Back to list</Button>
            <Card className="p-4 mb-6">
              <h2 className="text-lg font-bold mb-2">{historyRunDetail.flow?.flow_name ?? 'Unknown'}</h2>
              <div className="grid grid-cols-4 gap-4 text-sm">
                <div>
                  <span className="text-muted-foreground block">Status</span>
                  <Badge variant={statusBadgeVariant(historyRunDetail.flow?.status ?? '')}>{historyRunDetail.flow?.status}</Badge>
                </div>
                <div>
                  <span className="text-muted-foreground block">Started</span>
                  <p>{historyRunDetail.flow?.started_at ? formatTimestamp(historyRunDetail.flow.started_at) : '-'}</p>
                </div>
                <div>
                  <span className="text-muted-foreground block">Duration</span>
                  <p>{formatDuration(historyRunDetail.flow?.duration_ms ?? null)}</p>
                </div>
                <div>
                  <span className="text-muted-foreground block">Nodes</span>
                  <p>{historyRunDetail.flow?.node_count ?? 0}</p>
                </div>
              </div>
              {historyRunDetail.flow?.error && (
                <div className="mt-2 p-2 bg-destructive/10 rounded text-sm text-destructive">{historyRunDetail.flow.error}</div>
              )}
            </Card>

            <h3 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider mb-2">Nodes</h3>
            <div className="space-y-2">
              {(historyRunDetail.nodes ?? []).map((node, i) => (
                <Card key={i} className="p-3">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium">{node.node_id}</span>
                      <span className="text-xs text-muted-foreground">({node.node_type})</span>
                      <Badge variant={statusBadgeVariant(node.status)} className="text-xs">{node.status}</Badge>
                    </div>
                    <span className="text-xs text-muted-foreground">{formatDuration(node.duration_ms)}</span>
                  </div>
                  {node.error && <div className="mt-1 text-xs text-destructive">{node.error}</div>}
                </Card>
              ))}
            </div>
          </>
        ) : (
          <>
            <h2 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider mb-3">Past Runs</h2>
            {historyRuns.length === 0 ? (
              <p className="text-sm text-muted-foreground/50 text-center mt-12">No execution history yet. Run a flow to see it here.</p>
            ) : (
              <div className="space-y-2">
                {historyRuns.map(run => (
                  <button key={run.flow_id} onClick={() => viewRun(run.flow_id)}
                    className="w-full text-left bg-card border rounded-lg p-3 hover:bg-muted/50 transition-colors cursor-pointer">
                    <div className="flex items-center justify-between">
                      <div>
                        <span className="text-sm font-medium">{run.flow_name}</span>
                        <Badge variant={statusBadgeVariant(run.status)} className="ml-2 text-xs">{run.status}</Badge>
                      </div>
                      <span className="text-xs text-muted-foreground">{formatDuration(run.duration_ms)}</span>
                    </div>
                    <div className="flex items-center gap-4 mt-1 text-xs text-muted-foreground">
                      <span>{formatTimestamp(run.started_at)}</span>
                      <span>{run.node_count} nodes</span>
                    </div>
                  </button>
                ))}
              </div>
            )}
          </>
        )}
      </div>
    </div>
  )
}
