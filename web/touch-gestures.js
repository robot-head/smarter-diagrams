// One-finger panning and two-finger diagram zoom share the same camera.
export function touchGestures(viewport,{getZoom,setZoom,enabled=()=>true}){
  const points=new Map();let baseline;
  const reset=()=>{const p=[...points.values()];baseline={points:p.map(p=>({...p})),left:viewport.scrollLeft,top:viewport.scrollTop,zoom:getZoom()}};
  const distance=p=>Math.hypot(p[1].x-p[0].x,p[1].y-p[0].y);
  viewport.addEventListener('pointerdown',event=>{
    if(event.pointerType!=='touch'||!enabled())return;
    points.set(event.pointerId,{x:event.clientX,y:event.clientY});reset();viewport.setPointerCapture(event.pointerId);
  });
  viewport.addEventListener('pointermove',event=>{
    if(!points.has(event.pointerId))return;
    event.preventDefault();points.set(event.pointerId,{x:event.clientX,y:event.clientY});const p=[...points.values()];
    if(p.length>1&&baseline.points.length>1){const before=distance(baseline.points);if(before>0)setZoom(baseline.zoom*distance(p)/before)}
    else if(p.length===1)viewport.scrollTo(baseline.left+baseline.points[0].x-p[0].x,baseline.top+baseline.points[0].y-p[0].y);
  },{passive:false});
  const end=event=>{if(points.delete(event.pointerId))reset()};
  for(const type of ['pointerup','pointercancel','lostpointercapture'])viewport.addEventListener(type,end);
}
