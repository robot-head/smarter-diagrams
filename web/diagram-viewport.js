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
    const hint=document.createElement('span');hint.textContent='Scroll to pan';hint.className='diagram-pan-hint';controls.append(hint);
    this.observer=new ResizeObserver(()=>{if(this.fitted)this.fit()});this.observer.observe(viewport);this.apply();
  }
  attach(target,bounds) {this.target=target;this.bounds=bounds||target.getAttribute('viewBox').split(/\s+/).map(Number);this.fitted?this.fit():this.apply()}
  setZoom(value){this.fitted=false;this.zoom=Math.max(.05,Math.min(4,value));this.apply()}
  fit(){if(!this.bounds)return;this.zoom=Math.min(Math.max(1,this.viewport.clientWidth-32)/this.bounds[2],440/this.bounds[3]);this.apply()}
  apply(){this.output.textContent=`${Math.round(this.zoom*100)}%`;if(!this.target||!this.bounds)return;Object.assign(this.target.style,{width:`${this.bounds[2]*this.zoom}px`,height:`${this.bounds[3]*this.zoom}px`,maxWidth:'none',maxHeight:'none',display:'block',margin:'auto'});}
}
