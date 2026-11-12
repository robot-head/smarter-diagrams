// Cheap native triage of every fresh seed before browser painting/review.
const fs=require('node:fs'),path=require('node:path'),{spawnSync}=require('node:child_process');
const root=path.resolve(__dirname,'..'),out=path.join(root,'artifacts/layout-quality/preflight'),seen=new Set(),results=[];
fs.mkdirSync(out,{recursive:true});
for(const seeds of require('./layout-quality-seeds.cjs'))for(const c of require('./layout-exploration-cases.cjs')({seeds}).cases){
 if(seen.has(c.source))continue;seen.add(c.source);
 const run=spawnSync(path.join(root,'target/debug/smarter-diagrams'),['--format','json'],{input:c.source,encoding:'utf8',timeout:8000,maxBuffer:8*1024*1024});
 const r={id:c.id};if(run.status!==0)r.error=run.stderr||String(run.error||run.signal);else{r.metrics=require('./layout-audit-metrics.cjs')(JSON.parse(run.stdout));if(r.metrics.overlappingRoutePairs)r.error='Ambiguous coincident routes';if(r.metrics.mostRemoteCallout?.centerToAnchorPx>160.01)r.error='Detached automatic caption';}
 if(r.error){fs.writeFileSync(path.join(out,c.id+'.diagram'),c.source);console.log(JSON.stringify(r));}results.push(r);
}
fs.writeFileSync(path.join(out,'results.json'),JSON.stringify(results,null,2));console.log(`${results.length} distinct sources; ${results.filter(r=>r.error).length} failures.`);if(results.some(r=>r.error))process.exitCode=1;
