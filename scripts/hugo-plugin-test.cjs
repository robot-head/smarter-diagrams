const assert = require('node:assert/strict');
const fs = require('node:fs');
const http = require('node:http');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { chromium } = require('playwright');
const browserOptions = require('./browser-options.cjs');
const checkSvg = require('./check-rendered-svg.cjs');

const root = path.resolve(__dirname, '..');
const hugoBin = process.env.HUGO_BIN || 'hugo';
const cliBin = process.env.SMARTER_DIAGRAMS_BIN || path.join(root, 'target/debug/smarter-diagrams');
const wasmSrc = path.join(root, 'web/pkg');
const prerenderScript = path.join(root, 'extensions/hugo/scripts/prerender.mjs');

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.wasm': 'application/wasm',
  '.svg': 'image/svg+xml',
  '.css': 'text/css; charset=utf-8',
};

function serveStatic(publicDir) {
  return new Promise(resolve => {
    const server = http.createServer((req, res) => {
      const urlPath = decodeURIComponent((req.url || '/').split('?')[0]);
      let filePath = path.join(publicDir, urlPath);
      if (fs.existsSync(filePath) && fs.statSync(filePath).isDirectory()) {
        filePath = path.join(filePath, 'index.html');
      }
      if (!fs.existsSync(filePath)) {
        res.writeHead(404);
        res.end('Not found');
        return;
      }
      const ext = path.extname(filePath).toLowerCase();
      res.writeHead(200, { 'Content-Type': MIME[ext] || 'application/octet-stream' });
      res.end(fs.readFileSync(filePath));
    });
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      resolve({ server, origin: `http://127.0.0.1:${port}` });
    });
  });
}

