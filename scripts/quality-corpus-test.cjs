const assert=require('node:assert/strict');
const fs=require('node:fs');
const checkSvg=require('./check-rendered-svg.cjs');
const metrics=require('./layout-audit-metrics.cjs');
const corpus=require('./layout-exploration-cases.cjs')();
for(const id of ['c0ffee77-parallel','abc12345-dag','14-long-labels','local-bypass'])corpus.cases.push({id,source:fs.readFileSync(require('node:path').join(__dirname,'../tests/fixtures/quality',id+'.diagram'),'utf8')});
module.exports=async(browser,base)=>{
 const page=await browser.newPage({viewport:{width:1440,height:1000}});
 await page.goto(base);await page.waitForFunction(()=>document.querySelector('#status').textContent.includes('revision'));
 const rendered=await page.evaluate(async cases=>{
  const {WasmDocument,layout_json}=await import(new URL('./pkg/smarter_diagrams.js',location.href));
  const out=[];
  for(const c of cases){const d=new WasmDocument(c.source),layout=JSON.parse(layout_json(c.source));
   const expected=layout.labels.map(l=>l.feature);
   const result={id:c.id,layout,svg:d.svg(),expected};
   if(['01-dispatch-collect','02-planar-mesh','03-dense-mesh','09-many-record-slots','10-many-named-ports','c0ffee01-branch-join','c0ffee01-scoped','08-cross-ownership','14-long-labels'].includes(c.id)){
    const node=Object.keys(layout.nodes).find(id=>!id.includes('.')&&!['left','right','west','east','central','orders','queue'].includes(id));
    const old=layout.nodes[node],drag=d.begin_drag();
    for(const dx of [.1,-.1,1,4])drag.update(JSON.stringify([{node,position:{x:old.x+dx,y:old.y+4}}]));
    const preview=drag.svg();d.commit_drag(drag);const assertion=d.svg()===preview;
    const reopened=JSON.parse(layout_json(d.source()));
    result.drag={node,old,exactRelease:assertion,reopened,source:d.source()};drag.free();
   }
   d.free();out.push(result);
  }return out;
 },corpus.cases);
 const minima={'01-dispatch-collect':0,'02-planar-mesh':0,'03-dense-mesh':4,'09-many-record-slots':0,'10-many-named-ports':0,'local-bypass':0};
 for(const r of rendered){
  assert.deepEqual(await page.evaluate(checkSvg,r.svg),[],r.id+' Wasm paint');
  const actual=await page.evaluate(markup=>{const svg=new DOMParser().parseFromString(markup,'image/svg+xml');return[...svg.querySelectorAll('[data-label]')].map(e=>e.dataset.label)},r.svg);
  assert.deepEqual(actual.slice().sort(),r.expected.slice().sort(),r.id+' every assigned caption must reach SVG');
  const m=metrics(r.layout);assert.equal(m.overlappingRoutePairs,0,r.id+' ambiguous shared shafts');assert(!m.mostRemoteCallout||m.mostRemoteCallout.centerToAnchorPx<=160.01,r.id+' detached caption');
  if(r.id.endsWith('-scoped'))assert.equal(m.properCrossings,0,r.id);
  if(r.id in minima){assert.equal(m.properCrossings,minima[r.id],r.id);assert.equal(m.overlappingRoutePairs,0,r.id);}
  if(r.drag){const {node,old,reopened,exactRelease}=r.drag;assert(exactRelease,r.id+' release must preserve exact preview');assert(Math.abs(reopened.nodes[node].x-old.x-4)<.01);assert(Math.abs(reopened.nodes[node].y-old.y-4)<.01);
   for(const[id,before]of Object.entries(r.layout.nodes)){if(id===node||['left','right','west','east','central'].includes(id))continue;const after=reopened.nodes[id];for(const key of ['x','y','w','h'])assert(Math.abs(after[key]-before[key])<.01,`${r.id}: changed neighbor ${id}.${key}`);}
  }
 }
 for(const id of ['09-many-record-slots','10-many-named-ports']){
  const c=corpus.cases.find(c=>c.id===id);await page.locator('#source').fill(c.source);await page.click('#render');await page.waitForFunction(()=>document.querySelector('#canvas [data-node="queue"],#canvas [data-node="orders"]'));
  await page.locator('#canvas').screenshot({path:`artifacts/quality-${id}-wasm.png`});
 }
 const parallel=rendered.find(r=>r.id==='c0ffee77-parallel');assert.equal(parallel.expected.length,15);assert(Math.abs(parallel.layout.nodes.n8.y-parallel.layout.nodes.n9.y)<.01);
 const ports=rendered.find(r=>r.id==='10-many-named-ports');
 for(let i=0;i<4;i++)for(const key of [`queue#in${i}`,`queue#ack${i}`])assert(ports.expected.includes(key),`missing port ${key}`);
 assert.equal(rendered.find(r=>r.id==='08-cross-ownership').svg.match(/data-zone-label=/g).length,1);
 fs.writeFileSync('artifacts/wasm-quality-results.json',JSON.stringify(rendered.map(r=>({id:r.id,metrics:metrics(r.layout),captions:r.expected.length,drag:r.drag?{exactRelease:r.drag.exactRelease,node:r.drag.node}:undefined})),null,2));
 await page.close();console.log('Wasm quality: all 43 mixed-feature/regression sources, caption emission, graph crossing minima, local labels, scope islands, and nine drag/reopen cases passed.');
};
