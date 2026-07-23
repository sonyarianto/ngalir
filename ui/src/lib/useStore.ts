import { useState, useEffect } from 'react'
import { store } from './store'

export function useStore<T>(selector: (s: typeof store) => T): T {
  const [, force] = useState(0)

  useEffect(() => store.subscribe(() => force(n => n + 1)), [])

  return selector(store)
}
