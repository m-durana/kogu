// Pronunciation, per displayed reading. The old version fed the Han text to one OS voice, so a
// multi-reading character only ever spoke its default reading and Cantonese (zh-HK, rarely installed)
// was silent. Now each reading is voiced from its OWN romanisation:
//   • Mandarin  — per-syllable mp3 clips keyed by numbered pinyin (jsDelivr, CORS *).
//   • Cantonese — per-syllable mp3 clips keyed by jyutping (jyutping.org, CORS *).
//   • Japanese  — Web Speech fed the KANA reading, so the shown yomi is what's spoken.
// Clips are cached by the service worker (cache-first), so repeat taps and offline both work. If a
// clip is missing or the network is down we fall back to the OS voice on the Han text.

const ZH_BASE = 'https://cdn.jsdelivr.net/gh/davinfifield/mp3-chinese-pinyin-sound@master/mp3/'
const YUE_BASE = 'https://jyutping.org/audio/'
const SYNTH_LANG: Record<string, string> = { zh: 'zh-CN', yue: 'zh-HK', ja: 'ja-JP' }

let voices: SpeechSynthesisVoice[] = []
function haveSynth(): boolean {
  return typeof window !== 'undefined' && 'speechSynthesis' in window
}
function refresh() {
  try {
    voices = window.speechSynthesis.getVoices()
  } catch {
    voices = []
  }
}
if (haveSynth()) {
  refresh()
  window.speechSynthesis.onvoiceschanged = refresh
}

export function canSpeak(): boolean {
  // Clips work in any browser (just need fetch + Audio); ja needs speechSynthesis but degrades to the
  // clip path's silence rather than erroring. Offer the control whenever we're in a browser.
  return typeof window !== 'undefined' && typeof Audio !== 'undefined'
}

// numbered pinyin syllable → davinfifield filename: lowercase, ü (stored CC-CEDICT-style as "u:") → "uu",
// tone digit kept (neutral = 5). null if the token isn't a tone-marked syllable (stray latin/number).
export function zhFile(tok: string): string | null {
  const t = tok
    .toLowerCase()
    .replace(/u:/g, 'uu')
    .replace(/ü/g, 'uu')
    .replace(/v/g, 'uu')
  return /^[a-z]+[1-5]$/.test(t) ? t : null
}
// jyutping syllable → jyutping.org filename: lowercase, trailing tone 1-6. null if not a syllable.
export function yueFile(tok: string): string | null {
  const t = tok.toLowerCase()
  return /^[a-z]+[1-6]$/.test(t) ? t : null
}

// A single monotonic token guards playback: every new speak() bumps it, so any in-flight fetch loop or
// queued clip from an earlier tap bails the moment a newer tap starts.
let token = 0
// playback uses the Web Audio API so syllable clips can be SCHEDULED back-to-back (even slightly
// overlapped) on one timeline — that removes the audible gap the old <Audio>-per-clip approach left
// between syllables (each <Audio> only started after the previous fired 'ended', plus the clips'
// own silent padding). We keep references to the scheduled sources so a new tap can stop them.
let audioCtx: AudioContext | null = null
let liveSources: AudioBufferSourceNode[] = []

function getCtx(): AudioContext | null {
  if (typeof window === 'undefined') return null
  const AC = window.AudioContext || (window as any).webkitAudioContext
  if (!AC) return null
  if (!audioCtx) {
    try {
      audioCtx = new AC()
    } catch {
      return null
    }
  }
  return audioCtx
}

function stopAll() {
  token++
  for (const s of liveSources) {
    try {
      s.stop()
    } catch {}
  }
  liveSources = []
  if (haveSynth()) {
    try {
      window.speechSynthesis.cancel()
    } catch {}
  }
}

async function fetchBuffer(url: string, ac: AudioContext): Promise<AudioBuffer | null> {
  try {
    const res = await fetch(url)
    const type = res.headers.get('content-type') || ''
    // jyutping.org serves a 200 text/html SPA page for a missing syllable — accept only real audio.
    if (!res.ok || !type.startsWith('audio')) return null
    return await ac.decodeAudioData(await res.arrayBuffer())
  } catch {
    return null
  }
}

