import { defineConfig } from 'vitest/config'
import { svelte } from '@sveltejs/vite-plugin-svelte'

// The backend serves the API at the root (/search, /entry, ...). In dev we proxy /api -> backend
// and strip the prefix; in production nginx does the same. So the app always calls /api/*.
const BACKEND = process.env.KANZI_BACKEND ?? 'http://127.0.0.1:8080'

export default defineConfig({
  plugins: [svelte()],
  server: {
    proxy: {
      '/api': { target: BACKEND, changeOrigin: true, rewrite: (p) => p.replace(/^\/api/, '') },
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    include: ['src/**/*.test.ts'],
  },
})
