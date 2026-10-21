import fs from 'node:fs/promises';
import path from 'node:path';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import { assertVector } from '../../skills/logo/scripts/vector-contract.mjs';

const modulePath = process.env.SVG_STUDIO_MODULE;
if (!modulePath) throw new Error('Set SVG_STUDIO_MODULE to the existing SVG Studio module.');
const require = createRequire(modulePath);
const sharp = require('sharp');
const { vectorizeMulticolourImage, vectorizeImage } = await import(pathToFileURL(modulePath).href);
const root = path.resolve('assets/icons');
const artwork = path.join(root, 'artwork');
const groups = {
  pin: {key: 'pin', sheet: 'pin', columns: 1, rows: 1, names: ['pin'], scale: 3, monochrome: true},
  'trash': {key: 'trash', sheet: 'trash', columns: 1, rows: 1, names: ['trash'], scale: 3, monochrome: true},
  'window-controls': {key: 'windowControls', sheet: 'window-controls-sheet', columns: 3, rows: 1, names: ['minimize', 'maximize', 'restore'], scale: 3, monochrome: true},
  callouts: {key: 'callouts', sheet: 'callout-sheet', columns: 4, rows: 4, names: ['callout-note', 'callout-abstract', 'callout-info', 'callout-tip', 'callout-todo', 'callout-success', 'callout-question', 'callout-warning', 'callout-failure', 'callout-danger', 'callout-bug', 'callout-example', 'callout-quote'], scale: 3, monochrome: true},
  knowledge: {key: 'knowledge', sheet: 'knowledge-sheet', columns: 3, rows: 2, names: ['graph', 'canvas', 'bookmark', 'daily-note', 'templates', 'recovery'], scale: 3, monochrome: true},
  'code-controls': {key: 'codeControls', sheet: 'code-controls-sheet', columns: 2, rows: 1, names: ['close', 'copy'], scale: 3, monochrome: true},
  disclosure: {key: 'disclosure', sheet: 'disclosure', columns: 1, rows: 1, names: ['disclosure']},
  properties: {key: 'properties', sheet: 'property-sheet', columns: 4, rows: 2, names: ['property-text', 'property-list', 'property-number', 'property-checkbox', 'property-aliases', 'property-tags', 'property-date', 'property-date-time']},
  'files-tab': {key: 'filesTab', sheet: 'files-tab', columns: 1, rows: 1, names: ['files'], replaces: ['files']},
  'file-toolbar': {key: 'fileToolbar', sheet: 'file-toolbar-sheet', columns: 3, rows: 1, names: ['new-folder', 'sort', 'collapse-all']},
};
const group = groups[process.argv[2] ?? 'properties'];
if (!group) throw new Error('Choose an icon group: ' + Object.keys(groups).join(', '));
const original = path.join(artwork, `${group.sheet}-original.png`);
const manifestPath = path.join(root, 'manifest.json');
const manifest = JSON.parse(await fs.readFile(manifestPath, 'utf8'));
const prompt = JSON.parse(await fs.readFile(path.join(artwork, `${group.sheet}-prompt.json`), 'utf8'));
const names = group.names;
const scale = group.scale ?? 1;
const metadata = await sharp(original).metadata();
const cellWidth = Math.floor(metadata.width / group.columns);
const cellHeight = Math.floor(metadata.height / group.rows);
const options = group.monochrome
  ? {mode: 'monochrome', thresholdPercent: 60, cropStudioPadding: 10}
  : { colours: 2, removeBackground: false };
const outputs = [];
for (const [index, name] of names.entries()) {
  const { data, info } = await sharp(original).extract({left: index % group.columns * cellWidth, top: Math.floor(index / group.columns) * cellHeight, width: cellWidth, height: cellHeight}).ensureAlpha().raw().toBuffer({resolveWithObject: true});
  let left = info.width, right = -1, top = info.height, bottom = -1;
  for (let y = 0; y < info.height; y++) for (let x = 0; x < info.width; x++) {
    const offset = (y * info.width + x) * 4;
    const coverage = Math.round((255 - (data[offset] + data[offset + 1] + data[offset + 2]) / 3) * data[offset + 3] / 255);
    data[offset] = data[offset + 1] = data[offset + 2] = 0;
    data[offset + 3] = coverage > 16 ? coverage : 0;
    if (coverage >= 128) { left = Math.min(left, x); right = Math.max(right, x); top = Math.min(top, y); bottom = Math.max(bottom, y); }
  }
  if (right < left) throw new Error(`No foreground artwork in ${name}`);
  left = Math.max(0, left - 4); top = Math.max(0, top - 4);
  right = Math.min(info.width - 1, right + 4); bottom = Math.min(info.height - 1, bottom + 4);
  const source = path.join(artwork, `source-${name}.png`);
  await sharp(data, {raw: info}).extract({left, top, width: right - left + 1, height: bottom - top + 1})
    .resize(216 * scale, 216 * scale, {fit: 'contain', background: {r: 0, g: 0, b: 0, alpha: 0}})
    .extend({top: 20 * scale, bottom: 20 * scale, left: 20 * scale, right: 20 * scale, background: {r: 0, g: 0, b: 0, alpha: 0}}).png().toFile(source);
  let tracedSvg;
  if (group.monochrome) {
    tracedSvg = await vectorizeImage(source, options.thresholdPercent);
    const size = 256 * scale;
    tracedSvg = tracedSvg.replaceAll('currentColor', '#000000')
      .replace(/viewBox="[^"]+"/, 'viewBox="10 10 ' + size + ' ' + size + '"')
      .replace('width="100%"', 'width="' + size + '"')
      .replace('height="100%"', 'height="' + size + '"');
  } else {
    tracedSvg = await vectorizeMulticolourImage(source, options);
  }
  assertVector(tracedSvg);
  const checked = {paths: [...tracedSvg.matchAll(/<path\b/gi)].length};
  const native = tracedSvg.replace(/fill="#[0-9a-fA-F]{6}"/g, 'fill="#ffffff"');
  assertVector(native);
  await fs.writeFile(path.join(artwork, `traced-${name}.svg`), tracedSvg);
  await fs.writeFile(path.join(root, `${name}.svg`), native);
  outputs.push({name, source: `artwork/source-${name}.png`, traced: `artwork/traced-${name}.svg`, native: `${name}.svg`, sourceColours: 'black on transparency after white-background removal', nativeColours: 'white for native UI tinting', colourAdjustment: 'Explicit fill colours only; traced geometry unchanged.', paths: checked.paths, bytes: Buffer.byteLength(native)});
}
manifest.icons = [...manifest.icons.filter(icon => !names.includes(icon.name)), ...outputs];
manifest.additionalArtwork ??= {};
manifest.additionalArtwork[group.key] = {original: `artwork/${group.sheet}-original.png`, prompt: `artwork/${group.sheet}-prompt.json`, generator: 'Built-in image_gen.imagegen; backend model unidentified.', selectedRationale: prompt.selectedRationale ?? manifest.additionalArtwork[group.key]?.selectedRationale, preprocessing: 'Extract exact atlas cells; remove white background by luminance coverage, dropping coverage <=16; retain ink edges; trim with 4px allowance; contain-fit at the declared prepared resolution with transparent margins. No paths redrawn.', preparedRaster: {fit: 216 * scale, margin: 20 * scale}, engine: modulePath, options, replaces: group.replaces ?? []};
await fs.writeFile(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
console.log(`SVG Studio traced ${outputs.length} generated ${group.key} icons as genuine paths.`);
