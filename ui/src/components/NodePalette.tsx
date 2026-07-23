import { Button } from '@/components/ui/button'
import { CATEGORIES } from '@/lib/types'
import { store } from '@/lib/store'
import { useStore } from '@/lib/useStore'

function nodeLabel(type: string): string {
  const m = store.skillsMap[type]
  if (m?.name) return m.name
  return type.charAt(0).toUpperCase() + type.slice(1).replace(/-/g, ' ')
}

export default function NodePalette() {
  const skillsMap = useStore(s => s.skillsMap)

  function addToCanvas(type: string) {
    store.addNode(type, { x: 200 + Math.random() * 100, y: 150 + Math.random() * 100 })
  }

  function nodeDesc(type: string): string | undefined {
    return skillsMap[type]?.description
  }

  return (
    <aside className="w-40 border-r bg-card p-2 overflow-y-auto flex flex-col gap-1">
      <h3 className="text-xs text-primary uppercase tracking-wider font-semibold mb-2">Nodes</h3>
      {CATEGORIES.map(cat => (
        <div key={cat.name}>
          <h4 className="text-[10px] text-muted-foreground uppercase mt-2 mb-1 tracking-wider">{cat.name}</h4>
          {cat.nodes.map(type => (
            <Button
              key={type}
              variant="outline"
              size="xs"
              title={nodeDesc(type)}
              className="w-full justify-start text-xs font-mono text-muted-foreground mb-0.5"
              onClick={() => addToCanvas(type)}
            >
              {nodeLabel(type)}
            </Button>
          ))}
        </div>
      ))}
    </aside>
  )
}
