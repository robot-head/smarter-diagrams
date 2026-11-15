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
  await page.getByRole('heading', { name: 'Diagram compiler for software engineers' }).waitFor();
  await page.waitForFunction(() => document.querySelector('#home-editor-status')?.textContent.includes('revision'));
  await page.waitForFunction(() => document.querySelector('#home-sim-metrics')?.textContent.includes('workers'));
  assert.equal(await page.locator('[data-ui],[data-ui-load],.ui-win,.ui-pipe,.ui-term,[data-placeholder]').count(), 0, 'homepage must contain no fake UI or placeholder markers');
  fs.mkdirSync('artifacts', { recursive: true });
  await page.screenshot({ path: 'artifacts/docs-home.png', fullPage: true });
  const initialHomeSource = await page.locator('#home-editor-source').inputValue();
  const homeNode = page.locator('#home-editor-canvas [data-node="gateway"]');
  await homeNode.scrollIntoViewIfNeeded();
  const homeBox = await homeNode.boundingBox();
  const homeScale = await homeNode.evaluate(el => el.ownerSVGElement.getScreenCTM().a);
  await page.mouse.move(homeBox.x + homeBox.width / 2, homeBox.y + homeBox.height / 2);
  await page.mouse.down();
  await page.mouse.move(homeBox.x + homeBox.width / 2, homeBox.y + homeBox.height / 2 + 64 * homeScale, { steps: 5 });
  await page.mouse.up();
  await page.waitForFunction(orig => document.querySelector('#home-editor-source')?.value !== orig, initialHomeSource);
  const draggedSource = await page.locator('#home-editor-source').inputValue();
  assert(draggedSource.includes('@pin('), 'homepage node drag must insert @pin coordinates');
  assert(draggedSource.includes('# Production ingress'), 'homepage node drag must preserve leading comments');
  assert.match(await page.locator('#home-editor-diff').textContent(), /@pin\(/, 'homepage CST diff must show @pin edit');
  await page.click('#home-editor-undo');
  assert.equal(await page.locator('#home-editor-source').inputValue(), initialHomeSource, 'homepage undo must restore original source');

  // Verify touch-action: none on homepage editor viewport + mobile touch pan & pinch
  const touchActions = await page.evaluate(() => ({
    canvas: getComputedStyle(document.querySelector('#home-editor-canvas')).touchAction,
    svg: getComputedStyle(document.querySelector('#home-editor-canvas svg')).touchAction,
  }));
  assert.equal(touchActions.canvas, 'none', '#home-editor-canvas must set touch-action: none');
  assert.equal(touchActions.svg, 'none', '#home-editor-canvas svg must set touch-action: none');
  const touchContext = await browser.newContext({ viewport: { width: 375, height: 812 }, isMobile: true, hasTouch: true });
  const touchPage = await touchContext.newPage();
  await touchPage.goto(base.href);
  await touchPage.waitForFunction(() => document.querySelector('#home-editor-status')?.textContent.includes('revision'));
  const touchCanvas = touchPage.locator('#home-editor-canvas');
  await touchCanvas.scrollIntoViewIfNeeded();
  await touchPage.locator('#home-editor-controls').getByRole('button', { name: 'Actual size (100%)', exact: true }).click();
  const cdp = await touchContext.newCDPSession(touchPage);
  const tBox = await touchCanvas.boundingBox();
  const swipeY = tBox.y + tBox.height / 2;
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ id: 0, x: tBox.x + tBox.width - 30, y: swipeY }] });
  for (let i = 1; i <= 6; i++) {
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ id: 0, x: tBox.x + tBox.width - 30 - 25 * i, y: swipeY }] });
    await touchPage.waitForTimeout(20);
  }
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await touchPage.waitForFunction(() => document.querySelector('#home-editor-canvas').scrollLeft > 50);
  assert.equal(await touchPage.locator('#home-editor-source').inputValue(), initialHomeSource, 'touch panning must not modify source');
  const cx = tBox.x + tBox.width / 2;
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ id: 0, x: cx - 30, y: swipeY }] });
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ id: 0, x: cx - 30, y: swipeY }, { id: 1, x: cx + 30, y: swipeY }] });
  for (let i = 1; i <= 5; i++) {
    await cdp.send('Input.dispatchTouchEvent', {
      type: 'touchMove',
      touchPoints: [{ id: 0, x: cx - 30 - 15 * i, y: swipeY }, { id: 1, x: cx + 30 + 15 * i, y: swipeY }],
    });
    await touchPage.waitForTimeout(20);
  }
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await touchPage.waitForFunction(() => Number.parseInt(document.querySelector('#home-editor-controls output')?.textContent || '0', 10) > 100);
  await touchContext.close();

  // Verify switching theme preset -> Auto clears inline --sd-* palette overrides and follows site theme toggle
  const initialAutoBg = await page.evaluate(() => getComputedStyle(document.querySelector('#home-editor-canvas svg')).getPropertyValue('--sd-bg').trim());
  await page.selectOption('#home-editor-theme', 'catppuccin-mocha');
  const presetState = await page.evaluate(() => {
    const svg = document.querySelector('#home-editor-canvas svg');
    return {
      attr: svg.getAttribute('data-explicit-theme'),
      sdBg: svg.style.getPropertyValue('--sd-bg').trim(),
    };
  });
  assert.equal(presetState.attr, 'catppuccin-mocha');
  assert(presetState.sdBg.length > 0, 'preset theme must set inline --sd-bg');
  await page.selectOption('#home-editor-theme', 'auto');
  const autoState = await page.evaluate(() => {
    const svg = document.querySelector('#home-editor-canvas svg');
    return {
      attr: svg.getAttribute('data-explicit-theme'),
      inlineSdBg: svg.style.getPropertyValue('--sd-bg').trim(),
      inlineBg: svg.style.background,
      inlineWidth: svg.style.width,
      inlineHeight: svg.style.height,
      computedSdBg: getComputedStyle(svg).getPropertyValue('--sd-bg').trim(),
    };
  });
  assert.equal(autoState.attr, null, 'auto theme must clear data-explicit-theme');
  assert.equal(autoState.inlineSdBg, '', 'auto theme must clear inline --sd-bg');
  assert.equal(autoState.inlineBg, '', 'auto theme must clear inline background');
  assert(autoState.inlineWidth.length > 0 && autoState.inlineHeight.length > 0, 'auto theme must preserve viewport SVG sizing');
  assert.equal(autoState.computedSdBg, initialAutoBg, 'auto theme must restore light site --sd-bg');
  await page.click('#theme-toggle');
  const darkAutoBg = await page.evaluate(() => getComputedStyle(document.querySelector('#home-editor-canvas svg')).getPropertyValue('--sd-bg').trim());
  assert.notEqual(darkAutoBg, initialAutoBg, 'auto theme SVG must respond to site dark theme toggle');
  await page.click('#theme-toggle');

  // Verify statechart transitions synchronize #home-sim-rate and #home-sim-rate-val
  const initialMetrics = await page.locator('#home-sim-metrics').textContent();
  await page.click('#home-sim-step');
  await page.waitForFunction(prev => document.querySelector('#home-sim-metrics')?.textContent !== prev, initialMetrics);
  assert.match(await page.locator('#home-sim-metrics').textContent(), /workers/);
  const probeBtn = page.locator('#home-sim-events button:not([disabled])', { hasText: /^probe\b/ });
  for (let attempt = 0; attempt < 8 && (await probeBtn.count()) === 0; attempt++) {
    await page.click('#home-sim-step');
  }
  assert.equal(await probeBtn.count(), 1, 'breaker must reach Open state with enabled probe action');
  await probeBtn.click();
  assert.equal(await page.locator('#home-sim-rate').inputValue(), '30', 'probe action must update ingress slider to 30');
  assert.equal(await page.locator('#home-sim-rate-val').textContent(), '30/s', 'probe action must update ingress label to 30/s');
  const stabilizedBtn = page.locator('#home-sim-events button:not([disabled])', { hasText: /^stabilized\b/ });
  await stabilizedBtn.click();
  assert.equal(await page.locator('#home-sim-rate').inputValue(), '55', 'stabilized action must update ingress slider to 55');
  assert.equal(await page.locator('#home-sim-rate-val').textContent(), '55/s', 'stabilized action must update ingress label to 55/s');
  await page.click('#home-sim-reset');
  assert.equal(await page.locator('#home-sim-rate').inputValue(), '95', 'reset must restore ingress slider to 95');
  assert.equal(await page.locator('#home-sim-rate-val').textContent(), '95/s', 'reset must restore ingress label to 95/s');

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
        assert(Number.parseInt(await group.locator('output').textContent())>=70,'initial desktop diagram scale must remain readable');
        assert(Number.parseInt(await group.locator('output').textContent())<=100,'initial Fit must not enlarge small diagrams');
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
  assert.equal(diagrams,70,'all language and gallery choices must be checked');
  await page.goto(url('language/reference.html'));
  const dense=page.locator('.diagram-examples').last();
  await page.setViewportSize({width:375,height:812});
  await dense.getByRole('button',{name:'Actual size (100%)',exact:true}).click();
  const pan=dense.locator('.diagram-viewport');await pan.focus();await pan.press('ArrowRight');
  await page.waitForFunction(()=>[...document.querySelectorAll('.diagram-viewport')].some(el=>el.scrollLeft>0));
  await dense.getByRole('button',{name:'Zoom in',exact:true}).click();assert.equal(await dense.locator('output').textContent(),'125%');
  await dense.getByRole('button',{name:'Fit',exact:true}).click();assert(await dense.locator('img').evaluate(img=>img.getBoundingClientRect().width<=img.closest('.diagram-viewport').clientWidth+1));
  await dense.getByRole('button',{name:'Actual size (100%)',exact:true}).click();assert.equal(await dense.locator('output').textContent(),'100%');
  assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'viewer must contain mobile overflow');
  await page.setViewportSize({width:1440,height:1000});
  console.log(`Examples: ${diagrams} language/gallery choices passed glyph, leader, contrast, silhouette, arrow, description, comparison, grid, and mobile zoom/pan checks.`);

  await page.goto(url('guide/getting-started.html'));

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
  await require('./site-usability-test.cjs')(browser,page,base.href);
  await require('./round-two-test.cjs')(browser,base.href);
  assert.deepEqual(errors, []);
  await browser.close();
  const result = spawnSync(process.execPath, ['scripts/browser-test.cjs'], { env: { ...process.env, DIAGRAM_TEST_URL: url('demo/') }, stdio: 'inherit' });
  assert.equal(result.status, 0, 'authoring and spatial smoke tests');
  console.log(`Passed desktop/mobile documentation, search, deep links, Rust API, embedded editor, example loading, SVG download and live runtime checks at ${base.href}`);
})().catch(error => { console.error(error); process.exit(1); });
