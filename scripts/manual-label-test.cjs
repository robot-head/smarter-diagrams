const assert=require('node:assert/strict');
module.exports=async function manualLabels(browser,base){
  const page=await browser.newPage({viewport:{width:1360,height:1000}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.clock.install();await page.goto(base);await page.waitForFunction(()=>document.querySelector('#status')?.textContent.includes('revision'));
  const original=await page.locator('#source').inputValue();
  const read=()=>page.locator('[data-label="client->gateway"]').evaluate(el=>({x:Number(el.dataset.labelX),y:Number(el.dataset.labelY)}));
  const home=await read();
  await page.evaluate(()=>{window.originalSvg=document.querySelector('#canvas svg');window.originalNode=document.querySelector('[data-node="client"]');window.originalLabel=document.querySelector('[data-label="client->gateway"]')});
  const start=async()=>{const p=await page.locator('[data-label="client->gateway"]').evaluate(el=>{const x=Number(el.dataset.labelX),y=Number(el.dataset.labelY),m=el.ownerSVGElement.getScreenCTM(),p=new DOMPoint(x+8,y+3).matrixTransform(m);return{x:p.x,y:p.y,wx:x,wy:y,s:m.a}});await page.mouse.move(p.x,p.y);await page.mouse.down();return p};
  let grab=await start();
  for(const dy of [-16.125,-24.25,-40.5,-24.25,-16.125]){
    await page.mouse.move(grab.x,grab.y+dy*grab.s);await page.clock.runFor(17);const p=await read();assert(Math.abs(p.x-home.x)<.001&&Math.abs(p.y-home.y-dy)<.001,'label follows the pointer in both directions without quantized steps');assert.equal(await page.locator('#source').inputValue(),original);
    assert(await page.evaluate(()=>originalSvg===document.querySelector('#canvas svg')&&originalNode===document.querySelector('[data-node="client"]')&&originalLabel===document.querySelector('[data-label="client->gateway"]')),'preview must retain SVG, node and caption DOM identity');
  }
  const held=await read();await page.mouse.up();assert.deepEqual(await read(),held);const saved=await page.locator('#source').inputValue();assert.notEqual(saved,original);
  const reopened=await page.evaluate(async source=>{const {WasmDocument}=await import(new URL('./pkg/smarter_diagrams.js',location.href));const d=new WasmDocument(source),root=new DOMParser().parseFromString(d.svg(),'image/svg+xml'),el=root.querySelector('[data-label="client->gateway"]');const p={x:Number(el.dataset.labelX),y:Number(el.dataset.labelY)};d.free();return p},saved);assert.deepEqual(reopened,held,'manual position survives reopening');
  await page.click('#undo');assert.equal(await page.locator('#source').inputValue(),original);assert.deepEqual(await read(),home);await page.click('#redo');assert.deepEqual(await read(),held);
  grab=await start();await page.mouse.move(grab.x,grab.y-20*grab.s);await page.clock.runFor(17);await page.keyboard.press('Escape');await page.mouse.up();assert.deepEqual(await read(),held);assert.equal(await page.locator('#source').inputValue(),saved);
  await page.locator('[data-label="client->gateway"]').dblclick();assert.deepEqual(await read(),home,'double-click restores automatic placement without moving nodes');
  await page.click('#undo');assert.deepEqual(await read(),held);await page.click('#redo');assert.deepEqual(await read(),home);
  // Once activated, movement stays precise when returning inside the initial
  // click threshold. Releasing there must retain that tiny final position.
  const node=page.locator('[data-node="client"] [data-silhouette]');const p=await node.evaluate(el=>{const b=el.getBBox(),m=el.ownerSVGElement.getScreenCTM(),p=new DOMPoint(b.x+b.width/2+18,b.y+b.height/2).matrixTransform(m);return{x:p.x,y:p.y,s:m.a}});await page.mouse.move(p.x,p.y);await page.mouse.down();await page.mouse.move(p.x,p.y+20*p.s);await page.clock.runFor(17);await page.mouse.move(p.x,p.y+1.25*p.s);await page.clock.runFor(17);await page.mouse.up();
  assert(await node.evaluate(el=>{const b=el.getBBox();return Math.abs(b.y+b.height/2-1.25)<.03}),'returning near the grab point must not cancel or snap');
  assert.deepEqual(errors,[]);await page.close();console.log('Manual labels: reversible fractional dragging, persistent SVG identity, source reopen, exact drop, Undo/Redo, Escape, double-click reset and return-to-origin precision passed.');
};
