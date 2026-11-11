// Native-size text is the default. Fit is an explicit overview, with scroll
// panning and ordinary keyboard controls available at every zoom level.
export class DiagramViewport {
  constructor(viewport, controls, label) {
    this.viewport=viewport;this.zoom=1;this.fitted=false;
    viewport.classList.add('diagram-viewport');viewport.tabIndex=0;viewport.setAttribute('role','region');viewport.setAttribute('aria-label',`${label}. Scroll to pan; use the zoom controls to change scale.`);
    controls.classList.add('diagram-controls');controls.setAttribute('role','group');controls.setAttribute('aria-label',`${label} viewing controls`);
    const button=(text,name,action)=>{const b=document.createElement('button');b.textContent=text;b.setAttribute('aria-label',name);b.addEventListener('click',action);controls.append(b);};
    button('−','Zoom out',()=>this.setZoom(this.zoom/1.25));
    this.output=document.createElement('output');this.output.setAttribute('aria-live','polite');this.output.setAttribute('aria-label','Diagram scale');controls.append(this.output);
    button('+','Zoom in',()=>this.setZoom(this.zoom*1.25));button('100%','Actual size (100%)',()=>this.setZoom(1));button('Fit','Fit diagram',()=>{this.fitted=true;this.fit()});
    const hint=document.createElement('span');hint.textContent='Scroll or swipe to pan';hint.className='diagram-pan-hint';controls.append(hint);
    this.observer=new ResizeObserver(()=>{if(this.fitted)this.fit()});this.observer.observe(viewport);this.apply();
  }
  attach(target,bounds) {this.target=target;this.bounds=bounds||target.getAttribute('viewBox').split(/\s+/).map(Number);this.fitted?this.fit():this.apply()}
  setZoom(value){this.fitted=false;this.zoom=Math.max(.05,Math.min(4,value));this.apply()}
  fit(){
    if(!this.bounds)return;
    const style=getComputedStyle(this.viewport),number=value=>Number.parseFloat(value)||0;
    const width=this.viewport.clientWidth-number(style.paddingLeft)-number(style.paddingRight);
    const maxHeight=Number.parseFloat(style.maxHeight);
    const height=Number.isFinite(maxHeight)?maxHeight-number(style.paddingTop)-number(style.paddingBottom)-number(style.borderTopWidth)-number(style.borderBottomWidth):440;
    this.zoom=Math.min(Math.max(1,width)/this.bounds[2],Math.max(1,Math.min(440,height))/this.bounds[3]);this.apply();this.viewport.scrollTo(0,0);
  }
  apply(){this.output.textContent=`${Math.round(this.zoom*100)}%`;if(!this.target||!this.bounds)return;Object.assign(this.target.style,{width:`${this.bounds[2]*this.zoom}px`,height:`${this.bounds[3]*this.zoom}px`,maxWidth:'none',maxHeight:'none',display:'block',margin:'auto'});}
}
