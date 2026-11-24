// jsdom has no canvas. This installs a recording 2D context on every <canvas> and a `toBlob` that encodes a real
// (blank) PNG of the canvas's size, so the editor's pipeline can be checked by its calls and by the PNG it sends.
// Not a test file (vitest collects only *.test.ts(x)).
import { deflateSync } from "node:zlib";

export interface CtxCall {
  canvas: HTMLCanvasElement;
  op: string;
  args: unknown[];
}

export interface FakeCanvas {
  calls: CtxCall[];
  /** Every canvas `toBlob` was called on, with the bytes it produced. */
  blobs: { canvas: HTMLCanvasElement; width: number; height: number; bytes: Uint8Array<ArrayBuffer> }[];
  /** The calls made on one canvas, in order. */
  on: (canvas: HTMLCanvasElement) => CtxCall[];
  uninstall: () => void;
}

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (b: Uint8Array) => {
  let c = 0xffffffff;
  for (const x of b) c = CRC_TABLE[(c ^ x) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};

function chunk(type: string, data: Uint8Array): Uint8Array {
  const out = new Uint8Array(12 + data.length);
  const v = new DataView(out.buffer);
  v.setUint32(0, data.length);
  out.set(new TextEncoder().encode(type), 4);
  out.set(data, 8);
  v.setUint32(8 + data.length, crc32(out.subarray(4, 8 + data.length)));
  return out;
}

/** A valid RGBA PNG, all pixels transparent. */
export function blankPng(width: number, height: number): Uint8Array<ArrayBuffer> {
  const ihdr = new Uint8Array(13);
  const v = new DataView(ihdr.buffer);
  v.setUint32(0, width);
  v.setUint32(4, height);
  ihdr.set([8, 6, 0, 0, 0], 8); // 8-bit, RGBA, deflate, no filter, no interlace
  const raw = new Uint8Array((width * 4 + 1) * height); // filter byte 0 + zeros per row
  const parts = [new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk("IHDR", ihdr),
    chunk("IDAT", new Uint8Array(deflateSync(raw))), chunk("IEND", new Uint8Array(0))];
  const out = new Uint8Array(parts.reduce((a, p) => a + p.length, 0));
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.length;
  }
  return out;
}

export function installFakeCanvas(): FakeCanvas {
  const calls: CtxCall[] = [];
  const blobs: FakeCanvas["blobs"] = [];
  const proto = HTMLCanvasElement.prototype;
  const saved = { getContext: proto.getContext, toBlob: proto.toBlob, rect: proto.getBoundingClientRect };
  const contexts = new WeakMap<HTMLCanvasElement, unknown>();

  const make = (canvas: HTMLCanvasElement) => {
    const state: Record<string, unknown> = {};
    return new Proxy({} as Record<string, unknown>, {
      get(_, prop: string) {
        if (prop === "canvas") return canvas;
        if (prop in state) return state[prop];
        if (prop === "measureText") return (text: string) => ({ width: text.length * 8 });
        return (...args: unknown[]) => void calls.push({ canvas, op: prop, args });
      },
      set(_, prop: string, value: unknown) {
        state[prop] = value;
        calls.push({ canvas, op: `set:${prop}`, args: [value] });
        return true;
      },
    });
  };

  proto.getContext = function getContext(this: HTMLCanvasElement) {
    if (!contexts.has(this)) contexts.set(this, make(this));
    return contexts.get(this);
  } as typeof proto.getContext;
  proto.toBlob = function toBlob(this: HTMLCanvasElement, cb: BlobCallback, type?: string) {
    const bytes = blankPng(this.width, this.height);
    blobs.push({ canvas: this, width: this.width, height: this.height, bytes });
    cb(new Blob([bytes], { type: type ?? "image/png" }));
  };
  // No layout in jsdom: a canvas is laid out at its own pixel size, so client coordinates = canvas pixels.
  proto.getBoundingClientRect = function getBoundingClientRect(this: HTMLCanvasElement) {
    return { left: 0, top: 0, x: 0, y: 0, right: this.width, bottom: this.height, width: this.width, height: this.height, toJSON() {} } as DOMRect;
  };

  return {
    calls,
    blobs,
    on: (canvas) => calls.filter((c) => c.canvas === canvas),
    uninstall: () => {
      proto.getContext = saved.getContext;
      proto.toBlob = saved.toBlob;
      proto.getBoundingClientRect = saved.rect;
    },
  };
}
