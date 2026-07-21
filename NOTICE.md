# Data licensing and attribution

The Kogu **code** is MIT-licensed (see `LICENSE`). The **dictionary database**
(`data/kogu.sqlite`, built by `pipeline/`) is a derived work of the sources below. Several of
them are CC BY-SA, so the built database as a whole ships under
**[CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/)**.

If you redistribute the database (or data extracted from it), keep this attribution file with it.

## Bundled data sources

### Unihan (Unicode Character Database)
Character backbone: codepoints, variants, readings (pinyin, jyutping, on/kun), and the
kSpoofingVariant confusables. Unicode licence (Unicode-DFS terms).
<https://www.unicode.org/Public/UCD/latest/ucd/Unihan.zip>

### cjkvi-ids
Ideographic Description Sequences (`ids.txt`): the structural decomposition of each character.
Upstream states `ids.txt` is derived from the [CHISE project](http://www.chise.org/) and follows
CHISE's terms.
<https://github.com/cjkvi/cjkvi-ids>

### CC-CEDICT
Chinese-English dictionary entries (traditional/simplified headwords, pinyin, glosses).
CC BY-SA 4.0. <https://www.mdbg.net/chinese/dictionary?page=cedict>

### CC-Canto and CC-CEDICT Cantonese readings
Cantonese-English entries (jyutping, 粵字) plus Cantonese readings for CC-CEDICT entries
(two files). CC BY-SA. <https://cantonese.org/download.html>

### JMdict and KANJIDIC (via jmdict-simplified)
Japanese-English word entries and kanji data. These files are the property of the
**Electronic Dictionary Research and Development Group (EDRDG)** and are used in conformance
with the Group's licence: <https://www.edrdg.org/edrdg/licence.html> (CC BY-SA 4.0 with the
EDRDG attribution requirements). Fetched as the JSON builds from
<https://github.com/scriptin/jmdict-simplified>.

### Kanjium
Japanese pitch-accent data (`accents.txt`), used for accent display and to drive the TTS
sidecar's downstep. CC BY-SA 4.0. <https://github.com/mifunetoshiro/kanjium>

### OpenCC conversion tables
Simplified/traditional/regional/shinjitai character mappings (STCharacters, TSCharacters,
TWVariants, HKVariants, JPShinjitaiCharacters). Apache-2.0.
<https://github.com/BYVoid/OpenCC>

### tshet-uinh (nk2028)
Middle Chinese (廣韻/Guangyun) phonological data, romanized with Baxter's transcription by
`pipeline/scripts/gen_mc.mjs` into `char_mc.json`. **CC0** (public domain dedication).
<https://github.com/nk2028/tshet-uinh-data>

### Open JTalk (via pyopenjtalk) and the Tohoku HTS voice
Japanese pronunciation audio is synthesized locally by the TTS sidecar (`tts/synth_service.py`)
using **pyopenjtalk** (MIT), which bundles the Open JTalk engine (Modified BSD, Nagoya Institute
of Technology) and the NAIST Japanese dictionary (BSD-style). <https://github.com/r9y9/pyopenjtalk>
The voice is **htsvoice-tohoku-f01** (neutral style), (c) Intelligent Communication Network
(Ito-Nose) Laboratory, Tohoku University, **CC BY 4.0**.
<https://github.com/icn-lab/htsvoice-tohoku-f01>

### Wiktionary (via kaikki.org / wiktextract)
Etymology text, origin badges, phono-semantic component roles (Han compound templates), and
English-pivot translation tables. Dual-licensed **CC BY-SA 3.0 + GFDL**, per Wikimedia terms.
Extracted from the kaikki.org machine-readable dumps: <https://kaikki.org/>
(wiktextract: <https://github.com/tatuylonen/wiktextract>).

### Howell Etymological Dictionary of Han/Chinese Characters
Phono-semantic **character** etymologies, used to gap-fill single characters where Wiktionary has
none. Etymological interpretations © Lawrence J. Howell / Hikaru Morimoto, **MIT-licensed**.
<https://github.com/conscientiousCode/Etymological-Dictionary-of-Han-Chinese-Characters-Database>

### chinese-xinhua (成語 idiom origins)
The classical source (出處) of Chinese four-character idioms, used to gap-fill **word** etymology.
**MIT-licensed**. <https://github.com/pwxcoo/chinese-xinhua>

### mapull/chinese-dictionary (成語 idiom origins)
A larger idiom set with the cited classical source (book + quotation), gap-filling **word**
etymology. **MIT-licensed**. <https://github.com/mapull/chinese-dictionary>

### wordfreq
Primary word-frequency source (Zipf scale, multi-corpus blend). Code Apache-2.0, data
CC BY-SA 4.0. <https://github.com/rspeer/wordfreq>

### FrequencyWords (hermitdave)
Fallback 50k frequency lists (OpenSubtitles-derived) used when wordfreq is unavailable.
Content CC BY-SA 4.0. <https://github.com/hermitdave/FrequencyWords>

### Open Multilingual Wordnet (via the `wn` package)
Cross-language concept links from shared synsets: Japanese Wordnet (`omw-ja`, CC BY) and
Chinese Open Wordnet (`omw-cmn`, wordnet licence). Each wordnet carries its own licence; see
<https://omwn.org/> and <https://github.com/goodmami/wn>.

The GPLv2 classical datasets sometimes bundled with CJKV tooling (Guangyun, Shuowen Jiezi,
Kangxi via cjkvi-dict) are **not** ingested; Middle Chinese data comes from the CC0 tshet-uinh
corpus instead.

## Third-party runtime services (not bundled data)

These are proxied at request time. Nothing from them is stored in the database, but requests
you make through the corresponding features reach these services:

- **Mandarin audio clips**: per-syllable mp3s from
  <https://github.com/davinfifield/mp3-chinese-pinyin-sound> (served via jsDelivr).
- **Cantonese audio clips**: per-syllable mp3s from <https://jyutping.org>.
- **Handwriting recognition**: Google Input Tools handwriting endpoint
  (`inputtools.google.com`), a Google service under Google's terms.
- **Machine translation** (`/mt`): Google Translate's unofficial `gtx` endpoint, with
  <https://mymemory.translated.net> as fallback. Both are third-party services with their own
  terms; neither is affiliated with Kogu.

OCR is **local** (PP-OCRv5 ONNX models run in-process); no image leaves the server.
The Japanese TTS sidecar is **local** (OpenJTalk); no text leaves the server.
