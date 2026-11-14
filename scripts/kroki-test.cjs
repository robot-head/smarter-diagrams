const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const zlib = require('node:zlib');
const { createKrokiServer } = require('../extensions/kroki/server.cjs');

const root = path.resolve(__dirname, '..');

function resolveBinary() {
  if (process.env.SMARTER_DIAGRAMS_BIN && fs.existsSync(process.env.SMARTER_DIAGRAMS_BIN)) {
    return process.env.SMARTER_DIAGRAMS_BIN;
  }
  const exe = process.platform === 'win32' ? '.exe' : '';
  const debug = path.join(root, 'target', 'debug', `smarter-diagrams${exe}`);
  const release = path.join(root, 'target', 'release', `smarter-diagrams${exe}`);
  if (fs.existsSync(debug) && fs.existsSync(release)) {
    return fs.statSync(debug).mtimeMs >= fs.statSync(release).mtimeMs ? debug : release;
  }
  if (fs.existsSync(debug)) return debug;
  if (fs.existsSync(release)) return release;
  execFileSync('cargo', ['build', '--locked', '--features', 'export', '--bin', 'smarter-diagrams'], {
    cwd: root,
    stdio: 'inherit',
  });
  return debug;
}

function encodeKroki(source) {
  return zlib
    .deflateSync(Buffer.from(source, 'utf8'), { level: 9 })
    .toString('base64')
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');
}

async function main() {
  const bin = resolveBinary();
  const server = createKrokiServer({ bin });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const { port } = server.address();
  const baseUrl = `http://127.0.0.1:${port}`;

  try {
    // 1. Health endpoint
    const health = await fetch(`${baseUrl}/health`);
    assert.equal(health.status, 200);
    const healthJson = await health.json();
    assert.equal(healthJson.status, 'ok');
    assert.equal(healthJson.engine, 'smarter-diagrams');

    // 2. POST raw source -> SVG
    const sample = 'client -> api: HTTPS\napi -> db: SQL\n';
    const postSvg = await fetch(`${baseUrl}/smarterdiagrams/svg`, {
      method: 'POST',
      headers: { 'Content-Type': 'text/plain' },
      body: sample,
    });
    assert.equal(postSvg.status, 200);
    assert.match(postSvg.headers.get('content-type') || '', /image\/svg\+xml/);
    const svgText = await postSvg.text();
    assert.ok(svgText.includes('<svg') && svgText.includes('HTTPS'));

    // 3. POST JSON gateway with theme + progressive step options
    const stepped = `${sample}step s1 { include client -> api }\n`;
    const postJson = await fetch(`${baseUrl}/`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        diagram_type: 'smarterdiagrams',
        output_format: 'svg',
        diagram_source: stepped,
        diagram_options: { theme: 'catppuccin-mocha', step: 's1' },
      }),
    });
    assert.equal(postJson.status, 200);
    const themedSvg = await postJson.text();
    assert.ok(themedSvg.includes('data-explicit-theme="catppuccin-mocha"'));
    assert.ok(themedSvg.includes('data-node="client"') && !themedSvg.includes('data-node="db"'));

    // 4. POST -> PNG bytes
    const postPng = await fetch(`${baseUrl}/smarterdiagrams/png`, {
      method: 'POST',
      body: sample,
    });
    assert.equal(postPng.status, 200);
    const pngBuf = Buffer.from(await postPng.arrayBuffer());
    assert.equal(pngBuf.subarray(0, 8).toString('hex'), '89504e470d0a1a0a', 'expected PNG magic bytes');

    // 5. GET compressed base64url payload
    const encoded = encodeKroki(sample);
    const getSvg = await fetch(`${baseUrl}/smarterdiagrams/svg/${encoded}`);
    assert.equal(getSvg.status, 200);
    const getSvgText = await getSvg.text();
    assert.ok(getSvgText.includes('<svg') && getSvgText.includes('SQL'));

    // 6. Invalid diagram source returns 400
    const bad = await fetch(`${baseUrl}/smarterdiagrams/svg`, {
      method: 'POST',
      body: 'a ->\n',
    });
    assert.equal(bad.status, 400);

    console.log(
      `Kroki companion test passed: health, POST svg/png, JSON options (theme+step), GET deflate (${encoded.length} chars), and 400 error handling.`
    );
  } finally {
    await new Promise(resolve => server.close(resolve));
  }
}

main().catch(err => {
  console.error(err);
  process.exit(1);
});
