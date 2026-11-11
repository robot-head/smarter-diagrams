const assert=require('node:assert/strict');
const fs=require('node:fs');
module.exports=async function dragStability(browser,base){
  const page=await browser.newPage({viewport:{width:1360,height:900}}),errors=[];
  page.on('pageerror',error=>errors.push(error.message));
  await page.clock.install({time:new Date('2026-11-11T10:00:00Z')});
  await page.goto(base);await page.waitForFunction(()=>document.querySelector('#status')?.textContent.includes('revision'));
  await page.clock.pauseAt(new Date('2026-11-11T10:01:00Z'));
  await page.evaluate(async()=>{
    const{WasmDragPreview}=await import(new URL('./pkg/smarter_diagrams.js',location.href));
    const settle=WasmDragPreview.prototype.settle;window.qaSettles=0;
    WasmDragPreview.prototype.settle=function(){window.qaSettles++;return settle.call(this)};
  });
  const read=()=>page.evaluate(()=>Object.fromEntries([...document.querySelectorAll('#canvas [data-node]')].map(node=>{
    const body=node.querySelector('[data-silhouette]'),b=body.getBBox(),screen=body.getBoundingClientRect();
    return[node.dataset.node,{x:b.x+b.width/2,y:b.y+b.height/2,screenX:screen.x,screenY:screen.y}];
  })));
  const start=async()=>{
    const point=await page.locator('[data-node="gateway"] [data-silhouette]').evaluate(body=>{
      const b=body.getBBox(),m=body.ownerSVGElement.getScreenCTM(),p=new DOMPoint(b.x+b.width/2+18,b.y+b.height/2+4).matrixTransform(m);return{x:p.x,y:p.y,scale:m.a};
    });await page.mouse.move(point.x,point.y);await page.mouse.down();return point;
  };
  const baseline=await read(),original=await page.locator('#source').inputValue(),grab=await start(),samples=[];
  for(const y of [2,3,4,5,6,7,8,9,10]){
    await page.mouse.move(grab.x+64.25*grab.scale,grab.y+y*grab.scale);await page.clock.runFor(17);
    const actual=await read();
    assert(Math.abs(actual.gateway.x-baseline.gateway.x-64.25)<.03,'horizontal movement must not snap back near a row');
    assert(Math.abs(actual.gateway.y-y)<.03,'preview must not round to 16px grid steps');
    for(const id of ['client','worker','db'])assert.deepEqual(actual[id],baseline[id],`unrelated ${id} must remain fixed in world and screen coordinates`);
    assert.equal(await page.locator('#source').inputValue(),original);
    samples.push({cursorY:y,actualY:actual.gateway.y});
  }
  assert.equal(await page.evaluate(()=>window.qaSettles),0,'active motion must postpone expensive routing');
  await page.clock.runFor(80);assert.equal(await page.evaluate(()=>window.qaSettles),0,'brief pauses must not reroute');
  await page.clock.runFor(80);assert.equal(await page.evaluate(()=>window.qaSettles),1,'one route settlement after the debounce');
  const settled=await read();for(const id of ['client','worker','db'])assert.deepEqual(settled[id],baseline[id]);
  assert(Math.abs(settled.gateway.x-baseline.gateway.x-64.25)<.03);assert(Math.abs(settled.gateway.y-10)<.03);
  await page.mouse.move(grab.x+64.25*grab.scale,grab.y+11*grab.scale);await page.clock.runFor(17);await page.keyboard.press('Escape');await page.mouse.up();
  await page.clock.runFor(1000);assert.equal(await page.evaluate(()=>window.qaSettles),1,'cancellation must clear delayed rerouting');
  assert.equal(await page.locator('#source').inputValue(),original);
  const snap=await start();await page.mouse.move(snap.x+64*snap.scale,snap.y+3*snap.scale);await page.clock.runFor(17);await page.mouse.up();
  const dropped=await read();assert(Math.abs(dropped.gateway.x-304)<.03,'row snapping on drop must preserve the horizontal coordinate');
  await page.locator('#undo').dispatchEvent('click');assert.equal(await page.locator('#source').inputValue(),original);
  await page.clock.runFor(1000);assert.equal(await page.evaluate(()=>window.qaSettles),1,'drop must clear delayed rerouting');
  assert.deepEqual(errors,[]);fs.mkdirSync('artifacts/stable-drag',{recursive:true});fs.writeFileSync('artifacts/stable-drag/jitter-samples.json',JSON.stringify(samples,null,2));
  await page.close();console.log('Drag stability: continuous sub-grid motion, fixed neighboring boxes/camera, debounced routing, canceled timers and stable row-snap drop passed.');
};
