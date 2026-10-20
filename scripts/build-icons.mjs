import { mkdir, writeFile, readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { resolve, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { assertVector } from '../../skills/logo/scripts/vector-contract.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const modulePath = process.env.SVG_STUDIO_MODULE;
if (!modulePath) throw new Error('Set SVG_STUDIO_MODULE to the existing multicolour SVG Studio dist/vectorize.js.');
const { vectorizeMulticolourImage } = await import(pathToFileURL(modulePath));
if (!vectorizeMulticolourImage) throw new Error('SVG Studio must provide multicolour tracing.');
const require = createRequire(modulePath);
const sharp = require('sharp');
const out = resolve(root, 'assets/icons');
const artwork = resolve(out, 'artwork');
const names = ['files', 'search', 'new-note', 'new-tab', 'sidebar-left', 'sidebar-right', 'back', 'forward', 'reading', 'edit', 'split', 'save', 'commands', 'vault', 'appearance', 'outline', 'more'];
const sheet = await readFile(resolve(artwork, 'candidate-a.png'));
const { width, height } = await sharp(sheet).metadata();
await mkdir(out, { recursive: true });
let previous = {};
try { previous = JSON.parse(await readFile(resolve(out, 'manifest.json'), 'utf8')); }
catch (error) { if (error.code !== 'ENOENT') throw error; }
const overrides = Object.values(previous.additionalArtwork ?? {}).flatMap(artwork => artwork.replaces ?? []);
const records = [];
for (const [index, name] of names.entries()) {
  if (overrides.includes(name)) continue;
  const col = index % 4;
  const row = Math.floor(index / 4);
  const x = Math.round(col * width / 4);
  const y = Math.round(row * height / 4);
  const standalone = { 'new-tab': 'new-tab-original.png', more: 'more-original.png' };
  const original = standalone[name]
    ? await readFile(resolve(artwork, standalone[name]))
    : await sharp(sheet).extract({ left: x, top: y, width: Math.round((col + 1) * width / 4) - x, height: Math.round((row + 1) * height / 4) - y }).png().toBuffer();
  const { data, info } = await sharp(original).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
  let left = info.width, top = info.height, right = 0, bottom = 0;
  for (let py = 0; py < info.height; py++) for (let px = 0; px < info.width; px++) {
    if (data[(py * info.width + px) * 4 + 3] >= 128) {
      left = Math.min(left, px); right = Math.max(right, px);
      top = Math.min(top, py); bottom = Math.max(bottom, py);
    }
  }
  if (right <= left || bottom <= top) throw new Error(`Empty icon: ${name}`);
  // Remove transparent atlas margins only; leave room for antialiased edge pixels.
  left = Math.max(0, left - 4); top = Math.max(0, top - 4);
  right = Math.min(info.width - 1, right + 4); bottom = Math.min(info.height - 1, bottom + 4);
  const source = resolve(artwork, `source-${name}.png`);
  await sharp(original).extract({ left, top, width: right - left + 1, height: bottom - top + 1 })
    .resize(216, 216, { fit: 'contain', background: '#00000000' })
    .extend({ top: 20, bottom: 20, left: 20, right: 20, background: '#00000000' }).png().toFile(source);
  const traced = await vectorizeMulticolourImage(source, { colours: 2, removeBackground: false });
  assertVector(traced);
  const tracedPath = resolve(artwork, `traced-${name}.svg`);
  await writeFile(tracedPath, traced);
  // White is a rendering mask for native egui tinting. Paths/holes/viewBox are untouched.
  const native = traced.replace(/fill="#[0-9a-fA-F]{3,8}"/g, 'fill="#ffffff"');
  assertVector(native);
  await writeFile(resolve(out, `${name}.svg`), native);
  records.push({ name, source: `artwork/source-${name}.png`, traced: `artwork/traced-${name}.svg`, native: `${name}.svg`, paths: (native.match(/<path\b/g) || []).length, bytes: Buffer.byteLength(native) });
}
await writeFile(resolve(out, 'manifest.json'), JSON.stringify({
  ...previous,
  generator: 'Built-in image_gen.imagegen tool; backend model not independently identified',
  selected: 'candidate-a.png; new-tab-original.png replaces the ambiguous folder-shaped new-tab cell; more-original.png is the separately generated note-actions glyph',
  rationale: 'A has the lightest, simplest line treatment and distinct reading/split silhouettes. B is boxier; C is heavier.',
  engine: modulePath,
  options: { colours: 2, removeBackground: false, alphaCutoff: 128 },
  preprocessing: 'Equal atlas cells; trim transparent margins with 4px edge allowance; proportional fit in a 216px square plus 20px transparent padding; no drawn or edited paths.',
  nativeColour: 'Only explicit fill colours normalized to white for native tinting. Geometry is the SVG Studio output.',
  icons: [...records, ...(previous.icons ?? []).filter(icon => overrides.includes(icon.name) || !names.includes(icon.name))],
}, null, 2) + '\n');
console.log(`SVG Studio converted ${records.length} image icons into real SVG paths.`);
