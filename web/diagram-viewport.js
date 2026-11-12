// Start fitted. Manual zoom persists, and ordinary wheel gestures retain
// normal page scrolling and scroll chaining.
import {touchGestures} from './touch-gestures.js';
export class DiagramViewport {
  constructor(viewport, controls, label) {
    this.viewport=viewport;this.zoom=1;this.fitted=true;this.readable=true;
    viewport.classList.add('diagram-viewport');viewport.tabIndex=0;viewport.setAttribute('role','region');viewport.setAttribute('aria-label',`${label}. Ctrl or Command plus scroll to zoom. Use arrow keys or swipe to pan.`);
    controls.classList.add('diagram-controls');controls.setAttribute('role','group');controls.setAttribute('aria-label',`${label} viewing controls`);
    const button=(text,name,action)=>{const b=document.createElement('button');b.textContent=text;b.setAttribute('aria-label',name);b.addEventListener('click',action);controls.append(b);return b};
    button('−','Zoom out',()=>this.setZoom(this.zoom/1.25));
    this.output=document.createElement('output');this.output.setAttribute('aria-live','polite');this.output.setAttribute('aria-label','Diagram scale');button('','Actual size (100%)',()=>this.setZoom(1)).append(this.output);
    button('+','Zoom in',()=>this.setZoom(this.zoom*1.25));button('Fit','Fit diagram',()=>{this.fitted=true;this.readable=false;this.fit(true)});
    this.hint=document.createElement('span');this.hint.className='diagram-pan-hint';controls.append(this.hint);this.updateHint();
    touchGestures(viewport,{getZoom:()=>this.zoom,setZoom:value=>this.setZoom(value),enabled:()=>!viewport.classList.contains('touch-edit')});
    viewport.addEventListener('wheel',event=>{if(!event.ctrlKey&&!event.metaKey)return;event.preventDefault();this.setZoom(this.zoom*Math.exp(-Math.max(-100,Math.min(100,event.deltaY))*.005))},{passive:false});
    this.observer=new ResizeObserver(()=>{if(this.fitted)this.fit()});this.observer.observe(viewport);this.apply();
  }
  updateHint(moving=false){this.hint.textContent=moving?'Drag a node to move it':matchMedia('(pointer: coarse)').matches?'Pinch to zoom · drag to pan':'Ctrl/⌘ + scroll to zoom · swipe to pan'}
  lockFrame(bounds){this.frame=[...bounds];this.fitted=false;}
  attach(target,bounds) {
    this.target=target;const native=target.dataset.nativeViewBox||target.getAttribute('viewBox');this.naturalBounds=native?native.split(/\s+/).map(Number):bounds;
    this.bounds=this.frame||bounds||this.naturalBounds;
    if(native){target.dataset.nativeViewBox=this.naturalBounds.join(' ');target.setAttribute('viewBox',this.bounds.join(' '))}
    this.fitted?this.fit():this.apply();
  }
  setZoom(value){this.fitted=false;this.zoom=Math.max(.05,Math.min(4,value));this.apply()}
  fit(resetFrame=false){
    if(resetFrame){this.frame=undefined;this.bounds=this.naturalBounds;if(this.target?.hasAttribute('viewBox'))this.target.setAttribute('viewBox',this.bounds.join(' '))}
    if(!this.bounds)return;
    const style=getComputedStyle(this.viewport),number=value=>Number.parseFloat(value)||0;
    const width=this.viewport.clientWidth-number(style.paddingLeft)-number(style.paddingRight);
    const maxHeight=Number.parseFloat(style.maxHeight);
    const height=Number.isFinite(maxHeight)?maxHeight-number(style.paddingTop)-number(style.paddingBottom)-number(style.borderTopWidth)-number(style.borderBottomWidth):440;
    const floor=this.readable?(matchMedia('(pointer: coarse)').matches ? .55 : .7):.05;
    this.zoom=Math.max(floor,Math.min(1,Math.max(1,width)/this.bounds[2],Math.max(1,Math.min(440,height))/this.bounds[3]));this.apply();this.viewport.scrollTo(0,0);
  }
  apply(){this.output.textContent=`${Math.round(this.zoom*100)}%`;if(!this.target||!this.bounds)return;Object.assign(this.target.style,{width:`${this.bounds[2]*this.zoom}px`,height:`${this.bounds[3]*this.zoom}px`,maxWidth:'none',maxHeight:'none',display:'block',margin:'auto'});}
}
