const { chromium } = require('playwright');
(async () => {
  const browser = await chromium.launch({
    executablePath: process.env.CHROMIUM_PATH || '/usr/bin/chromium',
    headless: true, args: ['--no-sandbox'],
  });
  const page = await browser.newPage();
  await page.goto(process.env.DIAGRAM_TEST_URL || 'http://127.0.0.1:8765/web/');
  await page.waitForFunction(() => document.querySelector('#status').textContent.includes('revision'));
  const result = await page.evaluate(async () => {
    const { WasmDocument } = await import('/web/pkg/smarter_diagrams.js');
    let source = 'cluster {\n inner4: Short\n';
    for (let i = 0; i < 8; i++) source += ` inner${i} -> inner${i + 1}\n`;
    source += '}\n';
    for (let i = 0; i < 539; i++) source += `outside${i} -> outside${i + 1}\n`;
    let start = performance.now();
    const document = new WasmDocument(source), cold = performance.now() - start;
    const samples = [];
    // Warm the Wasm tiering compiler before measuring steady-state authoring.
    for (let i = 0; i < 39; i++) {
      start = performance.now();
      document.replace_source(document.revision(), source.replace('Short', 'A longer service'));
      if (i >= 30) samples.push(performance.now() - start);
      document.undo();
    }
    document.free(); samples.sort((a, b) => a - b);
    return { nodes: 550, cold_ms: cold, incremental_median_ms: samples[4], samples_ms: samples };
  });
  console.log(JSON.stringify({ ...result, browser: await browser.version() }, null, 2));
  await browser.close();
})().catch(error => { console.error(error); process.exit(1); });
