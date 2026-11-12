// Triage measurements, not a visual-quality score. Crossings are proper
// intersections only; coincident ends and bends are deliberately excluded.
module.exports = layout => {
 const routes=Object.values(layout.routes),epsilon=1e-6;
 const horizontal=(a,b)=>Math.abs(a.y-b.y)<epsilon,vertical=(a,b)=>Math.abs(a.x-b.x)<epsilon;
 const cross=(a,b,c,d)=>horizontal(a,b)&&vertical(c,d)?c.x>Math.min(a.x,b.x)+epsilon&&c.x<Math.max(a.x,b.x)-epsilon&&a.y>Math.min(c.y,d.y)+epsilon&&a.y<Math.max(c.y,d.y)-epsilon:vertical(a,b)&&horizontal(c,d)?cross(c,d,a,b):false;
 const overlap=(a,b,c,d)=>horizontal(a,b)&&horizontal(c,d)&&Math.abs(a.y-c.y)<epsilon?Math.min(Math.max(a.x,b.x),Math.max(c.x,d.x))-Math.max(Math.min(a.x,b.x),Math.min(c.x,d.x))>epsilon:vertical(a,b)&&vertical(c,d)&&Math.abs(a.x-c.x)<epsilon?Math.min(Math.max(a.y,b.y),Math.max(c.y,d.y))-Math.max(Math.min(a.y,b.y),Math.min(c.y,d.y))>epsilon:false;
 let properCrossings=0,overlappingRoutePairs=0,maxDetourRatio=0;
 for(let i=0;i<routes.length;i++){
  const p=routes[i].points,start=p[0],end=p.at(-1),direct=Math.abs(start.x-end.x)+Math.abs(start.y-end.y),length=p.slice(1).reduce((sum,b,j)=>sum+Math.hypot(b.x-p[j].x,b.y-p[j].y),0);
  if(direct>epsilon)maxDetourRatio=Math.max(maxDetourRatio,length/direct);
  for(let j=i+1;j<routes.length;j++){
   const q=routes[j].points;let shared=false;
   for(let a=1;a<p.length;a++)for(let b=1;b<q.length;b++){properCrossings+=Number(cross(p[a-1],p[a],q[b-1],q[b]));shared=shared||overlap(p[a-1],p[a],q[b-1],q[b]);}
   overlappingRoutePairs+=Number(shared);
  }
 }
 const callouts=layout.labels.filter(l=>l.callout).map(l=>({feature:l.feature,text:l.text,centerToAnchorPx:Math.hypot(l.bounds.x-l.anchor.x,l.bounds.y-l.anchor.y)})).sort((a,b)=>b.centerToAnchorPx-a.centerToAnchorPx);
 return {properCrossings,overlappingRoutePairs,maxDetourRatio,calloutCount:callouts.length,mostRemoteCallout:callouts[0]||null};
};
