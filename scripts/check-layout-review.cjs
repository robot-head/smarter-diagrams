// Validate manually recorded ratings against the exact five-round SVG evidence.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {createHash}=require('node:crypto');
const root=path.resolve(__dirname,'..'),folder=path.join(root,'artifacts/layout-quality/final');
const manifest=JSON.parse(fs.readFileSync(path.join(folder,'manifest.json')));
const review=JSON.parse(fs.readFileSync(path.resolve(root,process.argv[2]||'docs/layout-quality-review.json')));
assert.equal(review.status,'complete','Visual inspection is still in progress');
const engine=createHash('sha256');
for(const file of ['Cargo.toml','Cargo.lock',...fs.readdirSync(path.join(root,'src')).filter(f=>f.endsWith('.rs')).sort().map(f=>'src/'+f)])engine.update(file).update(fs.readFileSync(path.join(root,file)));
assert.equal(engine.digest('hex'),review.engineFingerprint,'Review is stale for the current source');
assert(manifest.completedAt,'All five mechanical rounds must finish first');
assert.equal(manifest.rounds.length,5);
const ratings=new Map(review.images.map(r=>[r.svgSha256,r]));
assert.equal(ratings.size,review.images.length,'Duplicate image ratings');
let streak=0,occurrences=0;const distinct=new Set();
for(const round of manifest.rounds){
 assert.equal(round.engineFingerprint,review.engineFingerprint,'Review is stale for this engine');
 for(const family of ['baseline','exploration'])for(const c of round[family].cases){
  if(c.control)continue;
  const r=ratings.get(c.svgSha256);assert(r,`Unreviewed: round ${round.round}/${family}/${c.id}`);
  assert(r.evidence&&r.notes,'Each rating needs inspection evidence and notes');
  assert(fs.existsSync(path.join(root,r.evidence)),`Missing inspection evidence: ${r.evidence}`);
  const svg=fs.readFileSync(path.join(root,round[family].directory,c.id+'.svg'));
  assert.equal(createHash('sha256').update(svg).digest('hex'),c.svgSha256,'SVG no longer matches the review');
  assert.equal(r.beauty,5,`${c.id}: beauty below 5`);assert.equal(r.clarity,5,`${c.id}: clarity below 5`);
  distinct.add(c.svgSha256);occurrences++;
 }
 streak++;
}
assert.equal(streak,5);
console.log(`Visual review: ${distinct.size} distinct SVGs, ${occurrences} example occurrences, five consecutive reviewed rounds. Ratings are subjective and bounded to this corpus.`);
