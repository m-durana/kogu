import { defineConfig } from 'vitest/config'
import { svelte } from '@sveltejs/vite-plugin-svelte'

// Config for the deep-run consistency sweep ONLY (sweep/*.sweep.ts); the normal
// `pnpm test` config deliberately excludes it. See sweep/consistency.sweep.ts.
export default defineConfig({
  plugins: [svelte()],
  // without this vitest resolves the SERVER build of svelte and mount() throws
  resolve: { conditions: ['browser'] },
  test: {
    environment: 'jsdom',
    globals: true,
    include: ['sweep/**/*.sweep.ts'],
    testTimeout: 1000 * 60 * 60 * 3,
    hookTimeout: 1000 * 60 * 10,
  },
})
