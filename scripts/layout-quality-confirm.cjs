// Recompile the entire discovered corpus five times, including every fresh seed.
// Exact SVG bytes let an inspected image retain its rating across all five passes.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),{createHash}=require('node:crypto'),{spawnSync}=require('node:child_process');
function confirm(){
 const root=path.resolve(__dirname,'..'),folder=path.join(root,'artifacts/layout-quality/final'),file=path.join(folder,'manifest.json');
 const manifest=JSON.parse(fs.readFileSync(file));assert(manifest.completedAt,'Generate the five discovery rounds first');
 const fingerprint=()=>{const h=createHash('sha256');for(const file of ['Cargo.toml','Cargo.lock',...fs.readdirSync(path.join(root,'src')).filter(f=>f.endsWith('.rs')).sort().map(f=>'src/'+f)])h.update(file).update(fs.readFileSync(path.join(root,file)));return h.digest('hex')};
 const engineFingerprint=fingerprint(),images=new Map();
 for(const round of manifest.rounds){assert.equal(round.engineFingerprint,engineFingerprint);for(const family of ['baseline','exploration'])for(const c of round[family].cases){if(c.control)continue;if(!images.has(c.svgSha256))images.set(c.svgSha256,{id:c.id,source:fs.readFileSync(path.join(root,round[family].directory,c.id+'.diagram'),'utf8')});}}
 manifest.confirmationPasses=[];
 for(let pass=1;pass<=5;pass++){
  const start=Date.now();assert.equal(fingerprint(),engineFingerprint,'Engine changed; restart the streak');
  for(const[hash,c]of images){const r=spawnSync(path.join(root,'target/debug/smarter-diagrams'),[],{input:c.source,encoding:'utf8',timeout:8000,maxBuffer:8*1024*1024});assert.equal(r.status,0,`${c.id}: ${r.stderr||r.error}`);assert.equal(createHash('sha256').update(r.stdout).digest('hex'),hash,`${c.id}: changed SVG on confirmation pass ${pass}`);}
  assert.equal(fingerprint(),engineFingerprint,'Engine changed during confirmation');
  manifest.confirmationPasses.push({pass,engineFingerprint,images:images.size,matchedSvgHashes:[...images.keys()],elapsedMs:Date.now()-start});fs.writeFileSync(file,JSON.stringify(manifest,null,2));console.log(`Full corpus confirmation ${pass}: all ${images.size} inspected SVGs matched exactly.`);
 }
}
module.exports=confirm;if(require.main===module)confirm();
