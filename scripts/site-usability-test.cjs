const assert=require('node:assert/strict');
const fs=require('node:fs');
module.exports=async function siteUsability(browser,page,base){
  const url=path=>new URL(path,base).href;
  fs.mkdirSync('artifacts/visual-qa-fixed',{recursive:true});
  await page.setViewportSize({width:1440,height:1000});
  for(const theme of ['light','dark']){
    await page.goto(base);await page.evaluate(theme=>localStorage.setItem('vitepress-theme-appearance',theme),theme);
    for(const path of ['', 'guide/getting-started.html','examples.html','playground.html']){
      await page.goto(url(path));await page.waitForFunction(theme=>document.documentElement.dataset.theme===theme,theme);
      const themeSwitch=page.locator('.VPNavBarAppearance .VPSwitchAppearance');
      assert(await themeSwitch.getAttribute('aria-label'),'theme switch needs an explicit label');
      assert(await page.getByRole('link',{name:'GitHub repository',exact:true}).first().count(),'repository icon label');
      if(path==='playground.html'){
        const frame=await(await page.locator('iframe').elementHandle()).contentFrame();await frame.waitForFunction(()=>document.querySelector('#status')?.textContent.includes('revision'));
        assert.equal(await frame.locator('html').getAttribute('data-theme'),theme,'embedded editor theme');
        assert(await frame.locator('#canvas').evaluate(el=>el.scrollWidth<=el.clientWidth+1),'initial embedded editor must fit');
        assert.equal(await frame.locator('#canvas-controls button').count(),5,'one percentage control, minus, plus, Fit and Move nodes');
        if(theme==='dark')assert.notEqual(await frame.locator('#canvas').evaluate(el=>getComputedStyle(el).backgroundColor),'rgb(255, 255, 255)');
      } else {
        for(const image of await page.locator('.diagram-example img').all()){
          await image.evaluate(el=>el.loading='eager');await image.evaluate(el=>el.decode());
          assert.equal((await image.getAttribute('src')).endsWith('-dark.svg'),theme==='dark','static figure theme must follow the site toggle');
        }
        if(theme==='dark'&&path==='examples.html'){
          const checkSvg=require('./check-rendered-svg.cjs');
          for(const image of await page.locator('.diagram-example img').all()){
            const markup=await(await page.request.get(new URL(await image.getAttribute('src'),base).href)).text();
            assert.deepEqual(await page.evaluate(checkSvg,markup),[],'dark example contrast, geometry and bounds');
          }
        }
      }
      await page.screenshot({path:`artifacts/visual-qa-fixed/${theme}-${path.replace(/\W/g,'-')||'home'}.png`,fullPage:true});
    }
    await page.goto(url('api/smarter_diagrams/index.html'));await page.locator('#sd-docs-return').waitFor();
    assert.equal(await page.locator('html').getAttribute('data-theme'),theme,'Rustdoc theme must match the guide');
    await page.screenshot({path:`artifacts/visual-qa-fixed/${theme}-rustdoc.png`,fullPage:false});
    await page.getByRole('link',{name:'← Smarter Diagrams documentation',exact:true}).click();
    await page.waitForFunction(theme=>document.documentElement.dataset.theme===theme,theme);
  }
  await page.goto(url('examples.html'));const figure=page.locator('.diagram-examples').first(),viewport=figure.locator('.diagram-viewport');
  await viewport.scrollIntoViewIfNeeded();const box=await viewport.boundingBox();await page.mouse.move(box.x+box.width/2,box.y+box.height/2);
  const before=await page.evaluate(()=>scrollY),scale=await figure.locator('output').textContent();await page.mouse.wheel(0,220);
  await page.waitForFunction(before=>scrollY>before,before);assert.equal(await figure.locator('output').textContent(),scale,'ordinary wheel must not zoom');
  const wheel=await viewport.evaluate(el=>{
    const plain=new WheelEvent('wheel',{deltaY:-40,cancelable:true,bubbles:true});el.dispatchEvent(plain);
    const modified=new WheelEvent('wheel',{deltaY:-40,ctrlKey:true,cancelable:true,bubbles:true});el.dispatchEvent(modified);
    return{plain:plain.defaultPrevented,modified:modified.defaultPrevented};
  });assert.deepEqual(wheel,{plain:false,modified:true});assert.notEqual(await figure.locator('output').textContent(),scale,'modifier wheel must zoom');

  for(const width of [320,375,767,820,960,1100]){
    await page.setViewportSize({width,height:900});
    await page.goto(url('guide/getting-started.html'));
    assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),`guide overflow at ${width}`);
    if(width<960){const menu=page.getByRole('button',{name:'mobile navigation',exact:true});await menu.click();await page.locator('#VPNavScreen').waitFor();assert(await page.locator('#VPNavScreen').getByRole('link',{name:'Playground',exact:true}).isVisible());await menu.click();}
    if(width===375){await page.goto(url('api/smarter_diagrams/index.html'));await page.locator('#sd-docs-return').waitFor();assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'mobile Rustdoc overflow');await page.screenshot({path:'artifacts/visual-qa-fixed/rustdoc-mobile.png',fullPage:false});}
    await page.goto(url('demo/'));await page.waitForFunction(()=>document.querySelector('#status')?.textContent.includes('revision'));
    assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),`editor overflow at ${width}`);
    assert(await page.locator('nav').evaluate(el=>el.scrollWidth<=el.clientWidth+1),`toolbar overflow at ${width}`);
    assert(await page.locator('#canvas').evaluate(el=>el.scrollWidth<=el.clientWidth+1),'initial editor Fit');
    if(width<768)assert.equal(await page.locator('main').evaluate(el=>getComputedStyle(el).flexDirection),'column');
    if(width===375||width===820)await page.screenshot({path:`artifacts/visual-qa-fixed/editor-${width}.png`,fullPage:true});
  }
  // Every source-history mutation must update the running model, not just SVG.
  await page.setViewportSize({width:1440,height:1000});await page.goto(url('demo/?example=simulation'));
  await page.waitForFunction(()=>document.querySelector('#status')?.textContent.includes('revision'));
  const source=await page.locator('#source').inputValue();await page.click('#simulate');
  assert.equal(await page.locator('#simulate').getAttribute('aria-pressed'),'true');
  await page.locator('#source').fill(source+'ghost: Temporary service @sim(service_rate: 50)\n');await page.click('#render');
  await page.waitForFunction(()=>document.querySelector('#metrics').textContent.includes('ghost:'));
  await page.click('#undo');await page.waitForFunction(()=>document.querySelector('#metrics').textContent.includes('worker:')&&!document.querySelector('#metrics').textContent.includes('ghost:'));
  assert.equal(await page.locator('[data-node="ghost"]').count(),0);assert.equal(await page.locator('#source').inputValue(),source);
  await page.screenshot({path:'artifacts/visual-qa-fixed/simulation-undo.png',fullPage:true});
  await page.click('#redo');await page.waitForFunction(()=>document.querySelector('#metrics').textContent.includes('ghost:'));
  assert.equal(await page.locator('[data-node="ghost"]').count(),1);await page.click('#simulate');
  await page.getByRole('button',{name:'Move nodes',exact:true}).click();assert.equal(await page.getByRole('button',{name:'Move nodes',exact:true}).getAttribute('aria-pressed'),'true');
  assert(await page.getByRole('button',{name:'Move nodes',exact:true}).locator('[aria-hidden=true]').count(),'active mode needs a non-color state icon');
  console.log('Site usability: light/dark figures and embeds, Rustdoc return/theme, initial Fit, modifier wheel, single zoom control, responsive navigation/toolbars and simulation Undo/Redo passed.');
};
