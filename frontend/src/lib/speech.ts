// On-device pronunciation via the Web Speech API (speechSynthesis). No data, no assets, no network:
// the voices live in the user's OS. Mandarin → zh-CN, Cantonese → zh-HK, Japanese → ja-JP.
const LANG: Record<string, string> = { zh: 'zh-CN', yue: 'zh-HK', ja: 'ja-JP' }

let voices: SpeechSynthesisVoice[] = []
function refresh() {
  try {
    voices = window.speechSynthesis.getVoices()
  } catch {
    voices = []
  }
}
if (typeof window !== 'undefined' && 'speechSynthesis' in window) {
  refresh()
  // getVoices() is populated asynchronously on most browsers
  window.speechSynthesis.onvoiceschanged = refresh
}

export function canSpeak(): boolean {
  return typeof window !== 'undefined' && 'speechSynthesis' in window
}

/** Speak a Han word/character in the given variety's language. The Han text is read by the OS voice;
 * we don't feed romanisation (the voice does its own grapheme-to-phoneme). */
export function speak(text: string, variety: string): void {
  if (!canSpeak() || !text) return
  const lang = LANG[variety] ?? 'zh-CN'
  const synth = window.speechSynthesis
  synth.cancel() // stop anything mid-utterance so taps feel responsive
  const u = new SpeechSynthesisUtterance(text)
  u.lang = lang
  if (!voices.length) refresh()
  const base = lang.split('-')[0]
  const v = voices.find((x) => x.lang === lang) ?? voices.find((x) => x.lang.replace('_', '-').startsWith(base))
  if (v) u.voice = v
  u.rate = 0.9
  synth.speak(u)
}