// per-syllable clips are recorded slowly with silent padding; a small speed-up plus a slight overlap
// between consecutive syllables makes a multi-syllable word flow as one utterance at a natural pace.
const CLIP_RATE = 1.1
const CLIP_OVERLAP = 0.07 // seconds each syllable starts before the previous one ends (trims the gap)

async function playClips(urls: string[], my: number): Promise<number> {
  const ac = getCtx()
  if (!ac) return 0
  if (ac.state === 'suspended') {
    try {
      await ac.resume()
    } catch {}
  }
  // fetch + decode every syllable IN PARALLEL, then schedule them on the audio clock back-to-back.
  const buffers = await Promise.all(urls.map((u) => fetchBuffer(u, ac)))
  if (my !== token) return 0
  let played = 0
  let when = ac.currentTime + 0.03
  for (const buf of buffers) {
    if (my !== token) break
    if (!buf) continue // a missing syllable: skip it, keep voicing the rest of the word
    const src = ac.createBufferSource()
    src.buffer = buf
    src.playbackRate.value = CLIP_RATE
    src.connect(ac.destination)
    src.start(when)
    liveSources.push(src)
    played++
    // advance the clock by this clip's (rate-adjusted) length minus the overlap, so the next syllable
    // begins just before this one fades out — no silent gap between syllables.
    when += Math.max(0.05, buf.duration / CLIP_RATE - CLIP_OVERLAP)
  }
  return played
}

// Web Speech on the Han text — the offline / clip-missing fallback, and the path used when we have no
// usable romanisation. rate 0.9 to match the clip pace.
function speakSynth(text: string, variety: string): void {
  if (!haveSynth() || !text) return
  const lang = SYNTH_LANG[variety] ?? 'zh-CN'
  const u = new SpeechSynthesisUtterance(text)
  u.lang = lang
  if (!voices.length) refresh()
  const base = lang.split('-')[0]
  const v = voices.find((x) => x.lang === lang) ?? voices.find((x) => x.lang.replace('_', '-').startsWith(base))
  if (v) u.voice = v
  u.rate = 0.9
  window.speechSynthesis.speak(u)
}

const KANA = /^[぀-ヿー\s]+$/
function speakJa(reading: string | null | undefined, fallback?: string): void {
  // feed the kana yomi so the SHOWN reading is spoken; if the reading isn't plain kana (furigana
  // markup, romaji), fall back to the word itself.
  const text = reading && KANA.test(reading) ? reading.replace(/\s+/g, '') : fallback || reading || ''
  speakSynth(text, 'ja')
}

/** Speak one reading in its variety's voice. `reading` is the numbered pinyin / jyutping / kana shown
 * on the row; `fallbackText` is the Han word, used when clips are unavailable. */
export function speakReading(reading: string | null | undefined, variety: string, fallbackText?: string): void {
  if (!canSpeak()) return
  stopAll()
  const my = token

  if (variety === 'ja') {
    speakJa(reading, fallbackText)
    return
  }

  const base = variety === 'yue' ? YUE_BASE : ZH_BASE
  const toFile = variety === 'yue' ? yueFile : zhFile
  const files = (reading || '')
    .split(/\s+/)
    .map(toFile)
    .filter((x): x is string => !!x)

  if (!files.length) {
    // no romanisation to key clips on (e.g. a bare letter/number reading) — use the OS voice.
    speakSynth(fallbackText || '', variety)
    return
  }

  void playClips(files.map((f) => base + f + '.mp3'), my).then((played) => {
    // every syllable was missing (and we weren't superseded) → fall back to the OS voice.
    if (played === 0 && my === token) speakSynth(fallbackText || '', variety)
  })
}

/** Back-compat shim: speak the Han text in its variety (no specific reading). Prefer speakReading. */
export function speak(text: string, variety: string): void {
  speakReading(null, variety, text)
}
