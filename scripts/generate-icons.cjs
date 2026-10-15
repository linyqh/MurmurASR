#!/usr/bin/env node
// Requires sharp (npm install --no-save sharp, or expose it through NODE_PATH).
// Run from any directory: node scripts/generate-icons.cjs
const sharp = require('sharp');
const fs = require('node:fs/promises');
const path = require('node:path');

const root = path.resolve(__dirname, '..');
const source = path.join(root, 'assets/icon/asr-source.png');
const output = path.join(root, 'src-tauri/icons');
const masterSize = 1024;
const tileSize = 832;
const inset = (masterSize - tileSize) / 2;

async function main() {
  await fs.mkdir(output, { recursive: true });
  // Inset past the generated image's irregular rim. Use a geometric alpha mask
  // so rounded corners are encoded in the pixels on BOTH macOS and Windows.
  // Discard the generated alpha inside the tile to avoid translucent white haze.
  const tile = await sharp(source)
    .extract({ left: 101, top: 114, width: 1052, height: 1032 })
    .removeAlpha()
    .resize(tileSize, tileSize, { kernel: 'lanczos3' })
    .png().toBuffer();
  const radius = 215 * tileSize / 1032;
  const mask = Buffer.from(`<svg width="${tileSize}" height="${tileSize}" xmlns="http://www.w3.org/2000/svg"><rect width="${tileSize}" height="${tileSize}" rx="${radius}" fill="white"/></svg>`);
  const roundedTile = await sharp(tile)
    .ensureAlpha()
    .composite([{ input: mask, blend: 'dest-in' }])
    .png().toBuffer();
  const master = await sharp({
    create: { width: masterSize, height: masterSize, channels: 4,
      background: { r: 0, g: 0, b: 0, alpha: 0 } },
  }).composite([{ input: roundedTile, left: inset, top: inset }])
    .png().toBuffer();
  await fs.writeFile(path.join(root, 'assets/icon/app-icon.png'), master);
  await fs.writeFile(path.join(output, 'icon.png'), master);

  const rendered = new Map();
  async function renderPng(size) {
    if (!rendered.has(size)) {
      // Lanczos keeps lettering crisp, but its ringing can introduce detached
      // alpha=1 pixels in the transparent margin. Resample alpha separately
      // with a non-ringing kernel, and zero RGB wherever alpha is zero.
      const rgba = await sharp(master).resize(size, size).raw().toBuffer();
      const alpha = await sharp(master).extractChannel(3)
        .resize(size, size, { kernel: 'linear' }).raw().toBuffer();
      for (let i = 0; i < alpha.length; i++) {
        rgba[i * 4 + 3] = alpha[i];
        if (alpha[i] === 0) rgba.fill(0, i * 4, i * 4 + 3);
      }
      rendered.set(size, await sharp(rgba, {
        raw: { width: size, height: size, channels: 4 },
      }).png().toBuffer());
    }
    return rendered.get(size);
  }

  const pngSizes = {
    '32x32.png': 32, '64x64.png': 64, '128x128.png': 128,
    '128x128@2x.png': 256, 'StoreLogo.png': 50,
  };
  for (const size of [30, 44, 71, 89, 107, 142, 150, 284, 310]) {
    pngSizes[`Square${size}x${size}Logo.png`] = size;
  }
  for (const [filename, size] of Object.entries(pngSizes)) {
    await fs.writeFile(path.join(output, filename), await renderPng(size));
  }

  // Modern Windows supports PNG-compressed ICO frames; each frame retains alpha.
  const sizes = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256];
  const frames = [];
  for (const size of sizes) frames.push(await renderPng(size));
  const header = Buffer.alloc(6 + 16 * frames.length);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(frames.length, 4);
  let offset = header.length;
  frames.forEach((frame, i) => {
    const entry = 6 + 16 * i;
    header[entry] = sizes[i] === 256 ? 0 : sizes[i];
    header[entry + 1] = header[entry];
    header.writeUInt16LE(1, entry + 4);
    header.writeUInt16LE(32, entry + 6);
    header.writeUInt32LE(frame.length, entry + 8);
    header.writeUInt32LE(offset, entry + 12);
    offset += frame.length;
  });
  await fs.writeFile(path.join(output, 'icon.ico'), Buffer.concat([header, ...frames]));

  // ICNS uses separate RGB / 8-bit alpha chunks for legacy small icons.
  // Modern and Retina representations use PNG. Putting PNG in legacy
  // small-size chunks can lose transparency in the native macOS decoder.
  const chunks = [];
  function addChunk(type, payload) {
    const header = Buffer.alloc(8);
    header.write(type, 0, 4, 'ascii');
    header.writeUInt32BE(payload.length + 8, 4);
    chunks.push(header, payload);
  }
  for (const [size, rgbType, alphaType] of [[16, 'is32', 's8mk'],
    [32, 'il32', 'l8mk'], [48, 'ih32', 'h8mk']]) {
    const rgba = await sharp(await renderPng(size)).raw().toBuffer();
    const rgb = Buffer.alloc(size * size * 3);
    const alpha = Buffer.alloc(size * size);
    for (let i = 0; i < alpha.length; i++) {
      rgba.copy(rgb, i * 3, i * 4, i * 4 + 3);
      alpha[i] = rgba[i * 4 + 3];
    }
    addChunk(rgbType, rgb);
    addChunk(alphaType, alpha);
  }
  for (const [type, size] of [['ic07', 128], ['ic08', 256], ['ic09', 512],
    ['ic10', 1024], ['ic11', 32], ['ic12', 64], ['ic13', 256], ['ic14', 512]]) {
    addChunk(type, await renderPng(size));
  }
  const icnsHeader = Buffer.alloc(8);
  icnsHeader.write('icns');
  icnsHeader.writeUInt32BE(8 + chunks.reduce((sum, chunk) => sum + chunk.length, 0), 4);
  await fs.writeFile(path.join(output, 'icon.icns'), Buffer.concat([icnsHeader, ...chunks]));
  console.log('Generated transparent rounded PNG, Windows ICO and macOS ICNS assets.');
}

main().catch(error => { console.error(error); process.exitCode = 1; });
