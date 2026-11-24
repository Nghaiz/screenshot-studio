// Generated RGB PNG fixtures for Accept scripts (moved out of p1.mjs in phase 4.3, shared with p4.mjs).
import { crc32, deflateSync } from "node:zlib";

/** A `width` x `height` RGB PNG whose pixels are a pattern of `seed` (different seeds, different pictures). */
export function png(width, height, seed) {
  const raw = Buffer.alloc((width * 3 + 1) * height);
  for (let y = 0; y < height; y++) {
    const row = y * (width * 3 + 1);
    for (let x = 0; x < width; x++) {
      const o = row + 1 + x * 3;
      raw[o] = (x * 3 + seed * 7) & 255;
      raw[o + 1] = (y * 5 + seed * 13) & 255;
      raw[o + 2] = ((x ^ y) + seed * 29) & 255;
    }
  }
  const chunk = (type, data) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(body));
    return Buffer.concat([len, body, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 2; // RGB
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}
