// Triage measurements, not a visual-quality score. Crossings are proper
// intersections only; coincident ends and bends are deliberately excluded.
module.exports = layout => {
 const routes=Object.values(layout.routes),epsilon=1e-6;
 const horizontal=(a,b)=>Math.abs(a.y-b.y)<epsilon,vertical=(a,b)=>Math.abs(a.x-b.x)<epsilon;
 const cross=(a,b,c,d)=>horizontal(a,b)&&vertical(c,d)?c.x>Math.min(a.x,b.x)+epsilon&&c.x<Math.max(a.x,b.x)-epsilon&&a.y>Math.min(c.y,d.y)+epsilon&&a.y<Math.max(c.y,d.y)-epsilon:vertical(a,b)&&horizontal(c,d)?cross(c,d,a,b):false;
 const overlap=(a,b,c,d)=>horizontal(a,b)&&horizontal(c,d)&&Math.abs(a.y-c.y)<epsilon?Math.min(Math.max(a.x,b.x),Math.max(c.x,d.x))-Math.max(Math.min(a.x,b.x),Math.min(c.x,d.x))>epsilon:vertical(a,b)&&vertical(c,d)&&Math.abs(a.x-c.x)<epsilon?Math.min(Math.max(a.y,b.y),Math.max(c.y,d.y))-Math.max(Math.min(a.y,b.y),Math.min(c.y,d.y))>epsilon:false;
 const collinear=(a,b,c)=>{
  if(horizontal(a,b)&&horizontal(b,c)&&(b.x-a.x)*(c.x-b.x)>=0)return true;
  if(vertical(a,b)&&vertical(b,c)&&(b.y-a.y)*(c.y-b.y)>=0)return true;
  const ac=Math.hypot(c.x-a.x,c.y-a.y);
  if(ac<=1e-7)return false;
  const perp=Math.abs((b.x-a.x)*(c.y-a.y)-(b.y-a.y)*(c.x-a.x))/ac;
  const dot=(b.x-a.x)*(c.x-b.x)+(b.y-a.y)*(c.y-b.y);
  return perp<epsilon&&dot>=0;
 };
 const coalesce=points=>{
  const out=[];
  for(const point of points){
   if(out.length&&Math.hypot(out.at(-1).x-point.x,out.at(-1).y-point.y)<1e-7)continue;
   while(out.length>=2&&collinear(out[out.length-2],out[out.length-1],point))out.pop();
   out.push(point);
  }
  return out;
 };
 const isBend=(a,b,c)=>{
  const l1=Math.hypot(b.x-a.x,b.y-a.y),l2=Math.hypot(c.x-b.x,c.y-b.y);
  if(l1<=1e-7||l2<=1e-7)return false;
  const crossNorm=Math.abs((b.x-a.x)*(c.y-b.y)-(b.y-a.y)*(c.x-b.x))/(l1*l2);
  const dotNorm=((b.x-a.x)*(c.x-b.x)+(b.y-a.y)*(c.y-b.y))/(l1*l2);
  return crossNorm>epsilon||dotNorm<-1+epsilon;
 };
 const coalesced=routes.map(r=>coalesce(r.points));
 let properCrossings=0,overlappingRoutePairs=0,maxDetourRatio=0,microJogCount=0,maxBendCount=0;
 for(let i=0;i<coalesced.length;i++){
  const p=coalesced[i];
  if(p.length<2)continue;
  const start=p[0],end=p.at(-1),direct=Math.abs(start.x-end.x)+Math.abs(start.y-end.y),length=p.slice(1).reduce((sum,b,j)=>sum+Math.hypot(b.x-p[j].x,b.y-p[j].y),0);
  if(direct>epsilon)maxDetourRatio=Math.max(maxDetourRatio,length/direct);
  let bends=0;
  for(let k=1;k+1<p.length;k++)if(isBend(p[k-1],p[k],p[k+1]))bends++;
  maxBendCount=Math.max(maxBendCount,bends);
  if(p.length>=4){
   for(let k=1;k+2<p.length;k++){
    if(isBend(p[k-1],p[k],p[k+1])&&isBend(p[k],p[k+1],p[k+2])&&Math.hypot(p[k+1].x-p[k].x,p[k+1].y-p[k].y)<12-epsilon)microJogCount++;
   }
  }
  for(let j=i+1;j<coalesced.length;j++){
   const q=coalesced[j];let shared=false;
   for(let a=1;a<p.length;a++)for(let b=1;b<q.length;b++){properCrossings+=Number(cross(p[a-1],p[a],q[b-1],q[b]));shared=shared||overlap(p[a-1],p[a],q[b-1],q[b]);}
   overlappingRoutePairs+=Number(shared);
  }
 }
 const callouts=layout.labels.filter(l=>l.callout).map(l=>({feature:l.feature,text:l.text,centerToAnchorPx:Math.hypot(l.bounds.x-l.anchor.x,l.bounds.y-l.anchor.y)})).sort((a,b)=>b.centerToAnchorPx-a.centerToAnchorPx);
 return {properCrossings,overlappingRoutePairs,maxDetourRatio,calloutCount:callouts.length,mostRemoteCallout:callouts[0]||null,microJogCount,maxBendCount};
};
