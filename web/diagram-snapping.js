const ns='http://www.w3.org/2000/svg';
export class DiagramSnapping {
  constructor(canvas,controls){
    this.canvas=canvas;this.guides=[];this.locks={};
    this.menu=document.createElement('details');this.menu.className='snap-options';
    this.menu.innerHTML='<summary>Grid &amp; snap</summary><div class="snap-panel"><label><input type="checkbox" name="show-grid"> Show grid</label><label>Grid size <select aria-label="Grid spacing"><option value="8">8 px</option><option value="16" selected>16 px</option><option value="24">24 px</option><option value="32">32 px</option></select></label><label><input type="checkbox" name="snap-grid"> Snap to grid</label><label><input type="checkbox" name="smart-guides"> Align &amp; equal spacing</label><small>Hold Alt/Option to drag freely.</small></div>';
    controls.append(this.menu);
    const position=()=>{if(!this.menu.open)return;const panel=this.menu.querySelector('.snap-panel'),anchor=this.menu.getBoundingClientRect();panel.style.left=`${Math.max(8,Math.min(innerWidth-panel.offsetWidth-8,anchor.right-panel.offsetWidth))}px`;panel.style.top=`${Math.min(anchor.bottom+4,Math.max(8,innerHeight-panel.offsetHeight-8))}px`};
    this.menu.addEventListener('toggle',()=>{position();requestAnimationFrame(position)});
    window.addEventListener('resize',()=>requestAnimationFrame(position));
    window.addEventListener('scroll',position,true);
    this.menu.addEventListener('change',()=>{this.locks={};this.guides=[];this.draw()});
  }
  get size(){return Number(this.menu.querySelector('select').value)}
  enabled(name){return this.menu.querySelector(`[name="${name}"]`).checked}
  begin(id,label,box){
    this.label=label;this.box={w:box.width,h:box.height};this.locks={};this.guides=[];
    this.peers=[...this.canvas.querySelectorAll(label?'[data-draggable-label="true"]':'[data-node]')].filter(el=>{
      const peer=label?el.dataset.label:el.dataset.node;return peer!==id&&(label||!peer.startsWith(id+'.')&&!id.startsWith(peer+'.'));
    }).map(el=>{if(label)return{x:Number(el.dataset.labelX),y:Number(el.dataset.labelY),w:0,h:0};const b=el.querySelector('[data-silhouette]').getBBox();return{x:b.x+b.width/2,y:b.y+b.height/2,w:b.width,h:b.height}});
  }
  snap(point,event){
    this.guides=[];if(!this.enabled('smart-guides')&&!this.enabled('snap-grid'))return point;if(event.altKey){this.locks={};return point}
    const result={...point},scale=this.canvas.querySelector('svg').getScreenCTM().a,tolerance=6/scale;
    for(const axis of ['x','y']){
      const other=axis==='x'?'y':'x',size=axis==='x'?'w':'h',cross=axis==='x'?'h':'w',half=this.box[size]/2,candidates=[];
      if(this.enabled('smart-guides')){
        for(const p of this.peers)for(const fraction of (this.label?[0]:[-.5,0,.5])){
          const value=p[axis]+fraction*p[size]-fraction*this.box[size];
          candidates.push({value,priority:fraction===0?0:1,kind:'align',axis,coordinate:p[axis]+fraction*p[size],from:p[other]-p[cross]/2,to:point[other]+this.box[cross]/2,text:fraction===0?'Centers aligned':'Edges aligned'});
        }
        if(!this.label){
          const row=this.peers.filter(p=>Math.abs(p[other]-point[other])<tolerance+(p[cross]+this.box[cross])/2).sort((a,b)=>a[axis]-b[axis]);
          for(let i=0;i+1<row.length;i++){
            const a=row[i],b=row[i+1],left=a[axis]+a[size]/2,right=b[axis]-b[size]/2,gap=right-left;
            if(gap<0)continue;
            const between=(gap-this.box[size])/2;
            if(between>=0)candidates.push({value:(left+right)/2,kind:'space',axis,text:`Equal gaps: ${Math.round(between)} px`,pairs:[[left,(left+right)/2-half],[(left+right)/2+half,right]],cross:point[other]+this.box[cross]/2+12});
            for(const direction of [-1,1]){
              const value=direction===1?b[axis]+b[size]/2+gap+half:a[axis]-a[size]/2-gap-half;
              candidates.push({value,kind:'space',axis,text:`Equal gaps: ${Math.round(gap)} px`,pairs:[[left,right],direction===1?[b[axis]+b[size]/2,value-half]:[value+half,a[axis]-a[size]/2]],cross:point[other]+this.box[cross]/2+12});
            }
          }
        }
      }
      const old=this.locks[axis];let hit=old&&candidates.find(c=>c.kind===old.kind&&Math.abs(c.value-old.value)<1e-6&&Math.abs(c.value-point[axis])<tolerance*1.7);
      if(!hit)hit=candidates.filter(c=>Math.abs(c.value-point[axis])<=tolerance).sort((a,b)=>Math.abs(a.value-point[axis])-Math.abs(b.value-point[axis])||(a.priority??2)-(b.priority??2))[0];
      this.locks[axis]=hit;if(hit){result[axis]=hit.value;this.guides.push(hit)}else if(this.enabled('snap-grid'))result[axis]=Math.round(point[axis]/this.size)*this.size;
    }
    return result;
  }
  clear(){const visible=this.guides.length;this.guides=[];this.locks={};if(visible)this.draw()}
  draw(){
    const svg=this.canvas.querySelector('svg');if(!svg)return;const previous=svg.querySelector('[data-authoring-overlay]'),previousGuides=svg.querySelector('[data-authoring-overlay-guides]');
    if(!this.enabled('show-grid')&&!this.guides.length){previous?.remove();previousGuides?.remove();return}
    const make=(tag,attrs,parent)=>{const e=document.createElementNS(ns,tag);for(const [k,v]of Object.entries(attrs))e.setAttribute(k,v);parent?.append(e);return e};
    const group=make('g',{'data-authoring-overlay':'true','pointer-events':'none','aria-hidden':'true'});
    if(this.enabled('show-grid')){
      const size=this.size,pattern=make('pattern',{id:'sd-authoring-grid',width:size,height:size,patternUnits:'userSpaceOnUse'},make('defs',{},group));
      make('circle',{cx:0,cy:0,r:1,fill:'var(--sd-muted,#73869b)',opacity:.45},pattern);
      const [x,y,w,h]=svg.getAttribute('viewBox').split(/\s+/).map(Number);make('rect',{x,y,width:w,height:h,fill:'url(#sd-authoring-grid)'},group);
    }
    // Guides render above the diagram, with screen-readable hairlines.
    const ink=make('g',{'data-authoring-overlay-guides':'true','data-snap-guides':'true','pointer-events':'none','aria-hidden':'true',stroke:'var(--sd-status,#176e62)','stroke-width':1,'vector-effect':'non-scaling-stroke'});
    for(const g of this.guides){
      if(g.kind==='align'){
        const a=Math.min(g.from,g.to)-24,b=Math.max(g.from,g.to)+24;
        make('line',g.axis==='x'?{x1:g.coordinate,x2:g.coordinate,y1:a,y2:b,'stroke-dasharray':'5 4'}:{y1:g.coordinate,y2:g.coordinate,x1:a,x2:b,'stroke-dasharray':'5 4'},ink);
      }else for(const [a,b]of g.pairs){
        const horizontal=g.axis==='x';make('line',horizontal?{x1:a,x2:b,y1:g.cross,y2:g.cross}:{y1:a,y2:b,x1:g.cross,x2:g.cross},ink);
        for(const v of [a,b])make('line',horizontal?{x1:v,x2:v,y1:g.cross-4,y2:g.cross+4}:{y1:v,y2:v,x1:g.cross-4,x2:g.cross+4},ink);
        const label=make('text',{x:horizontal?(a+b)/2:g.cross+8,y:horizontal?g.cross-6:(a+b)/2,fill:'var(--sd-status,#176e62)',stroke:'var(--sd-bg,#ffffff)','stroke-width':4,'paint-order':'stroke',style:'font-size:12px','text-anchor':horizontal?'middle':'start'},ink);label.textContent=`${Math.round((b-a)*10)/10} px`;
      }
    }
    // Preserve unchanged overlays so no-op clicks retain their DOM targets.
    if(!previous?.isEqualNode(group)){previous?.remove();svg.prepend(group)}
    if(!previousGuides?.isEqualNode(ink)){previousGuides?.remove();svg.append(ink)}

  }
}
