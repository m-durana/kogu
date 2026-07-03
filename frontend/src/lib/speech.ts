// Pronunciation, per displayed reading. The old version fed the Han text to one OS voice, so a
// multi-reading character only ever spoke its default reading and Cantonese (zh-HK, rarely installed)
// was silent. Now each reading is voiced from its OWN romanisation:
//   • Mandarin : per-syllable mp3 clips keyed by numbered pinyin (jsDelivr, CORS *).
//   • Cantonese: per-syllable mp3 clips keyed by jyutping (jyutping.org, CORS *).
//   • Japanese : Web Speech fed the KANA reading, so the shown yomi is what's spoken.
// Clips are cached by the service worker (cache-first), so repeat taps and offline both work. If a
// clip is missing or the network is down we fall back to the OS voice on the Han text.

// Pronunciation clips are proxied through our own backend (/api/clip/...) so they are SAME-ORIGIN :
// the upstream CDNs (jsDelivr for Mandarin, jyutping.org for Cantonese) are unreachable for some users
// (mainland China blocks jsDelivr; some iOS/PWA setups fail the cross-origin fetch), which silently
// broke zh/yue audio while the same-origin Japanese synth kept working.
const ZH_BASE = '/api/clip/zh/'
const YUE_BASE = '/api/clip/yue/'
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
// Playback uses plain <Audio> elements (reliable: they play on the tap's user gesture on every
// browser). To kill the gap between syllables we (a) prefetch ALL clips in parallel up front and
// (b) start the NEXT syllable a hair before the current one ends (overlap), so a word runs together.
let active: HTMLAudioElement[] = []

function stopAll() {
  token++
  for (const a of active) {
    try {
      a.pause()
    } catch {}
  }
  active = []
  if (haveSynth()) {
    try {
      window.speechSynthesis.cancel()
    } catch {}
  }
}

async function fetchClip(url: string): Promise<string | null> {
  try {
    const res = await fetch(url)
    const type = res.headers.get('content-type') || ''
    // jyutping.org serves a 200 text/html SPA page for a missing syllable: accept only real audio.
    if (!res.ok || !type.startsWith('audio')) return null
    return URL.createObjectURL(await res.blob())
  } catch {
    return null
  }
}

// per-syllable clips are recorded slowly with silent padding; a small speed-up + starting the next
// syllable just before the current ends makes a multi-syllable word flow at a natural pace.
const CLIP_RATE = 1.12
const CLIP_OVERLAP = 0.09 // seconds before a clip's end to start the next one (trims the inter-syllable gap)

// Play one clip (object URL). Resolves when it's OVERLAP-seconds from its end (so a caller can start
// the next clip and they run together), but lets the audio play out to its natural end and revokes the
// URL then. `rate` lets per-syllable zh/yue clips speed up while a whole-word ja synth plays at 1.0.
function playOne(obj: string, my: number, rate: number): Promise<void> {
  return new Promise<void>((resolve) => {
    const a = new Audio(obj)
    a.playbackRate = rate
    let advanced = false
    const advance = () => {
      if (!advanced) {
        advanced = true
        resolve()
      }
    }
    a.addEventListener('timeupdate', () => {
      if (a.duration && a.currentTime >= a.duration - CLIP_OVERLAP) advance()
    })
    a.addEventListener('ended', () => {
      URL.revokeObjectURL(obj)
      advance()
    })
    a.addEventListener('error', () => {
      URL.revokeObjectURL(obj)
      advance()
    })
    active.push(a)
    if (my !== token) return advance()
    a.play().catch(advance)
  })
}

async function playClips(urls: string[], my: number): Promise<number> {
  // Kick off ALL fetches in parallel, but play each clip the MOMENT its own fetch resolves: the first
  // syllable starts as soon as it's ready instead of blocking on the slowest clip (that all-or-nothing
  // wait was the multi-second delay before playback began). Remaining clips keep loading during
  // playback, so a multi-syllable word still runs together with no gap.
  const objsP = urls.map(fetchClip)
  const revokeFrom = (i: number) => objsP.slice(i).forEach((p) => p.then((o) => o && URL.revokeObjectURL(o)))
  let played = 0
  for (let i = 0; i < objsP.length; i++) {
    if (my !== token) {
      revokeFrom(i)
      break
    }
    const obj = await objsP[i]
    if (my !== token) {
      if (obj) URL.revokeObjectURL(obj)
      revokeFrom(i + 1)
      break
    }
    if (!obj) continue // a missing syllable: skip it, keep voicing the rest of the word
    played++
    await playOne(obj, my, CLIP_RATE)
  }
  return played
}

// Web Speech on the Han text: the offline / clip-missing fallback, and the path used when we have no
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
// Japanese: the browser voice can't honour pitch accent, so synthesize the kana with the local
// OpenJTalk service (/api/tts/ja) which FORCES the stored Kanjium downstep. The returned mp3 plays
// through the same <Audio> + service-worker-cache path as the zh/yue clips. Any failure (offline,
// service down, non-kana reading) falls back to the OS voice, exactly like the clip path.
async function speakJa(reading: string | null | undefined, fallback: string | undefined, accent: string | null | undefined, my: number): Promise<void> {
  const kana = reading && KANA.test(reading) ? reading.replace(/\s+/g, '') : ''
  if (!kana) {
    speakSynth(fallback || reading || '', 'ja')
    return
  }
  const a = accent != null && accent !== '' ? `&accent=${encodeURIComponent(accent)}` : ''
  const obj = await fetchClip(`/api/tts/ja?kana=${encodeURIComponent(kana)}${a}`)
  if (my !== token) {
    if (obj) URL.revokeObjectURL(obj)
    return
  }
  if (!obj) {
    speakSynth(kana, 'ja') // service unavailable → OS voice on the kana
    return
  }
  await playOne(obj, my, 1.0)
}

/** Speak one reading in its variety's voice. `reading` is the numbered pinyin / jyutping / kana shown
 * on the row; `fallbackText` is the Han word, used when clips are unavailable. `accent` is the Japanese
 * Kanjium downstep index (ja only), forwarded to the synth so the pitch accent is correct. */
export function speakReading(
  reading: string | null | undefined,
  variety: string,
  fallbackText?: string,
  accent?: string | null,
): Promise<void> {
  if (!canSpeak()) return Promise.resolve()
  stopAll()
  const my = token

  if (variety === 'ja') {
    return speakJa(reading, fallbackText, accent, my)
  }

  const base = variety === 'yue' ? YUE_BASE : ZH_BASE
  const toFile = variety === 'yue' ? yueFile : zhFile
  const files = (reading || '')
    .split(/\s+/)
    .map(toFile)
    .filter((x): x is string => !!x)

  if (!files.length) {
    // no romanisation to key clips on (e.g. a bare letter/number reading): use the OS voice.
    speakSynth(fallbackText || '', variety)
    return Promise.resolve()
  }

  // resolves when playback finishes (or is superseded): the UI uses this to light the speaker only
  // while it's actually sounding.
  return playClips(files.map((f) => base + f + '.mp3'), my).then((played) => {
    // every syllable was missing (and we weren't superseded) → fall back to the OS voice.
    if (played === 0 && my === token) speakSynth(fallbackText || '', variety)
  })
}

/** Back-compat shim: speak the Han text in its variety (no specific reading). Prefer speakReading. */
export function speak(text: string, variety: string): void {
  speakReading(null, variety, text)
}
