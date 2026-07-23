import { useEffect, useRef, useState } from 'react'
import type { CanvasNote } from '@/lib/types'
import { store } from '@/lib/store'

const COLORS = ['#fff3cd', '#f8d7da', '#d1e7dd', '#cfe2ff', '#e2d5f5', '#fff']

interface Props { note: CanvasNote }

export default function NoteBlock({ note }: Props) {
  const elRef = useRef<HTMLDivElement>(null)
  const [dragging, setDragging] = useState(false)
  const offsetRef = useRef({ x: 0, y: 0 })

  useEffect(() => {
    const el = elRef.current!
    if (!dragging) return
    function onMouseMove(e: MouseEvent) {
      const parent = el.parentElement
      if (!parent) return
      const pr = parent.getBoundingClientRect()
      const x = e.clientX - pr.left - offsetRef.current.x
      const y = e.clientY - pr.top - offsetRef.current.y
      el.style.left = `${x}px`
      el.style.top = `${y}px`
      store.updateNote(note.id, { position: { x, y } })
    }
    function onMouseUp() {
      store.pushUndo()
      setDragging(false)
    }
    window.addEventListener('mousemove', onMouseMove)
    window.addEventListener('mouseup', onMouseUp)
    return () => { window.removeEventListener('mousemove', onMouseMove); window.removeEventListener('mouseup', onMouseUp) }
  }, [dragging, note.id])

  function handleMouseDown(e: React.MouseEvent) {
    e.stopPropagation()
    store.selectNote(note.id)
    setDragging(true)
    const rect = elRef.current?.getBoundingClientRect()
    offsetRef.current = { x: e.clientX - (rect?.left ?? 0), y: e.clientY - (rect?.top ?? 0) }
  }

  return (
    <div
      ref={elRef}
      data-note={note.id}
      className={`absolute rounded-lg shadow-md z-20 select-none ${note.selected ? 'ring-2 ring-primary' : ''} ${dragging ? 'opacity-85' : ''}`}
      style={{ left: note.position.x, top: note.position.y, width: note.width, height: note.height, backgroundColor: note.color }}
      onMouseDown={handleMouseDown}
      role="region"
      aria-label="Sticky note"
      tabIndex={0}
    >
      <div className="flex items-center justify-between px-2 py-1 bg-black/10 rounded-t-lg cursor-move">
        <span className="text-[10px] text-black/50 uppercase tracking-wider">Note</span>
        {note.selected && (
          <div className="flex gap-1">
            {COLORS.map(c => (
              <button key={c}
                className="w-3 h-3 rounded-full border border-black/20 cursor-pointer"
                style={{ backgroundColor: c }}
                onClick={e => { e.stopPropagation(); store.updateNote(note.id, { color: c }) }}
                aria-label={`Color ${c}`}
              />
            ))}
          </div>
        )}
      </div>
      <textarea
        className="w-full h-[calc(100%-24px)] bg-transparent text-xs text-black/80 p-2 resize-none outline-none font-sans"
        defaultValue={note.text}
        onChange={e => store.updateNote(note.id, { text: e.target.value })}
        onClick={e => e.stopPropagation()}
      />
    </div>
  )
}
