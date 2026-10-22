// Compare generated pixels and traced geometry with the app's existing tints.
import fs from 'node:fs/promises';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const modulePath = process.env.SVG_STUDIO_MODULE;
if (!modulePath) throw new Error('Set SVG_STUDIO_MODULE to the existing SVG Studio engine.');
const sharp = createRequire(modulePath)('sharp');
const root = path.dirname(fileURLToPath(import.meta.url));
const renders = path.join(root, 'renders');
await fs.mkdir(renders, { recursive: true });
const original = await fs.readFile(path.join(root, 'graph-toolbar-source.png'));
const native = await fs.readFile(path.resolve(root, '../../graph.svg'), 'utf8');
const stats = [], layers = [], labels = [];
const width = 2220, height = 830;
labels.push(`<rect width="${width}" height="${height}" fill="#e6e8eb"/><text x="24" y="33" font-family="DejaVu Sans" font-size="22" fill="#202736">Graph toolbar — generated source / SVG Studio trace · native tints · 18 / 24 / 48 / 128 / 512 px</text>`);
for (const [index, [theme, background, ink]] of [['light', '#f7f6f2', '#465354'], ['dark', '#171b1d', '#b6c0c0']].entries()) {
  const x = 20 + index * 1100;
  labels.push(`<rect x="${x}" y="60" width="1080" height="750" rx="8" fill="${background}"/><text x="${x + 20}" y="88" font-family="DejaVu Sans" font-size="17" fill="${ink}">${theme.toUpperCase()} · SOURCE / SVG · 512 px</text>`);
  const source = await sharp({ create: { width: 768, height: 768, channels: 4, background: ink } }).composite([{ input: original, blend: 'dest-in' }]).png().toBuffer();
  for (const size of [18, 24, 48, 128, 512]) {
    const vector = native.replaceAll('#ffffff', ink).replace(/\bwidth="[^"]+"/, `width="${size}"`).replace(/\bheight="[^"]+"/, `height="${size}"`);
    const sourceAlpha = await sharp(source).resize(size, size).png().toBuffer();
    const a = await sharp(sourceAlpha).flatten({ background }).removeAlpha().raw().toBuffer();
    const b = await sharp(Buffer.from(vector)).resize(size, size).flatten({ background }).removeAlpha().raw().toBuffer();
    const src = await sharp(a, { raw: { width: size, height: size, channels: 3 } }).png().toBuffer();
    const dst = await sharp(b, { raw: { width: size, height: size, channels: 3 } }).png().toBuffer();
    const bg = [1, 3, 5].map(offset => parseInt(background.slice(offset, offset + 2), 16));
    let total = 0, foregroundError = 0, foregroundPixels = 0;
    for (let p = 0; p < a.length; p += 3) {
      let error = 0, foreground = false;
      for (let c = 0; c < 3; c++) { error += Math.abs(a[p + c] - b[p + c]); foreground ||= Math.abs(a[p + c] - bg[c]) > 8 || Math.abs(b[p + c] - bg[c]) > 8; }
      total += error;
      if (foreground) { foregroundPixels++; foregroundError += error; }
    }
    stats.push({ theme, size, meanDifference: total / a.length / 255, foregroundMeanDifference: foregroundError / (foregroundPixels * 3 * 255), foregroundPixels });
    const left = x + (size === 512 ? 20 : ({ 18: 20, 24: 130, 48: 270, 128: 500 }[size]));
    const top = size === 512 ? 105 : 642 + Math.floor((128 - size) / 2);
    layers.push({ input: src, left, top }, { input: dst, left: left + size + 10, top });
    if (size !== 512) labels.push(`<text x="${left}" y="794" font-family="DejaVu Sans" font-size="14" fill="${ink}">${size}px</text>`);
    await fs.writeFile(path.join(renders, `graph-toolbar-native-${theme}-${size}-source.png`), src);
    await fs.writeFile(path.join(renders, `graph-toolbar-native-${theme}-${size}-svg.png`), dst);
  }
}
await sharp(Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}">${labels.join('')}</svg>`)).composite(layers).png().toFile(path.join(root, 'graph-toolbar-native-proof.png'));
await fs.writeFile(path.join(root, 'graph-toolbar-native-proof.json'), JSON.stringify(stats, null, 2) + '\n');

// A compact sheet keeps the intended 18px glyph at its actual display size.
const small = [], smallLabels = [];
for (const [row, [theme, background, ink]] of [['light', '#f7f6f2', '#465354'], ['dark', '#171b1d', '#b6c0c0']].entries()) {
  const y = row * 120;
  smallLabels.push(`<rect y="${y}" width="680" height="120" fill="${background}"/><text x="14" y="${y + 22}" font-family="DejaVu Sans" font-size="13" fill="${ink}">${theme} · graph source / SVG at18 /24 /48px · existing Files / Bookmark at18px</text>`);
  let x = 18;
  for (const size of [18, 24, 48]) {
    for (const kind of ['source', 'svg']) {
      small.push({ input: await fs.readFile(path.join(renders, `graph-toolbar-native-${theme}-${size}-${kind}.png`)), left: x, top: y + 72 - Math.floor(size / 2) });
      x += size + 12;
    }
    x += 20;
  }
  for (const name of ['files', 'bookmark']) {
    const svg = (await fs.readFile(path.resolve(root, `../../${name}.svg`), 'utf8')).replaceAll('#ffffff', ink);
    small.push({ input: await sharp(Buffer.from(svg)).resize(18, 18).png().toBuffer(), left: x, top: y + 63 });
    x += 42;
  }
}
await sharp(Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg" width="680" height="240">${smallLabels.join('')}</svg>`)).composite(small).png().toFile(path.join(root, 'graph-toolbar-18px-proof.png'));
console.log(JSON.stringify({ conditions: stats.length, maximumForegroundMeanDifference: Math.max(...stats.map(item => item.foregroundMeanDifference)), native18pxProof: 'graph-toolbar-18px-proof.png' }));
