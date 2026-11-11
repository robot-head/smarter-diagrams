const assert=require('node:assert/strict');
const fs=require('node:fs');
module.exports=async function roundTwo(browser,base){
  const url=path=>new URL(path,base).href;
  const context=await browser.newContext({viewport:{width:1440,height:1000},permissions:['clipboard-read','clipboard-write']}),page=await context.newPage(),errors=[];
  page.on('pageerror',e=>errors.push(e.message));fs.mkdirSync('artifacts/round-two',{recursive:true});
  await page.goto(url('guide/getting-started.html'));
  const theme=page.locator('.VPNavBarAppearance .VPSwitchAppearance');
  if(await page.locator('html').getAttribute('data-theme')!=='dark')await theme.click();
  await page.waitForFunction(()=>localStorage.getItem('rustdoc-theme')==='dark'&&localStorage.getItem('rustdoc-use-system-theme')==='false');
  await page.goto(url('api/smarter_diagrams/index.html'));await page.locator('#sd-docs-return').waitFor();assert.equal(await page.locator('html').getAttribute('data-theme'),'dark');
  await page.getByRole('link',{name:'← Smarter Diagrams documentation',exact:true}).click();
  await page.setViewportSize({width:375,height:812});
  for(const mode of ['dark','light']){
    await page.getByRole('button',{name:'mobile navigation',exact:true}).click();const drawer=page.locator('#VPNavScreen');await drawer.waitFor();
    if(await page.locator('html').getAttribute('data-theme')!==mode)await drawer.locator('.VPSwitchAppearance').click();
    await page.waitForFunction(mode=>document.documentElement.dataset.theme===mode&&getComputedStyle(document.querySelector('#VPNavScreen')).opacity==='1',mode);
    assert(await drawer.locator('.VPSwitchAppearance').isVisible(),'mobile theme utility');assert(await drawer.getByRole('link',{name:'GitHub repository',exact:true}).isVisible(),'mobile repository utility');
    assert(await drawer.evaluate(el=>{const color=getComputedStyle(el).backgroundColor;return color!=='rgba(0, 0, 0, 0)'&&color!=='transparent'}),'drawer must have an opaque surface');
    await page.screenshot({path:`artifacts/round-two/mobile-nav-${mode}.png`});await page.getByRole('button',{name:'mobile navigation',exact:true}).click();
  }
  await page.goto(url('guide/getting-started.html'));await page.locator('.VPLocalNav button.menu').click();await page.locator('.VPBackdrop').waitFor({state:'visible'});
  assert(await page.evaluate(()=>Number(getComputedStyle(document.querySelector('.VPBackdrop')).zIndex)>Number(getComputedStyle(document.querySelector('.VPLocalNav')).zIndex)),'backdrop must cover sticky contents bar');
  assert(await page.evaluate(()=>!!document.elementFromPoint(innerWidth-5,90)?.closest('.VPBackdrop')),'sticky bar must not paint above the backdrop');
  await page.waitForFunction(()=>getComputedStyle(document.querySelector('.VPBackdrop')).opacity==='1'&&Math.abs(document.querySelector('.VPSidebar').getBoundingClientRect().x)<1);await page.screenshot({path:'artifacts/round-two/mobile-docs-drawer.png'});await page.mouse.click(370,200);
  await page.goto(url('reference/performance.html'));const table=page.locator('.table-scroll').filter({has:page.locator('table')});assert(await table.count()>0);
  const scrolled=await table.evaluateAll(tables=>tables.some(table=>{if(table.scrollWidth<=table.clientWidth)return false;table.scrollLeft=100;return table.scrollLeft>0}));assert(scrolled,'wide tables must be horizontally scrollable');
  assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));await table.first().scrollIntoViewIfNeeded();await page.screenshot({path:'artifacts/round-two/mobile-tables.png'});
  await page.setViewportSize({width:1440,height:1000});await page.goto(url('guide/getting-started.html'));
  const copy=page.locator('.vp-doc button.copy').first(),expectedCopy=await copy.evaluate(button=>button.parentElement.querySelector('pre').textContent.trim());await copy.click();await page.waitForFunction(()=>document.querySelector('.vp-doc button.copy')?.classList.contains('copied'));
  assert.equal((await page.evaluate(()=>navigator.clipboard.readText())).trim(),expectedCopy,'copy button must write the snippet');
  assert.equal(await page.locator('.vp-doc pre').evaluateAll(blocks=>blocks.filter(pre=>!pre.parentElement.querySelector(':scope > button')).length),0,'all documentation code blocks need a copy control');

  await page.goto(url('demo/'));await page.waitForFunction(()=>document.querySelector('#status')?.textContent.includes('revision'));
  const source=page.locator('#source');await source.fill('a\nb\n');await source.focus();await source.evaluate(el=>el.setSelectionRange(0,3));
  await page.keyboard.press('Tab');assert.equal(await source.inputValue(),'  a\n  b\n');await page.keyboard.press('Shift+Tab');assert.equal(await source.inputValue(),'a\nb\n');
  await page.keyboard.press('Escape');await page.keyboard.press('Tab');assert.notEqual(await page.evaluate(()=>document.activeElement?.id),'source','Escape then Tab exits the editor');
  await source.fill('a ->');await page.click('#render');assert.match(await page.locator('#status').textContent(),/line 1:5 — error: edge requires complete endpoints/);await page.screenshot({path:'artifacts/round-two/clean-error.png'});
  await source.fill(fs.readFileSync('tests/fixtures/multiline.diagram','utf8'));await page.click('#render');assert(await page.locator('#canvas tspan').count()>=12,'multiline node/container/record/port/edge/zone glyphs');
  const multiline=await page.locator('#canvas svg').evaluate(svg=>{const heading=svg.querySelector('[data-owner-node="prod"]').getBBox(),child=svg.querySelector('[data-node="prod.child"] [data-silhouette]').getBBox(),body=svg.querySelector('[data-node="record"] [data-silhouette]');return heading.y+heading.height<child.y-4&&[...svg.querySelectorAll('[data-node="record"] .port-label')].every(text=>{const b=text.getBBox();return [b.x,b.x+b.width].every(x=>[b.y,b.y+b.height].every(y=>body.isPointInFill(new DOMPoint(x,y))))})});assert(multiline,'multiline container headings and fields need reserved space');await page.screenshot({path:'artifacts/round-two/multiline.png'});
  const fixture=await page.evaluate(async()=>{
    const {render_svg}=await import(new URL('./pkg/smarter_diagrams.js',location.href));
    const declarations=Array.from({length:3},(_,i)=>`l${i} @pin(grid: 0,${i*15})\nr${i} @pin(grid: 40,${i*15})`).join('\n');
    const connections=Array.from({length:3},(_,i)=>Array.from({length:3},(_,j)=>`l${i}.E -> r${j}.W`).join('\n')).join('\n');
    const a=render_svg(declarations+'\n'+connections+'\n'),b=render_svg((declarations+'\n'+connections+'\n').replaceAll('l','left'));
    const host=document.createElement('div');host.style.cssText='position:absolute;left:-10000px';host.innerHTML=a+b;document.body.append(host);
    const ids=[...host.querySelectorAll('[id]')].map(el=>el.id),masks=host.querySelectorAll('mask').length;
    const local=[...host.querySelectorAll('svg')].every(svg=>[...svg.querySelectorAll('[mask]')].every(el=>svg.querySelector(`[id="${el.getAttribute('mask').slice(5,-1)}"]`)));
    host.remove();return{unique:new Set(ids).size===ids.length,masks,local};
  });assert(fixture.unique&&fixture.local&&fixture.masks>0,'multiple inline drawings need independent mask namespaces');
  await page.goto(url('demo/?example=ports'));await page.waitForFunction(()=>document.querySelector('#status')?.textContent.includes('revision'));
  const tiers=await page.locator('#canvas').evaluate(canvas=>Object.fromEntries([...canvas.querySelectorAll('[data-node]')].map(node=>{const b=node.querySelector('[data-silhouette]').getBBox();return[node.dataset.node,{x:b.x+b.width/2,y:b.y+b.height/2}]})));
  assert(tiers.worker.y>tiers.broker.y&&tiers.db.y===tiers.worker.y&&tiers.api.y===tiers.broker.y,'ports sample must use two readable tiers');
  await page.screenshot({path:'artifacts/round-two/record-tiers.png'});
  if(await page.locator('html').getAttribute('data-theme')!=='dark')await page.click('#theme-toggle');
  const download=page.waitForEvent('download');await page.click('#download');const file=await download,exported=fs.readFileSync(await file.path(),'utf8');
  const standalone=await page.evaluate(markup=>{
    const svg=new DOMParser().parseFromString(markup,'image/svg+xml').documentElement,bg=svg.querySelector('[data-export-background]');
    return{background:bg?.getAttribute('fill'),viewBox:svg.getAttribute('viewBox'),width:Number(bg?.getAttribute('width')),height:Number(bg?.getAttribute('height')),hasVariables:markup.includes('var(--sd-')};
  },exported);
  assert.equal(standalone.background,'#1b2029');assert(!standalone.hasVariables);const viewBox=standalone.viewBox.split(/\s+/).map(Number);assert(Math.abs(standalone.width-viewBox[2])<.001);assert(Math.abs(standalone.height-viewBox[3])<.001);fs.writeFileSync('artifacts/round-two/dark-export.svg',exported);
  const literalExport=await page.evaluate(async()=>{
    const{render_svg}=await import(new URL('./pkg/smarter_diagrams.js',location.href));const{standaloneSvg}=await import(new URL('./svg-download.js',location.href));
    const literal='var(--sd-bg,#ffffff) url(#diagram-title)',holder=document.createElement('div');holder.style.cssText='position:absolute;left:-10000px';holder.innerHTML=render_svg(`a: "${literal}"\n`);document.body.append(holder);
    const svg=new DOMParser().parseFromString(standaloneSvg(holder.querySelector('svg')),'image/svg+xml').documentElement;holder.remove();
    return{caption:svg.querySelector('text.node-title').textContent,description:svg.querySelector('desc').textContent,label:svg.querySelector('[data-node]').getAttribute('aria-label'),background:svg.querySelector('[data-export-background]').getAttribute('fill')};
  });assert.equal(literalExport.caption,'var(--sd-bg,#ffffff) url(#diagram-title)');assert.equal(literalExport.label,literalExport.caption);assert(literalExport.description.includes(literalExport.caption));assert.equal(literalExport.background,'#1b2029');
  await page.goto(url('demo/?example=architecture'));await page.waitForFunction(()=>document.querySelector('#status')?.textContent.includes('revision'));
  const zone=await page.locator('#canvas svg').evaluate(svg=>{const zone=svg.querySelector('[data-zone]');return ['prod.gateway','prod.worker','prod.broker'].every(id=>{const node=[...svg.querySelectorAll('[data-node]')].find(n=>n.dataset.node===id),b=node.querySelector('[data-silhouette]').getBBox();return !zone.isPointInFill(new DOMPoint(b.x+b.width/2,b.y+b.height/2))})});assert(zone,'zones must exclude unrelated services');
  await page.screenshot({path:'artifacts/round-two/scope-and-flow.png'});

  const mobileContext=await browser.newContext({viewport:{width:375,height:812},hasTouch:true,isMobile:true}),mobile=await mobileContext.newPage();mobile.on('pageerror',e=>errors.push(e.message));
  await mobile.goto(url('demo/'));await mobile.waitForFunction(()=>document.querySelector('#status')?.textContent.includes('revision'));assert(Number.parseInt(await mobile.locator('#canvas-controls output').textContent())>=55);assert.match(await mobile.locator('.diagram-pan-hint').textContent(),/Pinch to zoom/);
  await mobile.locator('#canvas').scrollIntoViewIfNeeded();const box=await mobile.locator('#canvas').boundingBox(),cdp=await mobileContext.newCDPSession(mobile),y=box.y+100,scale=Number.parseInt(await mobile.locator('#canvas-controls output').textContent());
  await cdp.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{x:100,y},{x:180,y}]});
  await cdp.send('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[{x:70,y},{x:210,y}]});await cdp.send('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});
  assert(Number.parseInt(await mobile.locator('#canvas-controls output').textContent())>scale*1.3,'pinch must zoom the diagram');
  await mobile.getByRole('button',{name:'Fit diagram',exact:true}).click();assert(await mobile.locator('#canvas').evaluate(el=>el.scrollWidth<=el.clientWidth+1),'Fit still shows the complete graph');await mobile.screenshot({path:'artifacts/round-two/mobile-fit.png'});
  assert.deepEqual(errors,[]);await mobileContext.close();await context.close();console.log('Round two: opaque mobile utilities/drawers, scrollable tables, actual theme/copy actions, editor indentation/escape/errors, scoped masks, two-tier records, standalone dark export, zone exclusions and readable pinch/Fit passed.');
};
