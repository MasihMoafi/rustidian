// Reuse the selected icon family's existing SVG Studio normalization and tracer.
import fs from 'node:fs/promises';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';

const modulePath = process.env.SVG_STUDIO_MODULE;
const skillDir = process.env.LOGO_SKILL_DIR;
if (!modulePath || !skillDir) throw new Error('Set SVG_STUDIO_MODULE and LOGO_SKILL_DIR to the existing installations.');
const sharp = createRequire(modulePath)('sharp');
const { vectorizeImage } = await import(pathToFileURL(modulePath).href);
const { assertVector } = await import(pathToFileURL(path.join(skillDir, 'scripts/vector-contract.mjs')).href);
const root = path.dirname(fileURLToPath(import.meta.url));
const original = path.join(root, 'graph-toolbar-original.png');
const source = path.join(root, 'graph-toolbar-source.png');
const { data, info } = await sharp(original).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
let left = info.width, right = -1, top = info.height, bottom = -1;
for (let y = 0; y < info.height; y++) for (let x = 0; x < info.width; x++) {
  const offset = (y * info.width + x) * 4;
  const coverage = Math.round((255 - (data[offset] + data[offset + 1] + data[offset + 2]) / 3) * data[offset + 3] / 255);
  data[offset] = data[offset + 1] = data[offset + 2] = 0;
  data[offset + 3] = coverage > 16 ? coverage : 0;
  if (coverage >= 128) { left = Math.min(left, x); right = Math.max(right, x); top = Math.min(top, y); bottom = Math.max(bottom, y); }
}
if (right < left) throw new Error('No graph foreground found');
left = Math.max(0, left - 4); top = Math.max(0, top - 4);
right = Math.min(info.width - 1, right + 4); bottom = Math.min(info.height - 1, bottom + 4);
await sharp(data, { raw: info }).extract({ left, top, width: right - left + 1, height: bottom - top + 1 })
  .resize(648, 648, { fit: 'contain', background: { r: 0, g: 0, b: 0, alpha: 0 } })
  .extend({ top: 60, bottom: 60, left: 60, right: 60, background: { r: 0, g: 0, b: 0, alpha: 0 } })
  .png().toFile(source);
const raw = await vectorizeImage(source, 60);
await fs.writeFile(path.join(root, 'graph-toolbar-studio-raw.svg'), raw);
const traced = raw.replaceAll('currentColor', '#000000')
  .replace(/viewBox="[^"]+"/, 'viewBox="10 10 768 768"')
  .replace('width="100%"', 'width="768"').replace('height="100%"', 'height="768"');
const native = traced.replace(/fill="#[0-9a-fA-F]{6}"/g, 'fill="#ffffff"');
assertVector(traced); assertVector(native);
await fs.writeFile(path.join(root, 'graph-toolbar-traced.svg'), traced);
await fs.writeFile(path.resolve(root, '../../graph.svg'), native);
const hash = value => createHash('sha256').update(value).digest('hex');
const paths = svg => [...svg.matchAll(/<path\b[^>]*\bd="([^"]+)"/g)].map(match => match[1]);
if (JSON.stringify(paths(raw)) !== JSON.stringify(paths(native))) throw new Error('Tracing geometry changed');
const provenance = {
  generator: 'Built-in image_gen.imagegen; backend model not independently identified',
  original: 'graph-toolbar-original.png', prompt: 'graph-toolbar-prompt.json',
  selectedRationale: 'One requested local candidate: four hollow circular note nodes and three links remain distinct at toolbar size.',
  selectionAuthority: 'Agent-selected local draft; no claim of user approval or native app verification.',
  preprocessing: 'Existing build-atlas-icons.mjs monochrome pipeline: remove white canvas by luminance coverage, discard coverage<=16, trim empty margins with4px allowance, proportional648px fit plus60px transparent padding. No contours redrawn.',
  originalSize: [info.width, info.height], trim: { left, top, right, bottom },
  preparedRaster: { size: 768, fit: 648, margin: 60 },
  engine: modulePath, engineSha256: hash(await fs.readFile(modulePath)),
  tracing: { method: 'SVG Studio vectorizeImage (monochrome Potrace)', thresholdPercent: 60, studioPaddingCrop: 10 },
  geometryAdjustments: 'Crop SVG Studio10px padding through viewBox only; no path coordinates, holes or contours changed.',
  nativeAdjustment: 'Explicit fill black changed to white for existing egui native tinting; paths and viewBox unchanged.',
  raw: 'graph-toolbar-studio-raw.svg', traced: 'graph-toolbar-traced.svg', native: '../../graph.svg',
  hashes: { original: hash(await fs.readFile(original)), prepared: hash(await fs.readFile(source)), raw: hash(raw), traced: hash(traced), native: hash(native), pathData: hash(JSON.stringify(paths(native))) },
  pathCount: paths(native).length, nativeBytes: Buffer.byteLength(native),
  reproduction: 'SVG_STUDIO_MODULE=/home/masih/Desktop/p/svg-wt-multicolour/mcp-server/dist/vectorize.js LOGO_SKILL_DIR=/home/masih/Desktop/p/skills/logo node assets/icons/artwork/graph-toolbar/graph-toolbar-reproduce.mjs',
};
await fs.writeFile(path.join(root, 'graph-toolbar-provenance.json'), JSON.stringify(provenance, null, 2) + '\n');
console.log(JSON.stringify({ paths: provenance.pathCount, nativeBytes: provenance.nativeBytes, nativeSha256: provenance.hashes.native }));
