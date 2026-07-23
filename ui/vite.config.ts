import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [
    tailwindcss(),
    svelte({
      onwarn: (warning, handler) => {
        if (warning.code.startsWith('a11y_')) return
        handler(warning)
      },
    }),
  ],
  build: {
    target: 'esnext',
  },
})
