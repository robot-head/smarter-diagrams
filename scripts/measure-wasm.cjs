const fs = require('node:fs');
const path = require('node:path');
const zlib = require('node:zlib');
const result = {};
for (const [name, directory] of [['core', 'pkg'], ['runtime', 'pkg-runtime']]) {
  const bytes = fs.readFileSync(path.join(__dirname, '..', 'web', directory, 'smarter_diagrams_bg.wasm'));
  const compressed = zlib.brotliCompressSync(bytes, { params: { [zlib.constants.BROTLI_PARAM_QUALITY]: 11 } });
  result[name] = { raw_bytes: bytes.length, brotli_bytes: compressed.length };
}
console.log(JSON.stringify(result, null, 2));
if (result.core.brotli_bytes >= 450000) {
  throw new Error('Core Wasm exceeds the 450 KB Brotli budget');
}
