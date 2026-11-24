// BLAKE3 (default hash mode, 32-byte output) in plain JS, for Accept scripts (phase 4.3).
//
// Node and the sidecar venv have no blake3, and ss-core's own hash cannot judge ss-core's own writes, so p4 hashes
// files with this independent implementation (a transcription of the BLAKE3 reference implementation,
// github.com/BLAKE3-team/BLAKE3 reference_impl.rs). `selfTest()` checks it against published vectors, and p4 also
// compares it with the `content_hash` ss-core stored (blake3 crate) for multi-chunk fixture files.
const IV = Uint32Array.of(0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19);
const PERM = [2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8];
const CHUNK_START = 1;
const CHUNK_END = 2;
const PARENT = 4;
const ROOT = 8;
const BLOCK_LEN = 64;
const CHUNK_LEN = 1024;

const rotr = (x, n) => ((x >>> n) | (x << (32 - n))) >>> 0;

function g(s, a, b, c, d, mx, my) {
  s[a] = (s[a] + s[b] + mx) >>> 0;
  s[d] = rotr(s[d] ^ s[a], 16);
  s[c] = (s[c] + s[d]) >>> 0;
  s[b] = rotr(s[b] ^ s[c], 12);
  s[a] = (s[a] + s[b] + my) >>> 0;
  s[d] = rotr(s[d] ^ s[a], 8);
  s[c] = (s[c] + s[d]) >>> 0;
  s[b] = rotr(s[b] ^ s[c], 7);
}

function round(s, m) {
  g(s, 0, 4, 8, 12, m[0], m[1]);
  g(s, 1, 5, 9, 13, m[2], m[3]);
  g(s, 2, 6, 10, 14, m[4], m[5]);
  g(s, 3, 7, 11, 15, m[6], m[7]);
  g(s, 0, 5, 10, 15, m[8], m[9]);
  g(s, 1, 6, 11, 12, m[10], m[11]);
  g(s, 2, 7, 8, 13, m[12], m[13]);
  g(s, 3, 4, 9, 14, m[14], m[15]);
}

/** The compression function: 16 output words. `counter` is a chunk index (< 2^53). */
function compress(cv, words, counter, blockLen, flags) {
  const s = new Uint32Array(16);
  s.set(cv, 0);
  s.set(IV.subarray(0, 4), 8);
  s[12] = counter >>> 0;
  s[13] = Math.floor(counter / 0x100000000) >>> 0;
  s[14] = blockLen;
  s[15] = flags;
  let m = Uint32Array.from(words);
  for (let r = 0; r < 7; r++) {
    round(s, m);
    if (r < 6) m = Uint32Array.from(PERM, (i) => m[i]);
  }
  for (let i = 0; i < 8; i++) {
    s[i] ^= s[i + 8];
    s[i + 8] ^= cv[i];
  }
  return s;
}

/** 16 little-endian words of a block (zero padded to 64 bytes). */
function blockWords(bytes) {
  const b = new Uint8Array(BLOCK_LEN);
  b.set(bytes);
  const dv = new DataView(b.buffer);
  return Uint32Array.from({ length: 16 }, (_, i) => dv.getUint32(i * 4, true));
}

/** Chaining value + final-block inputs of one chunk (<= 1024 bytes). */
function chunkOutput(chunk, counter) {
  let cv = Uint32Array.from(IV);
  const blocks = Math.max(1, Math.ceil(chunk.length / BLOCK_LEN));
  for (let i = 0; i < blocks - 1; i++) {
    const flags = i === 0 ? CHUNK_START : 0;
    cv = compress(cv, blockWords(chunk.subarray(i * BLOCK_LEN, (i + 1) * BLOCK_LEN)), counter, BLOCK_LEN, flags).subarray(0, 8);
  }
  const last = chunk.subarray((blocks - 1) * BLOCK_LEN);
  return { cv, words: blockWords(last), counter, blockLen: last.length, flags: (blocks === 1 ? CHUNK_START : 0) | CHUNK_END };
}

const parentOutput = (left, right) => {
  const words = new Uint32Array(16);
  words.set(left, 0);
  words.set(right, 8);
  return { cv: IV, words, counter: 0, blockLen: BLOCK_LEN, flags: PARENT };
};
const chainingValue = (o) => compress(o.cv, o.words, o.counter, o.blockLen, o.flags).subarray(0, 8);

/** BLAKE3 of `data` (Buffer / Uint8Array) as 64 lower-case hex characters. */
export function blake3Hex(data) {
  const bytes = data instanceof Uint8Array ? data : new Uint8Array(data);
  const chunks = Math.max(1, Math.ceil(bytes.length / CHUNK_LEN));
  const stack = [];
  for (let c = 0; c < chunks - 1; c++) {
    let cv = chainingValue(chunkOutput(bytes.subarray(c * CHUNK_LEN, (c + 1) * CHUNK_LEN), c));
    // Merge completed subtrees: one merge per trailing zero bit of the chunk count so far.
    for (let total = c + 1; (total & 1) === 0; total >>= 1) cv = chainingValue(parentOutput(stack.pop(), cv));
    stack.push(cv);
  }
  let out = chunkOutput(bytes.subarray((chunks - 1) * CHUNK_LEN), chunks - 1);
  while (stack.length) out = parentOutput(stack.pop(), chainingValue(out));
  const root = compress(out.cv, out.words, 0, out.blockLen, out.flags | ROOT);
  return Buffer.from(root.buffer, root.byteOffset, 32).toString("hex");
}

// Published vectors: BLAKE3 test_vectors.json (input byte i = i % 251) and the README's "abc".
const VECTORS = [
  [0, "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"],
  [1, "2d3adedff11b61f14c886e35afa036736dcd87a74d27b5c1510225d0f592e213"],
  [1024, "42214739f095a406f3fc83deb889744ac00df831c10daa55189b5d121c855af7"],
  [1025, "d00278ae47eb27b34faecf67b4fe263f82d5412916c1ffd97c8cb7fb814b8444"],
];

/** { ok, value }: the vectors above plus "abc". */
export function selfTest() {
  const bad = [];
  for (const [n, want] of VECTORS) {
    const got = blake3Hex(Uint8Array.from({ length: n }, (_, i) => i % 251));
    if (got !== want) bad.push(`len ${n}: ${got}`);
  }
  const abc = blake3Hex(Buffer.from("abc"));
  if (abc !== "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85") bad.push(`"abc": ${abc}`);
  return { ok: bad.length === 0, value: bad.length ? `mismatch ${bad.join("; ")}` : `${VECTORS.length + 1} published vectors match` };
}
