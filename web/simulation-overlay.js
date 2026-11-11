// Status ink uses the filled silhouette, including curved and sloping edges.
const overlaps=(a,b,gap=2)=>a.x<aRight(b)+gap&&aRight(a)>b.x-gap&&a.y<b.y+b.height+gap&&a.y+a.height>b.y-gap;
const aRight=b=>b.x+b.width;
function inside(shape,box,pad=2){
  return [box.x-pad,box.x+box.width+pad].every(x=>[box.y-pad,box.y+box.height+pad].every(y=>shape.isPointInFill(new DOMPoint(x,y))));
}
function statusBar(shape,box){
  const height=2,cx=box.x+box.width/2;
  for(const inset of [4,6,8,10]){
    const y=box.y+box.height-inset;
    const fits=x=>[y-2,y+height+2].every(y=>shape.isPointInFill(new DOMPoint(x,y)));
    if(!fits(cx))continue;
    function edge(boundary){let outside=boundary,within=cx;for(let n=0;n<28;n++){const mid=(outside+within)/2;if(fits(mid))within=mid;else outside=mid;}return within;}
    const left=edge(box.x)+2,right=edge(box.x+box.width)-2;
    const rect={x:left,y,width:right-left,height};
    if(rect.width>=4&&inside(shape,rect,1))return rect;
  }
}
export function paintQueueState(svg,state){
  svg.querySelectorAll('[data-simulation-overlay]').forEach(el=>el.remove());
  const element=name=>document.createElementNS(svg.namespaceURI,name);
  for(const node of svg.querySelectorAll('[data-node]')){
    const q=state?.nodes[node.dataset.node],shape=node.querySelector('[data-silhouette]');if(!q||!shape)continue;
    const box=shape.getBBox(),full=q.saturation>=.95,color=full?'var(--sd-overload,#a53b24)':'var(--sd-status,#176e62)';
    node.dataset.saturation=String(Math.round(q.saturation*100));node.setAttribute('aria-label',`${node.dataset.node}: queue ${q.queued.toFixed(1)}, ${Math.round(q.saturation*100)}% full, ${q.dropped.toFixed(1)} dropped`);
    shape.style.stroke=full?color:'';shape.style.strokeWidth=full?'2.5':'';shape.style.fill=full?'var(--sd-overload-fill,#fff0e6)':'';
    const textBoxes=[...node.querySelectorAll('text')].map(text=>text.getBBox());
    const group=element('g');group.dataset.simulationOverlay='true';group.style.opacity='1';node.append(group);
    const barBox=statusBar(shape,box);
    if(barBox&&!textBoxes.some(text=>overlaps(barBox,text))){
      const bar=element('rect');for(const [key,value]of Object.entries({...barBox,width:barBox.width*Math.max(0,Math.min(1,q.saturation)),fill:color}))bar.setAttribute(key,String(value));group.append(bar);textBoxes.push(barBox);
    }
    if(box.height<64)continue;
    const title=node.querySelector('text.node-title'),text=element('text');
    text.setAttribute('x',title?.getAttribute('x')||box.x+box.width/2);text.setAttribute('text-anchor','middle');text.setAttribute('style',`font-size:10px;font-weight:600;fill:${color}`);group.append(text);
    let placed=false;
    for(const caption of [`${Math.round(q.saturation*100)}% full`,`${Math.round(q.saturation*100)}%`]){
      text.textContent=caption;
      for(let y=box.y+box.height-8;y>=box.y+box.height/2+10;y-=1){
        text.setAttribute('y',y);const bounds=text.getBBox();
        if(inside(shape,bounds)&&!textBoxes.some(other=>overlaps(bounds,other))){placed=true;break;}
      }
      if(placed)break;
    }
    if(!placed)text.remove();
  }
}
