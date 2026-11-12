const assert=require('node:assert/strict');
function checkChangeInk(svg){
  const failures=[];
  for(const decoration of svg.querySelectorAll('[data-change]')){
    const node=decoration.closest('[data-node]'),body=node?.querySelector('[data-silhouette]');
    if(!node||node.dataset.node!==decoration.dataset.change){failures.push('change decoration escaped the node opacity group');continue;}
    for(const path of decoration.querySelectorAll('path')){
      const length=path.getTotalLength();for(let i=0;i<=100;i++)if(!body.isPointInStroke(path.getPointAtLength(length*i/100))){failures.push('change outline or hatch differs from silhouette');break;}
    }
    const title=node.querySelector('text');if(title&&!(decoration.compareDocumentPosition(title)&Node.DOCUMENT_POSITION_FOLLOWING))failures.push('hatching paints over label');
  }
  for(const change of svg.querySelectorAll('[data-change]'))if(change.querySelector('[data-change-hatch]')&&!svg.querySelector('pattern[id$="removed-hatch"]'))failures.push('hatch pattern missing');
  return failures;
}
module.exports=async function visualRegressions(browser,page,base){
  const url=path=>new URL(path,base).href;
  await page.setViewportSize({width:1360,height:900});
  // Test painted occupancy glyphs and bars on the shapes and locked sizes from
  // the visual audit, rather than checking that an overlay element exists.
  await page.goto(base);await page.waitForFunction(()=>document.querySelector('#status')?.textContent.includes('revision'));
  const occupancy=await page.evaluate(async()=>{
    const{render_svg}=await import(new URL('./pkg/smarter_diagrams.js',location.href));
    const{paintQueueState}=await import(new URL('./simulation-overlay.js',location.href));
    const failures=[],shapes=['rect','actor','queue','cache','diamond','decision','database','sql_table'];
    for(const shape of shapes)for(const locked of [false,true])for(const nodeLabel of (locked?['Status']:['Status','Organization membership history service'])){
      const pin=locked?' @pin(grid: 0,0, lock: [w: 12g, h: 4g])':'';
      const fields=shape==='sql_table'?' { port id: uuid [side: W, pos: 1] }':'';
      const holder=document.createElement('div');holder.style.cssText='position:absolute;left:-10000px;visibility:hidden';
      holder.innerHTML=render_svg(`node: ${JSON.stringify(nodeLabel)} [shape: ${shape}] @sim(service_rate: 1, queue_cap: 10)${pin}${fields}\n`);document.body.append(holder);
      const svg=holder.querySelector('svg');paintQueueState(svg,{nodes:{node:{saturation:1,queued:10,dropped:1}}});
      const node=svg.querySelector('[data-node]'),body=node.querySelector('[data-silhouette]'),overlay=node.querySelector('[data-simulation-overlay]');
      const original=[...node.querySelectorAll('text')].filter(t=>!overlay.contains(t)).map(t=>t.getBBox());
      for(const ink of overlay.querySelectorAll('rect,text')){
        const b=ink.getBBox();
        for(let i=0;i<=4;i++)for(let j=0;j<=4;j++)if(!body.isPointInFill(new DOMPoint(b.x+b.width*i/4,b.y+b.height*j/4)))failures.push(`${shape}/${locked}: status ink outside shape`);
        if(original.some(t=>b.x<t.x+t.width&&b.x+b.width>t.x&&b.y<t.y+t.height&&b.y+b.height>t.y))failures.push(`${shape}/${locked}: status covers label`);
      }
      if(!locked&&shape!=='sql_table'&&!overlay.querySelector('text'))failures.push(`${shape}: missing readable occupancy caption`);
      if(locked&&Math.abs(body.getBBox().height-64)>.01)failures.push(`${shape}: size lock changed`);
      holder.remove();
    }
    return failures;
  });assert.deepEqual(occupancy,[],'occupancy must stay inside silhouettes and clear of labels');

  await page.goto(url('advanced.html'));await page.waitForFunction(()=>document.querySelector('#status')?.textContent.includes('ready'));
  const checkSvg=require('./check-rendered-svg.cjs');
  const longDecision=await page.evaluate(async()=>{const{default:init,render_svg}=await import(new URL('./pkg/smarter_diagrams.js',location.href));await init();return render_svg('node: "Approve international payment request?" [shape: diamond]\n')});
  assert.deepEqual(await page.evaluate(checkSvg,longDecision),[],'long decision title clearance');
  for(const angle of [0,.4,Math.PI/4]){
    const markup=await page.evaluate(async angle=>{const{stack_json}=await import(new URL('./pkg-runtime/smarter_diagrams.js',location.href));return JSON.parse(stack_json(document.querySelector('#strata').value,angle)).svg;},angle);
    assert.deepEqual(await page.evaluate(checkSvg,markup),[],`spatial heading/connector clearance at ${angle}`);
    const obstacle=await page.evaluate(async angle=>{const{stack_json}=await import(new URL('./pkg-runtime/smarter_diagrams.js',location.href));return JSON.parse(stack_json('strata [app, middleware, hardware]\nclient [stratum: app] @pin(grid: 15,0)\nleft [stratum: middleware] @pin(grid: 0,0)\nobstacle: Unrelated service [stratum: middleware] @pin(grid: 15,0)\nhost [stratum: hardware] @pin(grid: 15,0)\nclient -> host [relation: runs_on]\n',angle)).svg;},angle);
    assert.deepEqual(await page.evaluate(checkSvg,obstacle),[],`three-plane node/connector clearance at ${angle}`);
  }
  await page.locator('#stack-controls').getByRole('button',{name:'Fit diagram',exact:true}).click();
  assert(await page.locator('#stack').evaluate(el=>el.scrollHeight<=el.clientHeight+1),'Fit must contain the whole stage');
  assert(await page.locator('#stack-legend').evaluate(el=>!el.closest('.diagram-viewport')),'legend must stay outside the scrolling diagram');
  await page.locator('#stack').screenshot({path:'artifacts/spatial-heading-clearance.png'});

  // A union containing one removal and one addition exposes orphan endpoint
  // frames and rectangle-based hatch geometry that a node-opacity check misses.
  await page.locator('#proposal').fill('client -> api\napi -> worker\n');
  for(const tau of ['0','0.5','1']){
    await page.locator('#morph').fill(tau);await page.waitForFunction(value=>document.querySelector('#diff [data-node="worker"]')?.getAttribute('opacity')===value,tau);
    const issues=await page.locator('#diff svg').evaluate(checkChangeInk);assert.deepEqual(issues,[],`branch decoration geometry and layering at ${tau}`);
    if(tau!=='0.5')assert.equal(await page.locator('#diff [mask]').count(),0,'hidden routes must not cut gaps into visible connectors');
    await page.locator('#diff').screenshot({path:`artifacts/branch-endpoint-${tau}.png`});
  }
  const cases=await page.evaluate(async()=>{
    const{branch_diff_svg}=await import(new URL('./pkg-runtime/smarter_diagrams.js',location.href));
    return ['rect','actor','queue','cache','diamond','decision','database','sql_table'].flatMap(shape=>{
      const small='anchor\n',large=`anchor\nnode: Shape [shape: ${shape}]\nanchor -> node\n`;
      return [0,.5,1].flatMap(tau=>[{shape,tau,svg:branch_diff_svg(small,large,tau)},{shape,tau,svg:branch_diff_svg(large,small,tau)}]);
    });
  });
  for(const example of cases){
    await page.locator('#diff').evaluate((el,markup)=>el.innerHTML=markup,example.svg);
    assert.deepEqual(await page.locator('#diff svg').evaluate(checkChangeInk),[],`${example.shape} change ink at ${example.tau}`);
  }
  const modified=await page.evaluate(async()=>{
    const{branch_diff_svg}=await import(new URL('./pkg-runtime/smarter_diagrams.js',location.href));
    const a='client -> db\ndb: Ledger [shape: database]\n',b='client -> db\ndb: Accounts [shape: sql_table] { port id: uuid [side: W, pos: 1] }\n';
    return [[a,b],[b,a]].flatMap(([base,proposal])=>[0,.25,.5,.75,1].map(tau=>({tau,expected:tau<.5?base:proposal,svg:branch_diff_svg(base,proposal,tau)})));
  });
  for(const frame of modified){
    await page.locator('#diff').evaluate((el,markup)=>el.innerHTML=markup,frame.svg);
    const record=frame.expected.includes('sql_table');
    assert.equal(await page.locator('#diff [data-label="db"]').count(),record?1:0,'external title belongs to the endpoint shape');
    assert.equal(await page.locator('#diff [data-port="db#id"]').count(),record?1:0,'named port belongs to the endpoint snapshot');
    assert.equal((await page.locator('#diff desc').textContent()).includes('ports id:'),record,'description must match snapshot ports');
    assert.deepEqual(await page.locator('#diff svg').evaluate(checkChangeInk),[],'modified shape outline');
    assert.deepEqual(await page.evaluate(checkSvg,frame.svg),[],`modified shape attachments and labels at ${frame.tau}`);
  }
  // Touch gestures run in a touch-capable context; mouse-wheel scrolling would
  // not catch a touch-action:none regression.
  const context=await browser.newContext({viewport:{width:375,height:812},isMobile:true,hasTouch:true});
  const mobile=await context.newPage();await mobile.goto(base);await mobile.waitForFunction(()=>document.querySelector('#status')?.textContent.includes('revision'));
  const canvas=mobile.locator('#canvas'),source=await mobile.locator('#source').inputValue();await canvas.scrollIntoViewIfNeeded();
  await mobile.getByRole('button',{name:'Actual size (100%)',exact:true}).click();
  const cdp=await context.newCDPSession(mobile);
  async function swipe(x,y,dx,dy){
    await cdp.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{x,y}]});
    for(let i=1;i<=8;i++){await cdp.send('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[{x:x+dx*i/8,y:y+dy*i/8}]});await mobile.waitForTimeout(30);}
    await cdp.send('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});
  }
  const viewportBox=await canvas.boundingBox(),svgBox=await canvas.locator('svg').boundingBox();
  await swipe(viewportBox.x+viewportBox.width-35,svgBox.y+8,-160,0);
  await mobile.waitForFunction(()=>document.querySelector('#canvas').scrollLeft>80);
  assert.equal(await mobile.locator('#source').inputValue(),source,'touch panning must not edit source');
  await canvas.evaluate(el=>el.scrollLeft=0);
  await mobile.getByRole('button',{name:'Move nodes',exact:true}).click();
  const client=await mobile.locator('[data-node="client"]').boundingBox();await swipe(client.x+client.width/2,client.y+client.height/2,0,80);
  await mobile.waitForFunction(()=>document.querySelector('#source').value.includes('@pin'));
  await mobile.getByRole('button',{name:'Undo',exact:true}).click();assert.equal(await mobile.locator('#source').inputValue(),source);
  await mobile.getByRole('button',{name:'Move nodes',exact:true}).click();
  await mobile.screenshot({path:'artifacts/editor-touch-pan.png',fullPage:true});
  await mobile.goto(url('advanced.html'));await mobile.waitForFunction(()=>document.querySelector('#status')?.textContent.includes('ready'));
  for(const viewport of [{width:375,height:812},{width:667,height:375}]){
    await mobile.setViewportSize(viewport);await mobile.locator('#stack-controls').getByRole('button',{name:'Fit diagram',exact:true}).click();
    assert(await mobile.locator('#stack').evaluate(el=>el.scrollHeight<=el.clientHeight+1),'mobile Fit must account for the height limit');
    assert(await mobile.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'mobile viewer overflow');
  }
  await context.close();console.log('Visual regressions: shape-safe occupancy, touch pan/edit, heading clearance, branch overlays, endpoint fades and complete Fit/legend passed.');
};
