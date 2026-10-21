import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const modulePath = process.env.SVG_STUDIO_MODULE;
if (!modulePath) throw new Error('Set SVG_STUDIO_MODULE to the existing SVG Studio module.');
const sharp = createRequire(modulePath)('sharp');
const iconsDir = resolve(root, 'assets/icons');
const manifest = JSON.parse(await readFile(resolve(iconsDir, 'manifest.json'), 'utf8'));
const selected = process.argv.slice(2);
if (selected.some(name => !/^[a-z0-9-]+$/.test(name) || !manifest.icons.some(icon => icon.name === name))) throw new Error('Choose existing icon names.');
if (selected.length) manifest.icons = manifest.icons.filter(icon => selected.includes(icon.name));
const proofDir = resolve(iconsDir, selected.length ? `proof/selected-${selected.join('-')}` : 'proof');
await mkdir(proofDir, {recursive: true});
// The proof renderer uses 780px per mark; keep each SVG canvas below 32767px.
const sheets = [], measurements = [], logs = [];
for (let start = 0; start < manifest.icons.length; start += 30) {
  const icons = manifest.icons.slice(start, start + 30);
  const name = `source-vs-vector-${sheets.length + 1}`;
  const pairs = icons.flatMap(icon => [resolve(iconsDir, icon.source), resolve(iconsDir, icon.traced)]);
  const proof = spawnSync(process.execPath, [resolve(root, '../skills/logo/scripts/proof.mjs'), resolve(proofDir, `${name}.png`), ...pairs], { env: process.env, encoding: 'utf8' });
  logs.push(proof.stdout + proof.stderr);
  await writeFile(resolve(proofDir, 'trace-proof.log'), logs.join('\n'));
  if (proof.status !== 0) throw new Error(proof.stderr || 'Logo proof failed');
  sheets.push(name);
  measurements.push(...JSON.parse(await readFile(resolve(proofDir, `${name}.json`), 'utf8')));
}
await writeFile(resolve(proofDir, 'source-vs-vector.json'), JSON.stringify(measurements, null, 2) + '\n');
await writeFile(resolve(proofDir, 'source-vs-vector.html'), `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Source and SVG proofs</title><style>body{margin:16px;background:#e6e8eb;font:16px sans-serif}img{display:block;max-width:100%;height:auto}</style><h1>Source and SVG proofs</h1><p>All ${manifest.icons.length} selected artworks, source left and SVG right at 24, 48, 128 and 512 pixels on both themes.</p>${sheets.map((name, index) => `<p><a href="${name}.html">Open proof sheet ${index + 1}</a></p><img src="${name}.png" alt="Source and vector comparison sheet ${index + 1}">`).join('')}</html>\n`);

const tileWidth = 360, tileHeight = 210;
const layers = [];
for (const [index, icon] of manifest.icons.entries()) {
  const left = (index % 4) * tileWidth, top = Math.floor(index / 4) * tileHeight;
  const title = `<svg width="360" height="210"><rect width="360" height="210" fill="#eef0ee"/><rect x="6" y="36" width="348" height="77" rx="5" fill="#f7f6f2"/><rect x="6" y="121" width="348" height="81" rx="5" fill="#171b1d"/><text x="12" y="20" font-size="12" font-family="sans-serif" fill="#222a2b">${icon.name} — source / SVG at 12, 18, 24, 48px</text></svg>`;
  layers.push({ input: Buffer.from(title), left, top });
  const original = await readFile(resolve(iconsDir, icon.source));
  const sourceSize = await sharp(original).metadata();
  const native = await readFile(resolve(iconsDir, icon.native), 'utf8');
  for (const [row, foreground] of ['#465354', '#b6c0c0'].entries()) {
    const tintedSource = await sharp({ create: { width: sourceSize.width, height: sourceSize.height, channels: 4, background: foreground } }).composite([{ input: original, blend: 'dest-in' }]).png().toBuffer();
    const tintedVector = Buffer.from(native.replaceAll('#ffffff', foreground));
    let x = 14;
    for (const size of [12, 18, 24, 48]) {
      const y = row === 0 ? 75 - Math.floor(size / 2) : 161 - Math.floor(size / 2);
      for (const input of [tintedSource, tintedVector]) {
        layers.push({ input: await sharp(input).resize(size, size).png().toBuffer(), left: left + x, top: top + y });
        x += size + 9;
      }
      x += 9;
    }
  }
}
await sharp({ create: { width: tileWidth * 4, height: tileHeight * Math.ceil(manifest.icons.length / 4), channels: 4, background: '#eef0ee' } }).composite(layers).png().toFile(resolve(proofDir, 'native-tints.png'));
console.log('Generated original-vs-vector proof (24/48/128/512px) and native-tint proof (12/18/24/48px) on light/dark surfaces.');
