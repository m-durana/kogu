// Focus management for the dialog cards (settings, install help, term/bound popups): the a11y
// audit found none of them moved focus in, trapped Tab, or restored focus on close, so keyboard
// users kept tabbing the page behind an "aria-modal" dialog.
//
// Svelte action: `<div use:dialogFocus role="dialog" tabindex="-1">`. On mount it remembers the
// opener, focuses the card, and keeps Tab cycling inside; on destroy it hands focus back.
const FOCUSABLE = 'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'

export function dialogFocus(node: HTMLElement) {
  const opener = document.activeElement as HTMLElement | null
  node.focus()
  const onKey = (e: KeyboardEvent) => {
    if (e.key !== 'Tab') return
    const els = Array.from(node.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
      (el) => el.offsetParent !== null,
    )
    if (!els.length) {
      e.preventDefault()
      return
    }
    const first = els[0]
    const last = els[els.length - 1]
    const active = document.activeElement
    if (e.shiftKey && (active === first || active === node)) {
      e.preventDefault()
      last.focus()
    } else if (!e.shiftKey && active === last) {
      e.preventDefault()
      first.focus()
    }
  }
  node.addEventListener('keydown', onKey)
  return {
    destroy() {
      node.removeEventListener('keydown', onKey)
      opener?.focus?.()
    },
  }
}
