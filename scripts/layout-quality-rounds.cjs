// Five fresh grammar rounds plus an unchanged, checksum-verified baseline.
// This generates evidence. Beauty/clarity ratings require visual inspection.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),{spawnSync}=require('node:child_process');
const root=path.resolve(__dirname,'..'),out=path.join(root,'artifacts/layout-quality/final');
const seeds=require('./layout-quality-seeds.cjs');
fs.mkdirSync(out,{recursive:true});
const manifest={startedAt:new Date().toISOString(),rounds:[],note:'Machine checks are separate from visual beauty/clarity ratings.'};let fingerprint,baseline;
for(const[round,seedset]of seeds.entries()){
 const name=`round-${String(round+1).padStart(2,'0')}`,folder=path.join(out,name);fs.mkdirSync(folder,{recursive:true});
 for(const family of ['baseline','exploration']){
  console.log(`${name}: generating ${family}`);
  const directory=path.relative(root,path.join(folder,family)),args=['scripts/layout-audit.cjs','--strict','--output',directory,...(family==='exploration'?['--explore','--seeds',seedset.map(s=>'0x'+s.toString(16)).join(',')]:["--include-exploration"])];
  // Unchanged SVG bytes need only one paint audit in the same browser/font
  // environment. Native compilation and determinism still run every round.
  if(round>0 || family==='exploration')args.push('--reuse-paint',path.relative(root,path.join(out,'round-01/baseline/results.json')));
  const run=spawnSync(process.execPath,args,{cwd:root,stdio:'inherit',timeout:300000});assert.equal(run.status,0,`${name}/${family}: generation or mechanical audit failed`);
  const data=JSON.parse(fs.readFileSync(path.join(root,directory,'results.json')));fingerprint??=data.engineFingerprint;assert.equal(data.engineFingerprint,fingerprint,'Engine changed between rounds; restart the streak');
  if(family==='baseline'){
   const hashes=Object.fromEntries(data.cases.map(c=>[c.id,c.svgSha256]));baseline??=hashes;assert.deepEqual(hashes,baseline,'The fixed baseline changed across rounds');
  }
  manifest.rounds[round]??={round:round+1,seeds:seedset.map(s=>'0x'+s.toString(16)),engineFingerprint:fingerprint};manifest.rounds[round][family]={directory,cases:data.cases.map(c=>({id:c.id,svgSha256:c.svgSha256,control:c.id.startsWith('control-')})),completedAt:new Date().toISOString()};
  fs.writeFileSync(path.join(out,'manifest.json'),JSON.stringify(manifest,null,2));
 }
}
manifest.completedAt=new Date().toISOString();fs.writeFileSync(path.join(out,'manifest.json'),JSON.stringify(manifest,null,2));
require('./layout-quality-confirm.cjs')();
console.log('Five mechanical rounds complete. Inspect every distinct image and record the separate beauty/clarity review before calling this a visual pass.');
