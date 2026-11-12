const assert=require('node:assert/strict'),fs=require('node:fs');
module.exports=async function dropContinuity(browser,base){
  const page=await browser.newPage({viewport:{width:1360,height:900}}),errors=[];
  page.on('pageerror',e=>errors.push(e.message));fs.mkdirSync('artifacts/drop-continuity',{recursive:true});
  await page.clock.install({time:new Date('2026-11-11T10:00:00Z')});
  await page.goto(base);await page.waitForFunction(()=>document.querySelector('#status')?.textContent.includes('revision'));
  await page.clock.pauseAt(new Date('2026-11-11T10:01:00Z'));
  const source=()=>page.locator('#source').inputValue();
  const read=()=>page.locator('#canvas').evaluate(canvas=>{
    const svg=canvas.querySelector('svg'),box=canvas.getBoundingClientRect();
    return{frame:svg.getAttribute('viewBox'),zoom:document.querySelector('#canvas-controls output').textContent,pan:[canvas.scrollLeft,canvas.scrollTop],canvas:[box.x,box.y,box.width,box.height],
      nodes:Object.fromEntries([...svg.querySelectorAll('[data-node]')].map(node=>{const body=node.querySelector('[data-silhouette]'),b=body.getBBox(),s=body.getBoundingClientRect();return[node.dataset.node,{x:b.x+b.width/2,y:b.y+b.height/2,screen:[s.x,s.y,s.width,s.height],paint:[getComputedStyle(body).opacity,getComputedStyle(node.querySelector('text.node-title')||body).fill]}]})),
      routes:[...svg.querySelectorAll('path.edge,[data-arrowhead]')].map(p=>[p.dataset.edge,p.getAttribute('d')]),
      labels:[...svg.querySelectorAll('[data-label]')].map(label=>{const t=label.querySelector('text'),b=t.getBBox();return[label.dataset.label,t.textContent,b.x,b.y,b.width,b.height]})};
  });
  const begin=async(id='gateway')=>{
    const grab=await page.locator(`[data-node="${id}"] [data-silhouette]`).evaluate(body=>{const b=body.getBBox(),m=body.ownerSVGElement.getScreenCTM(),x=b.x+b.width/2,y=b.y+b.height/2,p=new DOMPoint(x+18,y+4).matrixTransform(m);return{x:p.x,y:p.y,worldX:x,worldY:y,scale:m.a}});
    await page.mouse.move(grab.x,grab.y);await page.mouse.down();return grab;
  };
  const move=async(grab,x,y,frame=true)=>{await page.mouse.move(grab.x+(x-grab.worldX)*grab.scale,grab.y+(y-grab.worldY)*grab.scale);if(frame)await page.clock.runFor(17)};
  const original=await source(),baseline=await read();
  let grab=await begin();await move(grab,305.3,37.4);let held=await read();assert.equal(await source(),original);
  await page.clock.runFor(500);assert.deepEqual(await read(),held,'idle must not repair or change the visible layout');
  await page.screenshot({path:'artifacts/drop-continuity/held.png'});await page.mouse.up();assert.deepEqual(await read(),held,'mouse release must not change boxes, routes, labels, zoom, frame or canvas size');
  await page.screenshot({path:'artifacts/drop-continuity/dropped.png'});const saved=await source();assert.notEqual(saved,original);
  await page.clock.runFor(1000);assert.deepEqual(await read(),held,'no delayed post-drop re-layout');
  await page.mouse.move(5,5);assert.deepEqual(await read(),held,'hover redraw must preserve the camera and routes');
  const reopened=await page.evaluate(async text=>{
    const{WasmDocument}=await import(new URL('./pkg/smarter_diagrams.js',location.href));const d=new WasmDocument(text),svg=new DOMParser().parseFromString(d.svg(),'image/svg+xml').documentElement;d.free();return[...svg.querySelectorAll('path.edge,[data-arrowhead]')].map(p=>[p.dataset.edge,p.getAttribute('d')]);
  },saved);assert.deepEqual(reopened,held.routes,'saved text must reproduce the displayed routing without a reset');
  await page.click('#undo');assert.equal(await source(),original);const revision=await page.locator('#status').textContent();await page.click('#undo');assert.equal(await page.locator('#status').textContent(),revision,'one gesture adds one history item');
  await page.click('#redo');assert.deepEqual(await read(),held,'Redo restores the same valid diagram');await page.click('#undo');

  // A release arriving before its scheduled animation frame still flushes the
  // final pointer position, without a second solve in the commit path.
  grab=await begin();await move(grab,307.75,43.25,false);await page.mouse.up();const flushed=await read();assert(Math.abs(flushed.nodes.gateway.x-307.75)<.03&&Math.abs(flushed.nodes.gateway.y-43.25)<.03);await page.click('#undo');
  grab=await begin();await move(grab,305.3,137.4);const good=await read();await move(grab,480,0);assert(await page.locator('#canvas').evaluate(el=>el.classList.contains('invalid-drag')));assert.deepEqual(await read(),good,'invalid routing/overlap must retain the last valid drawing');assert.equal(await source(),original);
  await move(grab,305.3,137.4);assert.equal(await page.locator('#canvas.invalid-drag').count(),0,'the same gesture recovers without resetting');await page.mouse.up();assert.deepEqual(await read(),good);await page.click('#undo');
  grab=await begin();await move(grab,480,0);await page.mouse.up();assert.equal(await source(),original,'an invalid drop must not save source or poison history');assert.match(await page.locator('#status').textContent(),/Move canceled/);assert.deepEqual((await read()).nodes,baseline.nodes);assert.equal(await page.locator('.is-dragging,.diagram-dragging,.invalid-drag').count(),0);

  // Reproduce Client elbow/blue caption flicker with repeated subpixel jitter.
  grab=await begin('client');await move(grab,48,96);const stableClient=await read();
  for(let i=0;i<24;i++){
    await move(grab,48+(i%2)*.1,96+(i%2)*.1);const current=await read();
    const before=stableClient.labels.find(l=>l[0]==='client->gateway'),after=current.labels.find(l=>l[0]==='client->gateway');
    assert(before&&after);assert(Math.abs(before[2]-after[2])<1&&Math.abs(before[3]-after[3])<1,'Client caption must not alternate rails during tiny pointer motion');
  }
  await page.locator('#canvas').screenshot({path:'artifacts/drop-continuity/client-jitter-held.png'});
  const clientHeld=await read();await page.mouse.up();assert.deepEqual(await read(),clientHeld);await page.click('#undo');

  const history=await page.evaluate(async()=>{
    const{WasmDocument}=await import(new URL('./pkg/smarter_diagrams.js',location.href));const d=new WasmDocument(document.querySelector('#source').value),preview=d.begin_drag(),initial=preview.svg();
    for(const position of [{x:304,y:96},{x:100,y:200},{x:700,y:-100},{x:272,y:64},{x:240,y:0}])preview.update(JSON.stringify([{node:'gateway',position}]));
    const returned=preview.svg();preview.free();d.free();return initial===returned;
  });assert(history,'reversals and loops must not accumulate stretched routes');

  const obstacle=await page.evaluate(async()=>{
    const{WasmDocument}=await import(new URL('./pkg/smarter_diagrams.js',location.href));
    const d=new WasmDocument('a @pin(grid: 0,0)\nb @pin(grid: 20,0)\nblock @pin(grid: 10,0)\na.E -> b.W: transit\nflow request { a -> b }\n'),preview=d.begin_drag();
    const read=()=>{const root=new DOMParser().parseFromString(preview.svg(),'image/svg+xml');const path=root.querySelector('path.edge'),text=root.querySelector('[data-label="a->b"] text');return {path:path.getAttribute('d').match(/-?\d+(?:\.\d+)?/g).map(Number),label:[Number(text.getAttribute('x')),Number(text.getAttribute('y'))]}};
    let first;
    for(let i=0;i<80;i++){
      preview.update(JSON.stringify([{node:'a',position:{x:0,y:i%2?-.1:.1}}]));const now=read();if(!first)first=now;
      if(now.path.length!==first.path.length||now.path.some((v,j)=>Math.abs(v-first.path[j])>1)||now.label.some((v,j)=>Math.abs(v-first.label[j])>1)){preview.free();d.free();return false}
    }
    const held=preview.svg();d.commit_drag(preview);const exact=d.svg()===held;preview.free();d.free();return exact;
  });assert(obstacle,'constrained detours and captions must resist jitter and commit the exact preview');

  const database=await page.evaluate(async()=>{
    const{WasmDocument}=await import(new URL('./pkg/smarter_diagrams.js',location.href));
    for(const fixed of [true,false]){
      const source='client @pin(grid: 0,0)\ngateway @pin(grid: 15,0)\nworker @pin(grid: 30,0)\ndb @pin(grid: 10,25)\nclient.E -> gateway.W: HTTPS\ngateway.E -> worker.W: jobs\n'+(fixed?'worker.E -> db.W':'worker -> db')+': writes\nflow request { client -> gateway -> worker -> db }\n';
      const d=new WasmDocument(source),preview=d.begin_drag();let first;
      for(let i=0;i<80;i++){
        const delta=i%2?.1:-.1;preview.update(JSON.stringify([{node:'db',position:{x:160+delta,y:400+delta}}]));
        const root=new DOMParser().parseFromString(preview.svg(),'image/svg+xml'),path=root.querySelector('path.edge[data-edge="worker->db"]'),label=root.querySelector('[data-label="worker->db"] text');
        const now={path:path.getAttribute('d').match(/-?\d+(?:\.\d+)?/g).map(Number),label:[Number(label.getAttribute('x')),Number(label.getAttribute('y'))]};
        if(first&&(now.path.length!==first.path.length||now.path.some((v,j)=>Math.abs(v-first.path[j])>1)||now.label.some((v,j)=>Math.abs(v-first.label[j])>1))) {preview.free();d.free();return false;}
        first??=now;
      }
      const held=preview.svg();d.commit_drag(preview);if(d.svg()!==held){preview.free();d.free();return false;}preview.free();d.free();
    }return true;
  });assert(database,'backward Database drags must keep corridors and captions steady with fixed and automatic ports');

  // A manual camera frame must not crop the exported diagram, and explicit
  // Fit must use the complete current drawing rather than that frozen frame.
  grab=await begin('client');await move(grab,-120,160);held=await read();await page.mouse.up();assert.deepEqual(await read(),held);
  const clientSaved=await source();grab=await begin();await move(grab,480,0);await page.mouse.up();assert.equal(await source(),clientSaved);assert.deepEqual(await read(),held,'canceling a later move preserves the saved camera and natural extent');
  const exported=await page.evaluate(async()=>{const{standaloneSvg}=await import(new URL('./svg-download.js',location.href));return standaloneSvg(document.querySelector('#canvas svg'))});
  assert.deepEqual(await page.evaluate(require('./check-rendered-svg.cjs'),exported),[],'export includes ink outside the original camera frame');
  await page.getByRole('button',{name:'Fit diagram',exact:true}).click();assert(await page.locator('#canvas').evaluate(el=>el.scrollWidth<=el.clientWidth+1));assert.notEqual((await read()).frame,held.frame,'Fit must reframe using natural bounds');
  await page.click('#undo');

  // Layout gestures do not restart a running queue model or erase its state.
  await page.evaluate(()=>{const post=Worker.prototype.postMessage;window.qaOpens=0;Worker.prototype.postMessage=function(data,...args){if(data.type==='open')window.qaOpens++;return post.call(this,data,...args)}});
  await page.click('#simulate');const opens=await page.evaluate(()=>window.qaOpens);grab=await begin();await move(grab,305.3,137.4);held=await read();await page.mouse.up();assert.deepEqual(await read(),held);assert.equal(await page.evaluate(()=>window.qaOpens),opens,'dropping must not reopen/reset the simulation');assert.equal(await page.locator('#simulate').getAttribute('aria-pressed'),'true');await page.click('#simulate');
  assert.deepEqual(errors,[]);fs.writeFileSync('artifacts/drop-continuity/verified.json',JSON.stringify({url:base,held,dropped:held,source:saved,passed:true},null,2));await page.close();
  console.log('Drop continuity: exact fractional preview/commit, camera/canvas/labels/routes, pending frames, invalid-drop recovery, source reload, route reversals, full export/Fit and uninterrupted simulation passed.');
};
