const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { spawnSync } = require('node:child_process');
const browserOptions = require('./browser-options.cjs');
const base = new URL(process.env.DOCS_TEST_URL || 'http://127.0.0.1:4173/smarter-diagrams/');
if (!base.pathname.endsWith('/')) base.pathname += '/';
const url = path => new URL(path, base).href;

(async () => {
  const browser = await chromium.launch(browserOptions(base.href));
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  page.on('response', r => { if (r.status() >= 400 && r.url().startsWith(base.href)) errors.push(`${r.status()} ${r.url()}`); });
  let ready = false;
  for (let attempt = 0; attempt < 40; attempt++) {
    try { ready = (await page.request.get(base.href, { timeout: 1000 })).ok(); } catch {}
    if (ready) break;
    await new Promise(resolve => setTimeout(resolve, 250));
  }
  assert(ready, `Site unavailable: ${base.href}`);
  await page.goto(base.href);
  await page.getByRole('heading', { name: 'Your first diagram' }).waitFor();
  fs.mkdirSync('artifacts', { recursive: true });
  await page.screenshot({ path: 'artifacts/docs-home.png', fullPage: true });

  // Crawl the rendered documentation and same-site static assets. rustdoc has
  // its own generated cross-reference graph; inspect its principal entries below.
  const queue = [base.href], seen = new Set(), assets = new Set();
  while (queue.length) {
    const target = queue.shift(); if (seen.has(target)) continue; seen.add(target);
    const response = await page.request.get(target); assert.equal(response.status(), 200, target);
    const html = await response.text();
    const links = await page.evaluate(html => {
      const document = new DOMParser().parseFromString(html, 'text/html');
      return [...document.querySelectorAll('a[href],img[src],script[src],link[href]')].map(el => el.getAttribute('href') || el.getAttribute('src'));
    }, html);
    for (const link of links) {
      const next = new URL(link, target); next.hash = '';
      if (next.origin !== base.origin) continue;
      assert(next.pathname.startsWith(base.pathname), `link escaped Pages base: ${next.href}`);
      if (next.pathname.includes('/api/')) continue;
      if (next.pathname.endsWith('.html') || next.pathname === base.pathname) {
        if (!next.search && !next.pathname.includes('/demo/')) queue.push(next.href);
      } else if (!next.pathname.endsWith('/')) assets.add(next.href);
    }
  }
  assert(seen.size >= 25, `only ${seen.size} documentation pages discovered`);
  for (const target of assets) assert.equal((await page.request.get(target)).status(), 200, target);
  console.log(`Documentation: ${seen.size} pages and ${assets.size} static asset links passed.`);

  let diagrams = 0;
  for (const topic of ['basics', 'ports', 'layout', 'overlays', 'reference']) {
    await page.goto(url(`language/${topic}.html`));
    const codeBlocks = await page.locator('.vp-doc div.language-diagram').count();
    const figures = page.locator('figure.diagram-example');
    assert(codeBlocks > 0 && await figures.count() >= codeBlocks, `missing visual examples: ${topic}`);
    await figures.locator('img').evaluateAll(images => images.forEach(image => { image.loading = 'eager'; }));
    await page.waitForFunction(() => [...document.querySelectorAll('.diagram-example img')].every(image => image.complete && image.naturalWidth > 0));
    for (const source of await figures.locator('img').evaluateAll(images => images.map(image => image.src))) {
      const markup = await (await page.request.get(source)).text();
      const failures = await page.evaluate(markup => {
        const svg = new DOMParser().parseFromString(markup, 'image/svg+xml').documentElement;
        const holder = document.createElement('div');
        holder.style.cssText = 'position:absolute;left:-10000px;visibility:hidden';
        holder.append(svg); document.body.append(holder);
        try {
          const failures = [];
          const close = (a, b) => Math.hypot(a.x - b.x, a.y - b.y) < 0.02;
          const outlines = [...svg.querySelectorAll('[data-node]')].map(node => node.querySelector('rect,path,ellipse')).filter(Boolean);
          const attached = point => outlines.some(outline => outline.isPointInStroke(new DOMPoint(point.x, point.y)));
          const shafts = [...svg.querySelectorAll('path.edge,path.flow')].map(path => ({
            id: path.dataset.edge, color: getComputedStyle(path).stroke,
            endpoints: [path.getPointAtLength(0), path.getPointAtLength(path.getTotalLength())],
          }));
          const heads = [...svg.querySelectorAll('[data-arrowhead]')].map(head => {
            const n = head.getAttribute('d').match(/-?\d+(?:\.\d+)?/g).map(Number);
            return { id: head.dataset.edge, tip: { x: n[0], y: n[1] }, base: { x: (n[2] + n[4]) / 2, y: (n[3] + n[5]) / 2 }, color: getComputedStyle(head).fill };
          });
          for (const port of svg.querySelectorAll('[data-port]')) if (getComputedStyle(port).fill !== 'rgba(0, 0, 0, 0)') failures.push(`visible port dot: ${port.dataset.port}`);
          for (const head of heads) {
            if (!attached(head.tip)) failures.push(`arrow tip detached from silhouette: ${head.id}`);
            if (!shafts.some(shaft => shaft.id === head.id && shaft.color === head.color && shaft.endpoints.some(point => close(point, head.base)))) failures.push(`arrow/shaft gap or color mismatch: ${head.id}`);
          }
          for (const shaft of shafts) for (const point of shaft.endpoints) {
            if (!attached(point) && !heads.some(head => head.id === shaft.id && close(point, head.base))) failures.push(`connector endpoint detached: ${shaft.id}`);
          }
          return failures;
        } finally { holder.remove(); }
      }, markup);
      assert.deepEqual(failures, [], source);
    }
    diagrams += await figures.count();
    if (topic === 'ports' || topic === 'layout') await page.screenshot({ path: `artifacts/language-${topic}.png`, fullPage: true });
    if (topic === 'layout') {
      const image = caption => figures.filter({ hasText: caption }).locator('img').getAttribute('src');
      assert.notEqual(await image('Default layout'), await image('Layout: compact'), 'variant comparison must render different geometry');
    }
    if (topic === 'overlays') {
      for (const [caption, present, absent] of [['View: request_only', 'client', 'metrics'], ['View: downstream', 'metrics', 'client']]) {
        const source = await figures.filter({ hasText: caption }).locator('img').getAttribute('src');
        const svg = await (await page.request.get(new URL(source, base).href)).text();
        assert(svg.includes(`data-node="${present}"`), caption);
        assert(!svg.includes(`data-node="${absent}"`), caption);
      }
      const source = await figures.filter({ hasText: 'Focus: api (1 hop)' }).locator('img').getAttribute('src');
      const svg = await (await page.request.get(new URL(source, base).href)).text();
      assert(svg.includes('opacity="0.35" data-node="archive"'), 'focus must fade the outer context');
    }
  }
  console.log(`Language: ${diagrams} decoded SVG examples; silhouette attachments, arrow joins/colors, invisible port anchors, views and layout comparisons passed.`);

  await page.goto(base.href);

  await page.getByRole('button', { name: /search/i }).first().click();
  await page.locator('#localsearch-input').fill('UnpinEntities');
  await page.waitForFunction(() => document.querySelector('.VPLocalSearchBox')?.textContent.includes('Structured editing'));
  await page.keyboard.press('Escape');
  await page.goto(url('guide/getting-started.html'));
  await page.locator('.vp-doc h1').filter({ hasText: 'Getting started' }).waitFor();
  await page.reload(); await page.locator('.vp-doc h1').filter({ hasText: 'Getting started' }).waitFor();

  for (const path of ['api/smarter_diagrams/index.html', 'api/smarter_diagrams/document/struct.Document.html', 'api/smarter_diagrams/fn.compile.html']) {
    const response = await page.goto(url(path)); assert.equal(response.status(), 200, path);
    await page.locator('main').waitFor();
  }
  await page.goto(url('playground.html'));
  const frame = await (await page.locator('iframe').elementHandle()).contentFrame();
  await frame.waitForFunction(() => document.querySelector('#status')?.textContent.includes('revision'));
  assert.equal(await frame.locator('[data-node]').count(), 4);
  await page.screenshot({ path: 'artifacts/docs-playground.png', fullPage: true });

  await page.goto(url('examples.html'));
  await page.getByRole('link', { name: 'Open in the editor', exact: true }).first().click();
  await page.waitForFunction(() => document.querySelector('#status')?.textContent.includes('revision'));
  assert.equal(await page.locator('[data-node]').count(), 3);
  assert.match(await page.locator('#source').inputValue(), /api: API service/);
  const downloadPromise = page.waitForEvent('download'); await page.click('#download');
  const download = await downloadPromise; await download.saveAs('artifacts/live-diagram.svg');
  assert.match(fs.readFileSync('artifacts/live-diagram.svg', 'utf8'), /data-node="api"/);

  await page.setViewportSize({ width: 375, height: 812 });
  for (const path of ['', 'guide/getting-started.html', 'language/ports.html', 'demo/']) {
    await page.goto(url(path));
    if (path === 'demo/') await page.waitForFunction(() => document.querySelector('#status')?.textContent.includes('revision'));
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), `horizontal overflow: ${path}`);
  }
  await page.screenshot({ path: 'artifacts/docs-mobile-editor.png', fullPage: true });
  assert.deepEqual(errors, []);
  await browser.close();
  const result = spawnSync(process.execPath, ['scripts/browser-test.cjs'], { env: { ...process.env, DIAGRAM_TEST_URL: url('demo/') }, stdio: 'inherit' });
  assert.equal(result.status, 0, 'authoring and spatial smoke tests');
  console.log(`Passed desktop/mobile documentation, search, deep links, Rust API, embedded editor, example loading, SVG download and live runtime checks at ${base.href}`);
})().catch(error => { console.error(error); process.exit(1); });
