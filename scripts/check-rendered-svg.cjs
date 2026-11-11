// Runs inside Chromium against painted SVG geometry and actual glyph bounds.
module.exports = function checkRenderedSvg(markup) {
  const svg = new DOMParser().parseFromString(markup, 'image/svg+xml').documentElement;
  const holder=document.createElement('div');holder.style.cssText='position:absolute;left:-10000px;visibility:hidden';holder.append(svg);document.body.append(holder);
  try {
    const failures=[];
    const close=(a,b)=>Math.hypot(a.x-b.x,a.y-b.y)<.03;
    const rect=box=>({left:box.x,top:box.y,right:box.x+box.width,bottom:box.y+box.height});
    const inside=(p,r)=>p.x>r.left+.05&&p.x<r.right-.05&&p.y>r.top+.05&&p.y<r.bottom-.05;
    const hits=(a,b,r)=>{let lo=0,hi=1;for(const[start,delta,min,max]of[[a.x,b.x-a.x,r.left,r.right],[a.y,b.y-a.y,r.top,r.bottom]]){if(Math.abs(delta)<1e-9){if(start<min||start>max)return false;}else{const x=(min-start)/delta,y=(max-start)/delta;lo=Math.max(lo,Math.min(x,y));hi=Math.min(hi,Math.max(x,y));}}return lo<=hi;};
    const rgb=value=>(value.match(/[\d.]+/g)||[]).slice(0,3).map(Number);
    const luminance=color=>color.map(v=>v/255).map(v=>v<=.04045?v/12.92:((v+.055)/1.055)**2.4).reduce((sum,v,i)=>sum+v*[.2126,.7152,.0722][i],0);
    const contrast=(a,b)=>{const x=luminance(a),y=luminance(b);return(Math.max(x,y)+.05)/(Math.min(x,y)+.05);};
    const outlines=[...svg.querySelectorAll('[data-silhouette]')];
    const attached=point=>outlines.some(outline=>outline.isPointInStroke(new DOMPoint(point.x,point.y)));
    const shafts=[...svg.querySelectorAll('path.edge,path.flow')].map(path=>({path,id:path.dataset.edge,color:getComputedStyle(path).stroke,endpoints:[path.getPointAtLength(0),path.getPointAtLength(path.getTotalLength())]}));
    const heads=[...svg.querySelectorAll('[data-arrowhead]')].map(head=>{const n=head.getAttribute('d').match(/-?\d+(?:\.\d+)?/g).map(Number);return{id:head.dataset.edge,tip:{x:n[0],y:n[1]},base:{x:(n[2]+n[4])/2,y:(n[3]+n[5])/2},color:getComputedStyle(head).fill};});
    for(const port of svg.querySelectorAll('[data-port]')){
      const style=getComputedStyle(port);
      if(port.dataset.unusedPort){if(style.stroke==='none'||Number(port.getAttribute('r'))>3)failures.push(`unused port mark missing or oversized: ${port.dataset.port}`);}
      else if(style.fill!=='rgba(0, 0, 0, 0)')failures.push(`visible connected port dot: ${port.dataset.port}`);
    }
    for(const head of heads){if(!attached(head.tip))failures.push(`arrow tip detached: ${head.id}`);if(!shafts.some(shaft=>shaft.id===head.id&&shaft.color===head.color&&shaft.endpoints.some(p=>close(p,head.base))))failures.push(`arrow/shaft join or color: ${head.id}`);}
    for(const shaft of shafts)for(const point of shaft.endpoints)if(!attached(point)&&!heads.some(head=>head.id===shaft.id&&close(point,head.base)))failures.push(`detached shaft: ${shaft.id}`);
    const labels=[...svg.querySelectorAll('[data-label]')];
    for(const line of svg.querySelectorAll('[data-callout]')){
      const a=line.getPointAtLength(0),b=line.getPointAtLength(line.getTotalLength());
      for(const label of labels){const text=label.querySelector('text');if(text&&hits(a,b,rect(text.getBBox())))failures.push(`callout enters text: ${label.dataset.label}`);}
      for(const flow of svg.querySelectorAll('path.flow')){const badge=flow.parentElement.querySelector('text');if(badge&&hits(a,b,rect(badge.getBBox()))&&(badge.compareDocumentPosition(line)&Node.DOCUMENT_POSITION_FOLLOWING))failures.push('callout paints over flow step number');}
    }
    for(const label of labels){
      if(label.dataset.knockout==='true')continue;
      const text=label.querySelector('text');if(!text)continue;const box=rect(text.getBBox());
      for(const shaft of shafts){const length=shaft.path.getTotalLength();for(let i=0;i<=length;i+=2)if(inside(shaft.path.getPointAtLength(i),box)){failures.push(`connector enters label: ${label.dataset.label}`);break;}}
    }
    for(const node of svg.querySelectorAll('[data-node]')){
      const body=node.querySelector('[data-silhouette]'),text=node.querySelector('text.node-title');if(!body)continue;
      if(node.dataset.shape==='queue'&&text){const box=body.getBBox();if(text.getBBox().x<box.x+36)failures.push(`queue title overlaps decoration: ${node.dataset.node}`);}
      const story=node.querySelector('[data-story-outline]');if(story){const length=story.getTotalLength();for(let i=0;i<=length;i+=Math.max(1,length/80))if(!body.isPointInStroke(story.getPointAtLength(i))){failures.push(`story silhouette differs: ${node.dataset.node}`);break;}}
      const style=getComputedStyle(body);
      if(!node.classList.contains('context-node')&&contrast(rgb(style.stroke),rgb(style.fill))<2.99)failures.push(`node outline contrast below 3:1: ${node.dataset.node}`);
      if(node.classList.contains('context-node')&&text&&contrast(rgb(getComputedStyle(text).fill),[255,255,255])<4.5)failures.push(`faded text contrast below 4.5:1: ${node.dataset.node}`);
    }
    if(!svg.querySelector('desc')?.textContent.includes('Connections:'))failures.push('structured SVG description missing');
    return [...new Set(failures)];
  } finally {holder.remove();}
};