(async () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'sd-hugo-plugin-test-'));
  try {
    fs.mkdirSync(path.join(tmpDir, 'layouts/_default'), { recursive: true });
    fs.mkdirSync(path.join(tmpDir, 'content'), { recursive: true });
    fs.mkdirSync(path.join(tmpDir, 'themes'), { recursive: true });
    fs.cpSync(path.join(root, 'extensions/hugo'), path.join(tmpDir, 'themes/smarter-diagrams'), { recursive: true });

    fs.writeFileSync(
      path.join(tmpDir, 'hugo.toml'),
      `baseURL = "http://example.org/docs/"
locale = "en-us"
title = "Hugo Smarter Diagrams Test"
theme = ["smarter-diagrams"]

[params.smarterDiagrams]
  mode = "auto"
  wasmModuleUrl = "/smarter-diagrams/pkg/smarter_diagrams.js"
`
    );

    const baseTemplate = `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{{ .Title }}</title>
  {{ partial "smarter-diagrams/head.html" . }}
</head>
<body>
  <main>{{ .Content }}</main>
</body>
</html>
`;
    fs.writeFileSync(path.join(tmpDir, 'layouts/_default/single.html'), baseTemplate);
    fs.writeFileSync(path.join(tmpDir, 'layouts/_default/list.html'), baseTemplate);

    fs.writeFileSync(
      path.join(tmpDir, 'content/_index.md'),
      `---
title: "Hugo Fenced Diagrams"
---

# Hugo Fenced Diagrams

\`\`\`diagram {id="fig-static-view" view="control_loop" caption="Static pre-rendered reconciliation view" showSource="details"}
kubectl: "kubectl" [shape: person]
apiserver: "kube-apiserver"
reconciler: "Operator Reconciler"

kubectl -> apiserver: "apply CR"
apiserver -> reconciler: "watch"
reconciler -> apiserver: "patch status"

view control_loop {
  include apiserver -> reconciler -> apiserver
}
\`\`\`

~~~smarter-diagrams {id="fig-static-focus" focus="gateway" hops="1" caption="Static pre-rendered focus lens"}
client -> gateway: HTTPS
gateway -> auth: mTLS
auth -> audit_db: SQL
~~~

\`\`\`diagram {id="fig-wasm-live" mode="wasm" focus="checkout" hops="1" caption="Live browser Wasm focus render"}
edge_gw -> checkout: gRPC
checkout -> postgres: SQL
postgres -> replica: WAL
\`\`\`
`
    );

    const prerender = spawnSync(
      process.execPath,
      [
        prerenderScript,
        '--site',
        tmpDir,
        '--bin',
        cliBin,
        '--wasm-src',
        wasmSrc,
        '--copy-wasm',
      ],
      { encoding: 'utf8' }
    );
    assert.equal(prerender.status, 0, prerender.stderr || prerender.stdout);
    assert.match(prerender.stdout, /2 diagram block\(s\) across 1 file\(s\) \(2 rendered, 0 cached\)/);

    const cachedRun = spawnSync(
      process.execPath,
      [prerenderScript, '--site', tmpDir, '--bin', cliBin, '--wasm-src', wasmSrc],
      { encoding: 'utf8' }
    );
    assert.equal(cachedRun.status, 0, cachedRun.stderr || cachedRun.stdout);
    assert.match(cachedRun.stdout, /\(0 rendered, 2 cached\)/);

    const hugoRes = spawnSync(
      hugoBin,
      ['--source', tmpDir, '--destination', path.join(tmpDir, 'public/docs')],
      { encoding: 'utf8' }
    );
    assert.equal(hugoRes.status, 0, `hugo build failed:\n${hugoRes.stderr}\n${hugoRes.stdout}`);

    const builtHtml = fs.readFileSync(path.join(tmpDir, 'public/docs/index.html'), 'utf8');
    assert.equal(
      (builtHtml.match(/data-smarter-diagrams-styles/g) || []).length,
      1,
      'palette stylesheet must be emitted once per page'
    );
    assert.equal(
      (builtHtml.match(/<script type="module">/g) || []).length,
      1,
      'Wasm loader script must be emitted at most once per page'
    );

    const { server, origin } = await serveStatic(path.join(tmpDir, 'public'));
    const browser = await chromium.launch(browserOptions(`${origin}/docs/`));
    try {
      const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
      const errors = [];
      page.on('pageerror', e => errors.push(e.message));
      page.on('response', r => {
        if (r.status() >= 400) errors.push(`${r.status()} ${r.url()}`);
      });

      await page.goto(`${origin}/docs/`);
      await page.waitForSelector('#fig-wasm-live [data-sd-wasm="ready"]');

      assert.equal(await page.locator('#fig-static-view').getAttribute('data-sd-mode'), 'static');
      assert.equal(await page.locator('#fig-static-view [data-node="kubectl"]').count(), 0, 'view projection must omit kubectl');
      assert.equal(await page.locator('#fig-static-view [data-node="reconciler"]').count(), 1);
      assert.equal(await page.locator('#fig-static-view details.smarter-diagram-source').count(), 1);

      assert.equal(await page.locator('#fig-static-focus').getAttribute('data-sd-mode'), 'static');
      assert.equal(
        await page.locator('#fig-static-focus [data-node="audit_db"]').getAttribute('data-focus-opacity'),
        '0.35'
      );

      assert.equal(await page.locator('#fig-wasm-live').getAttribute('data-sd-mode'), 'wasm');
      assert.equal(
        await page.locator('#fig-wasm-live [data-node="replica"]').getAttribute('data-focus-opacity'),
        '0.35'
      );

      for (const theme of ['light', 'dark']) {
        await page.evaluate(t => {
          document.documentElement.dataset.theme = t;
          document.documentElement.classList.toggle('dark', t === 'dark');
        }, theme);
        const svgs = await page.evaluate(() => {
          const cs = getComputedStyle(document.documentElement);
          const vars = [
            '--sd-flow', '--sd-flow-text', '--sd-bg', '--sd-surface', '--sd-fg', '--sd-edge',
            '--sd-muted', '--sd-node', '--sd-border', '--sd-database', '--sd-cap',
            '--sd-database-border', '--sd-actor', '--sd-actor-border', '--sd-cache',
            '--sd-cache-border', '--sd-decision', '--sd-decision-border', '--sd-queue',
            '--sd-queue-line', '--sd-container', '--sd-plane', '--sd-zone-text', '--sd-story',
            '--sd-added', '--sd-removed', '--sd-modified', '--sd-status', '--sd-overload',
            '--sd-overload-fill', '--sd-cut',
          ].map(k => `${k}:${cs.getPropertyValue(k).trim()}`).join(';');
          return [...document.querySelectorAll('figure.smarter-diagram svg')].map(el =>
            el.outerHTML.replace('<svg ', `<svg style="${vars}" `)
          );
        });
        assert.equal(svgs.length, 3);
        for (const markup of svgs) {
          assert.deepEqual(await page.evaluate(checkSvg, markup), [], `SVG check failed in ${theme} theme`);
        }
      }

      assert.deepEqual(errors, []);
      console.log('Hugo plugin test passed: static view/focus pre-render, caching, subpath baseURL Wasm render, and light/dark SVG accessibility.');
    } finally {
      await browser.close();
      server.close();
    }
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
})().catch(err => {
  console.error(err);
  process.exit(1);
});
