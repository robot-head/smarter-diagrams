const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { spawnSync } = require('node:child_process');
const browserOptions = require('./browser-options.cjs');
const checkSvg = require('./check-rendered-svg.cjs');
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
  for (const topic of ['basics', 'ports', 'layout', 'overlays', 'reference', 'gallery']) {
    await page.goto(url(topic==='gallery'?'examples.html':`language/${topic}.html`));
    const codeBlocks=await page.locator('.vp-doc div.language-diagram').count();
    const groups=page.locator('.diagram-examples');
    const diagnosticCount=await page.locator('.diagram-diagnostic').count();
    assert(codeBlocks>0 && await groups.count()+diagnosticCount>=codeBlocks,`missing examples: ${topic}`);
    for(let i=0;i<await groups.count();i++){
      const group=groups.nth(i),tabs=group.getByRole('tab'),choices=Math.max(1,await tabs.count());
      let comparisonFrame;
      for(let choice=0;choice<choices;choice++){
        if(await tabs.count())await tabs.nth(choice).click();
        const image=group.locator('figure img');await image.evaluate(image=>image.loading='eager');
        await image.evaluate(image=>image.decode());
        const source=await image.getAttribute('src');
        const markup=await(await page.request.get(new URL(source,base).href)).text();
        assert.deepEqual(await page.evaluate(checkSvg,markup),[],source);
        const frame=markup.match(/viewBox="([^"]+)"/)[1];
        if(comparisonFrame)assert.equal(frame,comparisonFrame,'comparison must keep its scale/frame');comparisonFrame=frame;
        assert.equal(await group.locator('output').textContent(),'100%','readable native size must be the default');
        const label=await group.locator('figcaption').textContent();
        if(label.includes('View: request_only')){assert(markup.includes('data-node="client"'));assert(!markup.includes('data-node="metrics"'));}
        if(label.includes('View: downstream')){assert(markup.includes('data-node="metrics"'));assert(!markup.includes('data-node="client"'));}
        if(label.includes('Focus: api'))assert(markup.includes('data-focus-opacity="0.35" data-node="archive"'));
        diagrams++;
      }
      if(i===0){await group.locator('summary').click();assert(await group.getByRole('table',{name:'Nodes and ownership'}).isVisible());await group.locator('summary').click();}
    }
    if(topic==='layout'){
      assert.match(await page.locator('.diagram-diagnostic').textContent(),/infeasible layout/);
      const first=groups.first();await first.getByLabel('Show grid').check();assert(await first.getByRole('table',{name:'Grid coordinates and dimensions'}).isVisible());assert(await first.locator('.diagram-grid line').count()>4);await first.getByLabel('Show grid').uncheck();
    }
    if(topic==='ports'||topic==='layout')await page.screenshot({path:`artifacts/language-${topic}.png`,fullPage:true});
  }
  assert.equal(diagrams,40,'all language and gallery choices must be checked');
  await page.goto(url('language/reference.html'));
  const dense=page.locator('.diagram-examples').last();
  await page.setViewportSize({width:375,height:812});
  const pan=dense.locator('.diagram-viewport');await pan.focus();await pan.press('ArrowRight');
  await page.waitForFunction(()=>[...document.querySelectorAll('.diagram-viewport')].some(el=>el.scrollLeft>0));
  await dense.getByRole('button',{name:'Zoom in',exact:true}).click();assert.equal(await dense.locator('output').textContent(),'125%');
  await dense.getByRole('button',{name:'Fit',exact:true}).click();assert(await dense.locator('img').evaluate(img=>img.getBoundingClientRect().width<=img.closest('.diagram-viewport').clientWidth+1));
  await dense.getByRole('button',{name:'Actual size (100%)',exact:true}).click();assert.equal(await dense.locator('output').textContent(),'100%');
  assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'viewer must contain mobile overflow');
  await page.setViewportSize({width:1440,height:1000});
  console.log(`Examples: ${diagrams} language/gallery choices passed glyph, leader, contrast, silhouette, arrow, description, comparison, grid, and mobile zoom/pan checks.`);

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
