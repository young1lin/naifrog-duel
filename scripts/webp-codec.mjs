// Lossless WebP for the game textures, through sharp (libvips + libwebp). No filesystem side effects.
// The browser downloads these instead of PNG: the same pixels in less than half the bytes.
import assert from 'node:assert/strict';
import sharp from 'sharp';

// RGBA (or any image sharp reads, such as PNG) -> {width, height, pixels} as 8-bit RGBA.
export async function decodeRgba(buffer) {
  const {data, info} = await sharp(buffer).ensureAlpha().raw().toBuffer({resolveWithObject: true});
  return {width: info.width, height: info.height, pixels: data};
}

// 8-bit RGBA -> lossless WebP. `exact` keeps the colour of fully transparent pixels, so
// filtering at sprite edges samples exactly what the atlas holds; the round trip is checked.
export async function encodeWebp(width, height, pixels) {
  assert.equal(pixels.length, width * height * 4);
  const webp = await sharp(pixels, {raw: {width, height, channels: 4}})
    .webp({lossless: true, effort: 6, exact: true})
    .toBuffer();
  assert((await decodeRgba(webp)).pixels.equals(pixels), 'WebP round trip changed pixels');
  return webp;
}
