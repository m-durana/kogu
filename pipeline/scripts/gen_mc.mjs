// Generate sources/char_mc.json: character -> [Middle Chinese Baxter reading, ...]
//
// Data: nk2028/tshet-uinh (CC0), the 廣韻 (Guangyun) phonological corpus bundled in the
// `tshet-uinh` npm package, romanized with Baxter's transcription via `tshet-uinh-examples`.
// Baxter (e.g. 馬 = maeX, 母 = muX, 海 = xojX) is the standard ASCII transcription of Middle
// Chinese used in the literature, so the readings are checkable against any reference.
//
// Re-runnable. Disk on / is tight, so install the (~1.3 MB) deps under /mnt:
//   MC_PREFIX=/mnt/HC_Volume_102319212/tmp/mc-work \
//   npm --prefix "$MC_PREFIX" install tshet-uinh@0.15.4 tshet-uinh-examples@latest tshet-uinh-deriver-tools@latest
//   node --experimental-vm-modules pipeline/scripts/gen_mc.mjs   # honours MC_PREFIX / NODE_PATH
//
// Writes pipeline/sources/char_mc.json (~350 KB, ~19.5k characters).
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';

const HERE = path.dirname(new URL(import.meta.url).pathname);
const SOURCES = path.resolve(HERE, '..', 'sources');
const PREFIX = process.env.MC_PREFIX || '/mnt/HC_Volume_102319212/tmp/mc-work';

// resolve the deps from the install prefix (lib/node_modules under an npm --prefix install)
async function load(name) {
  const candidates = [
    path.join(PREFIX, 'node_modules', name),
    path.join(PREFIX, 'lib', 'node_modules', name),
  ];
  for (const base of candidates) {
    if (fs.existsSync(base)) {
      const req = createRequire(path.join(base, 'package.json'));
      const pkg = req('./package.json');
      const entry = pkg.module || pkg.exports?.import || pkg.exports?.['.']?.import || pkg.main || 'index.js';
      const file = path.join(base, typeof entry === 'string' ? entry : entry.default || 'index.js');
      return import(pathToFileURL(file).href);
    }
  }
  throw new Error(`cannot find ${name} under ${PREFIX} (run the npm --prefix install in the header)`);
}

const TshetUinh = (await load('tshet-uinh')).default;
const ex = await load('tshet-uinh-examples');

const out = {};
for (const entry of TshetUinh.資料.廣韻.iter條目()) {
  const ch = entry.字頭;
  if (!ch || !entry.音韻地位) continue;
  const r = ex.baxter()(entry.音韻地位, ch);
  if (!r) continue;
  (out[ch] ||= []);
  if (!out[ch].includes(r)) out[ch].push(r);
}

const dest = path.join(SOURCES, 'char_mc.json');
fs.writeFileSync(dest, JSON.stringify(out));
console.error(`wrote ${Object.keys(out).length} characters -> ${dest}`);
