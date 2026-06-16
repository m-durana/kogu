import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'

const app = mount(App, {
  target: document.getElementById('app')!,
})

// drop the static app-shell (index.html) now that the real UI is mounted, so there's no duplicate header
document.getElementById('shell')?.remove()

// PWA: register the service worker (installability + fast loads). Dev server skips it.
if ('serviceWorker' in navigator && import.meta.env.PROD) {
  window.addEventListener('load', () => {
    navigator.serviceWorker.register('/sw.js').catch(() => {})
  })
}

export default app
