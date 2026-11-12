const assert = require('node:assert/strict');
const checkSvg = require('./check-rendered-svg.cjs');
module.exports = async (browser, base) => {
  const page = await browser.newPage({viewport:{width:1360,height:1000}});
  await page.goto(base);
  await page.waitForFunction(()=>document.querySelector('#status').textContent.includes('revision'));
  const mesh = 'a: Service A\nb: Service B\nc: Service C\nx: Store X [shape: database]\ny: Store Y [shape: database]\nz: Store Z [shape: database]\n' + ['a','b','c'].flatMap(a=>['x','y','z'].map(b=>`${a} -> ${b}: access\n`)).join('');
  await page.locator('#source').fill(mesh);await page.click('#render');
  await page.waitForFunction(()=>document.querySelectorAll('#canvas [data-node]').length===6);
  const result = await page.evaluate(async source=>{
    const {layout_json,WasmDocument}=await import(new URL('./pkg/smarter_diagrams.js',location.href));
    const l=JSON.parse(layout_json(source)),routes=Object.values(l.routes);
    const cross=(a,b,c,d)=>{
      if(Math.abs(a.y-b.y)<1e-6&&Math.abs(c.x-d.x)<1e-6)return c.x>Math.min(a.x,b.x)+1e-6&&c.x<Math.max(a.x,b.x)-1e-6&&a.y>Math.min(c.y,d.y)+1e-6&&a.y<Math.max(c.y,d.y)-1e-6;
      return Math.abs(a.x-b.x)<1e-6&&Math.abs(c.y-d.y)<1e-6?cross(c,d,a,b):false;
    };
    let crossings=0;for(let i=0;i<routes.length;i++)for(let j=i+1;j<routes.length;j++)for(const [k,p]of routes[i].points.entries())if(k)for(const [q,r]of routes[j].points.entries())if(q)crossings+=cross(routes[i].points[k-1],p,routes[j].points[q-1],r);
    const d=new WasmDocument(source),drag=d.begin_drag(),old=l.nodes.c;
    drag.update(JSON.stringify([{node:'c',position:{x:old.x,y:old.y+4}}]));d.commit_drag(drag);drag.free();const reopened=JSON.parse(layout_json(d.source()));d.free();
    const body=document.querySelector('#canvas [data-node="a"] [data-silhouette]').getBBox();
    const text=document.querySelector('#canvas [data-node="a"] text'),matrix=text.getScreenCTM();
    return {crossings,bodyHeight:body.height,nativeHeight:l.nodes.a.h,minimumStartingTextPx:parseFloat(getComputedStyle(text).fontSize)*matrix.a,reopened:reopened.nodes.c,old};
  },mesh);
  assert.equal(result.crossings,1,'the nonplanar three-by-three graph needs exactly one crossing');
  assert(Math.abs(result.bodyHeight-result.nativeHeight)<.01,'whole-source replacement must use fresh presentation');
  assert.equal(result.reopened.y,result.old.y+4);assert.equal(result.reopened.h,result.old.h);
  assert(result.minimumStartingTextPx>=10,'start at a readable detail scale');
  assert.deepEqual(await page.evaluate(checkSvg,await page.locator('#canvas svg').evaluate(svg=>svg.outerHTML)),[]);
  await page.locator('#canvas').screenshot({path:'artifacts/presentation-mesh-desktop.png'});
  await page.setViewportSize({width:375,height:900});
  assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));
  await page.locator('#canvas').screenshot({path:'artifacts/presentation-mesh-mobile.png'});
  await page.setViewportSize({width:1360,height:1000});
  const shortcuts='read -> validate -> normalize -> enrich -> filter -> aggregate -> encode -> store: data\nread -> filter: skip optional work\nnormalize -> store: cached result\n';
  await page.locator('#source').fill(shortcuts);await page.click('#render');
  await page.waitForFunction(()=>document.querySelectorAll('#canvas [data-node]').length===8);
  assert.deepEqual(await page.evaluate(checkSvg,await page.locator('#canvas svg').evaluate(svg=>svg.outerHTML)),[]);
  const shortcutCrossings=await page.evaluate(async source=>{
    const {layout_json}=await import(new URL('./pkg/smarter_diagrams.js',location.href));
    const routes=Object.values(JSON.parse(layout_json(source)).routes);
    const cross=(a,b,c,d)=>{
      if(Math.abs(a.y-b.y)<1e-6&&Math.abs(c.x-d.x)<1e-6)return c.x>Math.min(a.x,b.x)+1e-6&&c.x<Math.max(a.x,b.x)-1e-6&&a.y>Math.min(c.y,d.y)+1e-6&&a.y<Math.max(c.y,d.y)-1e-6;
      return Math.abs(a.x-b.x)<1e-6&&Math.abs(c.y-d.y)<1e-6?cross(c,d,a,b):false;
    };
    let count=0;for(let i=0;i<routes.length;i++)for(let j=i+1;j<routes.length;j++)for(let p=1;p<routes[i].points.length;p++)for(let q=1;q<routes[j].points.length;q++)count+=cross(routes[i].points[p-1],routes[i].points[p],routes[j].points[q-1],routes[j].points[q]);return count;
  },shortcuts);
  assert.equal(shortcutCrossings,0,'outside bypasses must not cross the main sequence');
  await page.locator('#canvas').screenshot({path:'artifacts/presentation-shortcuts-desktop.png'});
  await page.close();
  console.log('Presentation: one-crossing mesh, noncrossing shortcuts, editor replacement, readable initial scale, mobile containment, and Wasm drag/reopen passed.');
};
