const assert=require('node:assert/strict');
const fs=require('node:fs');
module.exports=async function nodeDragTests(browser,page,base){
  fs.mkdirSync('artifacts/live-drag',{recursive:true});
  const ready=async p=>{await p.goto(base);await p.waitForFunction(()=>document.querySelector('#status')?.textContent.includes('revision'))};
  const geometry=(p,id='gateway')=>p.locator(`[data-node="${id}"] [data-silhouette]`).evaluate(body=>{
    const b=body.getBBox(),svg=body.ownerSVGElement,m=svg.getScreenCTM(),center={x:b.x+b.width/2,y:b.y+b.height/2};
    const point=new DOMPoint(center.x+18,center.y+4).matrixTransform(m);
    return{center,point:{x:point.x,y:point.y},scale:m.a,frame:svg.getAttribute('viewBox')};
  });
  const source=p=>p.locator('#source').inputValue();
  const waitMoved=(p,id,y)=>p.waitForFunction(({id,y})=>{const b=document.querySelector(`[data-node="${id}"] [data-silhouette]`).getBBox();return Math.abs(b.y+b.height/2-y)<1},{id,y});
  const begin=async(p,id='gateway')=>{const g=await geometry(p,id);await p.mouse.move(g.point.x,g.point.y);await p.mouse.down();return g};
  const frames=p=>p.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
  await page.setViewportSize({width:1360,height:900});await ready(page);
  const preview=await page.evaluate(async()=>{
    const{WasmDocument}=await import(new URL('./pkg/smarter_diagrams.js',location.href));
    const doc=new WasmDocument('a -> b\n');doc.drag(JSON.stringify([{node:'a',position:{x:0,y:80}}]));doc.undo();
    const source=doc.source(),revision=String(doc.revision()),svg=doc.preview_drag(JSON.stringify([{node:'a',position:{x:0,y:160}}]));
    const unchanged=source===doc.source()&&revision===String(doc.revision()),redo=doc.redo(),redone=doc.source();doc.free();return{unchanged,redo,redone,svg};
  });
  assert(preview.unchanged&&preview.redo,'preview must preserve document revision, source and redo history');assert.match(preview.redone,/@pin/);
  assert.deepEqual(await page.evaluate(require('./check-rendered-svg.cjs'),preview.svg),[],'preview uses the routed compiler output');

  const original=await source(page),initial=await begin(page),scale=await page.locator('#canvas-controls output').textContent();
  const route=await page.locator('path.edge').first().getAttribute('d');
  await page.mouse.move(initial.point.x+16*initial.scale,initial.point.y+48*initial.scale,{steps:5});await waitMoved(page,'gateway',48);
  assert.equal(await source(page),original,'pointer movement must not commit source');assert.notEqual(await page.locator('path.edge').first().getAttribute('d'),route,'connections reroute before release');
  await page.mouse.move(initial.point.x+32*initial.scale,initial.point.y+96*initial.scale,{steps:5});await waitMoved(page,'gateway',96);
  const held=await geometry(page);assert(Math.abs(held.center.x-initial.center.x-32)<1,'grab offset must not become a center jump');
  assert.deepEqual(held.frame.split(/\s+/).map(Number),initial.frame.split(/\s+/).map(Number),'camera frame stays stable during previews');assert.equal(await page.locator('#canvas-controls output').textContent(),scale,'preview must not refit the viewport');
  assert(await page.locator('[data-node="gateway"] [data-silhouette]').evaluate(body=>{const b=body.getBoundingClientRect();return document.elementFromPoint(b.x+b.width/2,b.y+b.height/2)?.closest('[data-node]')?.dataset.node==='gateway'}),'moved node remains painted outside the original SVG extents');
  assert.equal(await page.evaluate(()=>getSelection().toString()),'','drag must not select SVG labels');
  assert.equal(await page.locator('[data-node="gateway"] text').evaluate(el=>getComputedStyle(el).userSelect),'none');
  await page.locator('.canvas-panel').screenshot({path:'artifacts/live-drag/mouse-during.png'});
  await page.mouse.up();const committed=await source(page);assert.match(committed,/@pin/);const dropped=await geometry(page);assert.deepEqual(dropped,held,'drop must keep the exact preview and camera');
  await page.click('#undo');assert.equal(await source(page),original,'one undo restores the whole gesture');const revision=await page.locator('#status').textContent();
  await page.click('#undo');assert.equal(await page.locator('#status').textContent(),revision,'previews must not add hidden undo steps');
  await page.click('#redo');assert.equal(await source(page),committed);await page.click('#undo');

  for(const cancel of ['Escape','pointercancel']){
    if(cancel==='pointercancel')await page.locator('#canvas').evaluate(el=>el.addEventListener('pointerdown',event=>window.qaDragPointer=event.pointerId,{once:true}));
    const before=await begin(page);await page.mouse.move(before.point.x,before.point.y+64*before.scale,{steps:5});await waitMoved(page,'gateway',64);
    if(cancel==='Escape')await page.keyboard.press('Escape');
    else await page.locator('#canvas').evaluate(el=>el.dispatchEvent(new PointerEvent('pointercancel',{pointerId:window.qaDragPointer,isPrimary:true,bubbles:true})));
    await page.mouse.up();await frames(page);assert.equal(await source(page),original,`${cancel} must not commit`);
    assert(Math.abs((await geometry(page)).center.y)<1,`${cancel} restores original geometry`);
    assert.equal(await page.locator('.is-dragging,.diagram-dragging').count(),0,`${cancel} clears drag state`);
  }
  assert.notEqual(await page.locator('#source').evaluate(el=>getComputedStyle(el).userSelect),'none','source remains selectable after cancellation');
  await page.locator('#source').focus();await page.keyboard.press('ControlOrMeta+A');assert(await page.locator('#source').evaluate(el=>el.selectionEnd-el.selectionStart===el.value.length),'source text remains editable and selectable');
  await page.getByRole('button',{name:'Actual size (100%)',exact:true}).click();await page.getByRole('button',{name:'Zoom in',exact:true}).click();
  await page.locator('#canvas').evaluate(el=>el.scrollLeft=160);
  const pan=await page.locator('#canvas').evaluate(el=>el.scrollLeft),zoomed=await begin(page);
  await page.mouse.move(zoomed.point.x+32*zoomed.scale,zoomed.point.y+64*zoomed.scale,{steps:5});await waitMoved(page,'gateway',64);
  assert(Math.abs((await geometry(page)).center.x-272)<1,'scrolled/zoomed pointer coordinates preserve the grab offset');
  assert.equal(await page.locator('#canvas').evaluate(el=>el.scrollLeft),pan,'preview preserves horizontal pan');await page.mouse.up();await page.click('#undo');
  const outside=await begin(page),canvasBox=await page.locator('#canvas').boundingBox();
  await page.mouse.move(canvasBox.x-24,outside.point.y+96*outside.scale,{steps:5});await page.mouse.up();
  assert.notEqual(await source(page),original,'pointer capture commits a release outside the canvas');
  assert.equal(await page.locator('.is-dragging,.diagram-dragging').count(),0,'outside release clears capture state');
  await page.click('#undo');assert.equal(await source(page),original);

  const context=await browser.newContext({viewport:{width:375,height:812},isMobile:true,hasTouch:true});
  const mobile=await context.newPage(),errors=[];mobile.on('pageerror',e=>errors.push(e.message));await ready(mobile);
  await mobile.getByRole('button',{name:'Actual size (100%)',exact:true}).click();await mobile.getByRole('button',{name:'Move nodes',exact:true}).click();
  await mobile.locator('#canvas').scrollIntoViewIfNeeded();const touchSource=await source(mobile),cdp=await context.newCDPSession(mobile);
  for(const cancel of [false,true]){
    const g=await geometry(mobile,'client');
    await cdp.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[g.point]});
    await cdp.send('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[{x:g.point.x,y:g.point.y+48}]});await waitMoved(mobile,'client',48);
    assert.equal(await source(mobile),touchSource,'touch preview does not commit before release');
    await cdp.send('Input.dispatchTouchEvent',{type:cancel?'touchCancel':'touchEnd',touchPoints:[]});
    if(!cancel){await mobile.waitForFunction(()=>document.querySelector('#source').value.includes('@pin'));await mobile.click('#undo')}
    await frames(mobile);assert.equal(await source(mobile),touchSource,'touch cancellation/undo restores source');
    assert(Math.abs((await geometry(mobile,'client')).center.y)<1,'touch cancellation/undo restores geometry');
  }
  assert.deepEqual(errors,[]);await context.close();
  console.log('Live drag: held-pointer geometry/routing, stable camera, grab offsets, zoom/pan, single-step history, Escape/pointer cancellation, text selection and touch preview passed.');
};
