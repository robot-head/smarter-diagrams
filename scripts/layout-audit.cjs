// Reproducible native-layout stress corpus and browser-painted geometry audit.
// Build first: cargo build --locked --all-features
// Run: npm run test:layout-audit  (outputs artifacts/layout-audit/index.html)
// Larger mixed-feature corpus: npm run test:layout-explore -- --strict
// Both corpora are strict CI checks; visual ratings are recorded separately.
const fs=require('node:fs'),path=require('node:path'),{spawnSync}=require('node:child_process'),{performance}=require('node:perf_hooks');
const {createHash}=require('node:crypto');
const {chromium}=require('playwright'),browserOptions=require('./browser-options.cjs'),checkSvg=require('./check-rendered-svg.cjs');
const argument=name=>{const i=process.argv.indexOf(name);return i<0?undefined:process.argv[i+1]};
const seeds=argument('--seeds')?.split(',').map(Number);
if(seeds && (seeds.length!==3 || seeds.some(seed=>!Number.isSafeInteger(seed)||seed<=0||seed>0xffffffff)))throw Error('--seeds requires three nonzero u32 seeds');
const exploration=process.argv.includes('--explore'),extended=exploration?require('./layout-exploration-cases.cjs')({seeds}):null;
const root=path.resolve(__dirname,'..'),out=path.resolve(root,argument('--output')||(exploration?'artifacts/layout-exploration':'artifacts/layout-audit')),binary=path.join(root,'target/debug/smarter-diagrams');
{const built=spawnSync('cargo',['build','--locked','--all-features','--example','layout-audit'],{cwd:root,encoding:'utf8',timeout:120000});if(built.status!==0)throw Error(built.stderr||'Audit helper build failed');}
const engineHash=createHash('sha256');
for(const file of ['Cargo.toml','Cargo.lock',...fs.readdirSync(path.join(root,'src')).filter(name=>name.endsWith('.rs')).sort().map(name=>'src/'+name)])engineHash.update(file).update(fs.readFileSync(path.join(root,file)));
const engineFingerprint=engineHash.digest('hex');
const reusePath=argument('--reuse-paint');
const reuse=reusePath?JSON.parse(fs.readFileSync(path.resolve(root,reusePath))):null;
if(reuse && reuse.engineFingerprint!==engineFingerprint)throw Error('Paint cache belongs to another engine');
const paintCache=new Map((reuse?.cases||[]).filter(r=>r.status==='rendered').map(r=>[r.svgSha256,r]));
const detailIds=exploration?['a11ce002-sparse-mesh','5eed1203-sparse-mesh','09-many-record-slots','10-many-named-ports','02-planar-mesh','03-dense-mesh','c0ffee01-dag','08-cross-ownership']:[];
const cases=[];
const add=(id,concept,source)=>cases.push({id,concept,source:source.trim()+'\n'});
add('01-pipeline','A request moves through four distinct stages',`client: Browser\napi: Public API\nworker: Order worker\ndb: Ledger\nclient -> api: HTTPS\napi -> worker: jobs\nworker -> db: writes`);
add('02-long-pipeline','Eight sequential transformations',Array.from({length:8},(_,i)=>`s${i}: ${['Read','Validate','Normalize','Enrich','Filter','Aggregate','Encode','Store'][i]}`).concat('s0 -> s1 -> s2 -> s3 -> s4 -> s5 -> s6 -> s7: data').join('\n'));
add('03-fan-out','One event reaches five independent consumers',`bus: Event log [shape: queue]\n${Array.from({length:5},(_,i)=>`c${i}: Consumer ${i+1}\nbus -> c${i}: event`).join('\n')}`);
add('04-fan-in','Five producers converge at one sink',`${Array.from({length:5},(_,i)=>`p${i}: Producer ${i+1}\np${i} -> warehouse: upload`).join('\n')}\nwarehouse: Warehouse [shape: database]`);
add('05-decision','A yes/no branch rejoins after processing',`start: Request\nchoice: Allowed? [shape: diamond]\nyes: Execute\nno: Reject\nend: Response\nstart -> choice\nchoice -> yes: yes\nchoice -> no: no\nyes -> end\nno -> end`);
add('06-feedback','A retry loop returns failures to a queue',`api: API\nqueue: Jobs\nworker: Worker\ndb: Database\napi -> queue: enqueue\nqueue -> worker: consume\nworker -> db: persist\nworker -> queue: retry`);
add('07-ring','Four peers form a directed ring',`a: Node A\nb: Node B\nc: Node C\nd: Node D\na -> b -> c -> d -> a: token`);
add('08-self-loop','One service retries itself',`api: Worker\napi -> api: retry`);
add('09-parallel','Three distinct protocols share the same endpoints',`client: Client\nserver: Server\nclient -> server: HTTPS\nclient -> server: WebSocket\nclient -> server: telemetry`);
add('10-bidirectional','Requests and acknowledgements remain distinguishable',`client: Client\napi: API\nqueue: Queue\nclient <-> api: request / reply\napi <-> queue: publish / ack`);
add('11-disconnected','Two independent subsystems and a standalone observer',`a: Checkout\nb: Orders\nc: Search\nd: Index\ne: Observer\na -> b: write\nc -> d: query`);
add('12-butterfly','A complete two-by-two dependency graph',`a: Frontend A\nb: Frontend B\nc: Backend C\nd: Backend D\na -> c: primary\na -> d: fallback\nb -> c: fallback\nb -> d: primary`);
add('13-mesh','Three services all depend on three stores',`${['a','b','c'].map(x=>`${x}: Service ${x.toUpperCase()}`).join('\n')}\n${['x','y','z'].map(x=>`${x}: Store ${x.toUpperCase()} [shape: database]`).join('\n')}\n${['a','b','c'].flatMap(a=>['x','y','z'].map(b=>`${a} -> ${b}: access`)).join('\n')}`);
add('14-long-labels','Detailed node names and connection semantics remain legible',`client: Customer account management console\napi: Regional authorization and policy enforcement service\ndb: Durable customer transaction history\nclient -> api: Authenticate using a short-lived access token\napi -> db: Read the current organization membership`);
add('15-unicode','Unicode names and multiline labels retain visual hierarchy',`a: "東京 API"\nb: ${JSON.stringify('München\nGateway')}\nc: "Δεδομένα" [shape: database]\na -> b: "認証"\nb -> c: "übertragen"`);
add('16-container','Ownership groups an API and its data',`client: Browser\nprod: Production {\n api: API\n db: Database\n api -> db: query\n}\nclient -> prod.api: HTTPS`);
add('17-nested','Nested ownership remains easy to read',`client: Client\ncloud: Cloud {\n region: Region {\n  api: API\n  db: Database\n  api -> db: query\n }\n log: Audit log [shape: queue]\n region.api -> log: audit\n}\nclient -> cloud.region.api: HTTPS`);
add('18-cross-scope','Two ownership groups exchange events',`west: West {\n api: API\n queue: Queue\n api -> queue: send\n}\neast: East {\n worker: Worker\n db: Database\n worker -> db: write\n}\nwest.queue -> east.worker: replicate`);
add('19-zone','Shared trust scope crosses ownership boundaries',`client: Browser\napi: API\ndb: Ledger\nkms: Key service\nclient -> api: HTTPS\napi -> db: SQL\napi -> kms: key\nzone secure "Trusted boundary" around [api,db,kms] {\n padding: 1.5g\n}`);
add('20-records','Two table slots receive distinct operations',`api: API @pin(grid: 0,0)\ndb: Orders [shape: sql_table] @pin(grid: 24,0) {\n port id: uuid [side: W,pos: 1]\n port customer_id: uuid [side: W,pos: 2]\n}\napi.E -> db#id: insert\napi.E -> db#customer_id: lookup`);
add('21-ports','Two named queue ports convey direction and acknowledgements',`producer: Publisher @pin(grid: 0,0)\nbroker: Event log [shape: queue] @pin(grid: 24,0) {\n port input [side: W,ratio: 0.25]\n port ack [side: W,ratio: 0.75]\n}\nproducer.E -> broker#input: publish\nbroker#ack -> producer.E: ack`);
add('22-obstacle','A connector avoids a central pinned obstacle',`a: Source @pin(grid: 0,0)\nb: Target @pin(grid: 40,0)\nwall: Firewall @pin(grid: 20,0)\na.E -> b.W: tunnel`);
add('23-trace','A numbered request trace explains execution order',`client: Browser\napi: API\nworker: Worker\ndb: Ledger\nclient -> api: HTTPS\napi -> worker: jobs\nworker -> db: writes\nflow request "Request path" { client -> api -> worker -> db }`);
add('24-vertical','A vertical pipeline uses explicit north/south attachments',`a: Receive\nb: Validate\nc: Persist\na.S -> b.N: request\nb.S -> c.N: accepted\nalign col [a,b,c]\ndistribute col [a,b,c] [gap: 5g]`);
// Forty bounded seeded grammars: DAGs, cyclic graphs, multigraphs, pinned
// obstacle courses and branching trees. The source is saved even on failure.
let state=0x5eedc0de;const random=()=>{state^=state<<13;state^=state>>>17;state^=state<<5;return(state>>>0)/4294967296},pick=a=>a[Math.floor(random()*a.length)];
for(let f=0;f<5;f++)for(let s=0;s<8;s++){
 const n=5+Math.floor(random()*5),family=['dag','cycle','parallel','pinned','tree'][f],lines=[],seen=new Set();
 for(let i=0;i<n;i++){const label=pick(['API','Worker','Cache','Event log','Ledger','Validate request','地域ゲートウェイ','Organization membership service'])+' '+i,shape=pick(['rect','rect','queue','database','diamond','cache']);lines.push(`n${i}: ${JSON.stringify(label)} [shape: ${shape}]${f===3?` @pin(grid: ${(i%3)*24},${Math.floor(i/3)*16})`:''}`)}
 const edge=(a,b)=>{const key=`${a},${b}`;if(seen.has(key)&&f!==2)return;seen.add(key);lines.push(`n${a} -> n${b}: ${pick(['send','ack','retry','read through','validate organization access',''])}`)};
 for(let i=1;i<n;i++)edge(f===4?Math.floor((i-1)/2):i-1,i);
 if(f===0)for(let i=0;i<n;i++)for(let j=i+2;j<n;j++)if(random()<.22)edge(i,j);
 if(f===1){edge(n-1,0);for(let j=0;j<3;j++)edge(Math.floor(random()*n),Math.floor(random()*n))}
 if(f===2)for(let j=0;j<4;j++)edge(Math.floor(random()*(n-1)),n-1);
 if(f===3){edge(0,n-1);edge(2,n-2)}
 add(`fuzz-${family}-${s}`,`${family} grammar sample ${s}; seed 0x5eedc0de`,lines.join('\n'));
}
if(extended)cases.splice(0,cases.length,...extended.cases);
else if(process.argv.includes('--include-exploration'))cases.push(...require('./layout-exploration-cases.cjs')().cases);
(async()=>{
 fs.rmSync(out,{recursive:true,force:true});fs.mkdirSync(out,{recursive:true});const results=[];
 for(const c of cases){
  fs.writeFileSync(path.join(out,c.id+'.diagram'),c.source);
  const render=source=>spawnSync(binary,[],{input:source,encoding:'utf8',timeout:8000,maxBuffer:8*1024*1024});
  const start=performance.now(),r=render(c.source),ms=performance.now()-start;
  const result={id:c.id,concept:c.concept,ms:Math.round(ms*100)/100,status:r.error?.code==='ETIMEDOUT'?'timeout':r.status===0?'rendered':'diagnostic'};
  if(r.status!==0){result.diagnostic=(r.stderr||r.error?.message||'unknown error').trim();results.push(result);continue}
  fs.writeFileSync(path.join(out,c.id+'.svg'),r.stdout);
  {
   const geometry=spawnSync(path.join(root,'target/debug/examples/layout-audit'),[],{input:c.source,encoding:'utf8',timeout:8000,maxBuffer:8*1024*1024});
   if(geometry.status!==0)throw Error(`${c.id}: SVG rendered but layout JSON failed: ${geometry.stderr}`);
   const metadata=JSON.parse(geometry.stdout);result.routeMetrics=require('./layout-audit-metrics.cjs')(metadata.layout);result.semanticFailures=metadata.semanticFailures;
   result.routeFailures=[];
   if(result.routeMetrics.overlappingRoutePairs)result.routeFailures.push("Ambiguous coincident route segments");
   if(result.routeMetrics.mostRemoteCallout?.centerToAnchorPx>160.01&&!c.source.includes("@label("))result.routeFailures.push("Automatic caption more than 160px from its anchor");
  }
  result.deterministic=render(c.source).stdout===r.stdout;
  const mutated=render('# harmless comment\n'+c.source.replaceAll('\n','\r\n'));
  result.triviaInvariant=mutated.status===0&&mutated.stdout===r.stdout;
  result.svg=r.stdout;result.svgSha256=createHash("sha256").update(r.stdout).digest("hex");results.push(result);
 }
 const browser=await chromium.launch(browserOptions('http://127.0.0.1')),page=await browser.newPage({viewport:{width:1600,height:1700},deviceScaleFactor:1});
 await page.setContent('<!doctype html><style>body{margin:0;font:15px system-ui;background:#eef2f6}*{box-sizing:border-box}.grid{display:grid;grid-template-columns:1fr 1fr;gap:16px;padding:16px}article{height:540px;background:white;border:1px solid #ccd5df;border-radius:8px;padding:14px;overflow:hidden}h2{font-size:16px;margin:0 0 4px}p{font-size:12px;color:#4b5d70;margin:0 0 10px}.figure pre{white-space:pre-wrap;overflow-wrap:anywhere;font:13px/1.5 monospace;max-width:100%}.figure{height:450px;display:flex;align-items:center;justify-content:center}.figure svg{width:100%;height:100%;object-fit:contain}</style><main class="grid"></main>');
 for(const r of results.filter(r=>r.svg)){
  const cached=paintCache.get(r.svgSha256);
  if(cached){for(const key of ['paintFailures','metrics'])if(key in cached)r[key]=cached[key];r.paintReusedFrom=reusePath;continue;}
  r.paintFailures=await page.evaluate(checkSvg,r.svg);
  r.metrics=await page.evaluate(markup=>{
   const host=document.createElement('div');host.innerHTML=markup;document.body.append(host);const svg=host.querySelector('svg'),v=svg.viewBox.baseVal;
   const nodes=[...svg.querySelectorAll('[data-node]')].map(e=>({id:e.dataset.node,container:e.dataset.shape==='container',box:e.querySelector('[data-silhouette]').getBBox()}));
   const overlap=(a,b)=>Math.min(a.x+a.width,b.x+b.width)-Math.max(a.x,b.x)>1&&Math.min(a.y+a.height,b.y+b.height)-Math.max(a.y,b.y)>1;
   const nodeOverlaps=[];for(let i=0;i<nodes.length;i++)for(let j=i+1;j<nodes.length;j++){const a=nodes[i],b=nodes[j];if(!a.id.startsWith(b.id+'.')&&!b.id.startsWith(a.id+'.')&&overlap(a.box,b.box))nodeOverlaps.push([a.id,b.id])}
   const captions=[...svg.querySelectorAll('[data-label]')].filter(e=>e.querySelector('text')).map(e=>({id:e.dataset.label,box:e.querySelector('text').getBBox()})),captionOverlaps=[];
   for(let i=0;i<captions.length;i++)for(let j=i+1;j<captions.length;j++)if(overlap(captions[i].box,captions[j].box))captionOverlaps.push([captions[i].id,captions[j].id]);
   const paths=[...svg.querySelectorAll('path.edge')],length=paths.reduce((sum,p)=>sum+p.getTotalLength(),0),fit=Math.min(1,730/v.width,450/v.height),fontSizes=[...svg.querySelectorAll('text')].map(e=>parseFloat(getComputedStyle(e).fontSize));
   const finiteGeometry=[v.x,v.y,v.width,v.height,length,...nodes.flatMap(n=>[n.box.x,n.box.y,n.box.width,n.box.height])].every(Number.isFinite)&&v.width>0&&v.height>0;
   const result={finiteGeometry,nodes:nodes.length,edges:paths.length,viewBox:[v.x,v.y,v.width,v.height],aspectRatio:v.width/v.height,bodyAreaRatio:nodes.filter(n=>!n.container).reduce((s,n)=>s+n.box.width*n.box.height,0)/(v.width*v.height),totalRouteLength:length,minFitTextPx:Math.min(...fontSizes)*fit,nodeOverlaps,captionOverlaps};host.remove();return result;
  },r.svg);
 }
 for(const r of results.filter(r=>r.metrics))if(/^fuzz-(dag|parallel|tree)-/.test(r.id)||r.id==='02-long-pipeline')r.presentationFailures=r.metrics.minFitTextPx<8?['Fully fitted text below 8 px']:[];
 // Isolate the four unpinned caption failures without changing topology.
 const controls=[];
 for(const id of (exploration?[]:['03-fan-out','fuzz-cycle-7','fuzz-tree-0','fuzz-tree-6','03-fan-out-spaced'])){
  const original=id.replace('-spaced',''),base=cases.find(c=>c.id===original).source;
  const source=id.endsWith('-spaced')?base+'\nalign col [c0,c1,c2,c3,c4]\ndistribute col [c0,c1,c2,c3,c4] [gap: 8g]\n':base.split('\n').map(line=>line.includes(' -> ')?line.replace(/:[^\n]*$/,''):line).join('\n');
  const r=spawnSync(binary,[],{input:source,encoding:'utf8',timeout:8000,maxBuffer:8*1024*1024});
  const control={id:id+'-control',concept:id.endsWith('-spaced')?'Captions retained; consumers distributed vertically':'Same topology; edge captions removed',status:r.status===0?'rendered':'diagnostic',diagnostic:r.stderr.trim(),svg:r.status===0?r.stdout:undefined};
  fs.writeFileSync(path.join(out,control.id+'.diagram'),source);if(control.svg){fs.writeFileSync(path.join(out,control.id+'.svg'),control.svg);control.paintFailures=await page.evaluate(checkSvg,control.svg)}controls.push(control);
 }
 const esc=s=>s.replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('"','&quot;');
 const reportPath=path.join(root,exploration?'docs/layout-exploration.json':'docs/layout-audit.json');
 const report=fs.existsSync(reportPath)?JSON.parse(fs.readFileSync(reportPath,'utf8')):{knownDiagnostics:{},currentReview:{cases:[]}},reviews=new Map((report.engineFingerprint===engineFingerprint?report.currentReview?.cases||[]:[]).map(r=>[r.id,(r.beauty==null?'':`Beauty ${r.beauty}/5; clarity ${r.clarity}/5. `)+r.review]));
 const cards=results.map(r=>`<article><h2>${esc(r.id)}</h2><p>${esc(r.concept)}</p><div class="figure">${r.svg||`<pre>${esc(r.diagnostic)}</pre>`}</div></article>`);
 const controlCards=controls.map(r=>`<article><h2>${esc(r.id)}</h2><p>${esc(r.concept)}</p><div class="figure">${r.svg||esc(r.diagnostic)}</div></article>`);
 for(let i=0;i<results.length;i+=6){await page.locator('main').evaluate((e,html)=>e.innerHTML=html,cards.slice(i,i+6).join(''));await page.screenshot({path:path.join(out,`sheet-${String(i/6+1).padStart(2,'0')}.png`),fullPage:true})}
 if(controlCards.length){await page.locator('main').evaluate((e,html)=>e.innerHTML=html,controlCards.join(''));await page.screenshot({path:path.join(out,'control-sheet.png'),fullPage:true});}
 if(detailIds.length){const detail=await browser.newPage({viewport:{width:2400,height:1600}});for(const id of detailIds){const r=results.find(r=>r.id===id);if(!r?.svg)continue;await detail.setContent('<style>body{margin:20px;background:white}</style>'+r.svg);await detail.evaluate(()=>document.fonts.ready);await detail.locator('svg').screenshot({path:path.join(out,'detail-'+id+'.png')});}await detail.close();}
 for(const r of [...results,...controls]){delete r.svg}
 fs.writeFileSync(path.join(out,'results.json'),JSON.stringify({engineFingerprint,revision:spawnSync('git',['rev-parse','HEAD'],{encoding:'utf8',cwd:root}).stdout.trim(),seed:extended?.seeds||(process.argv.includes('--include-exploration')?['0x5eedc0de',...require('./layout-exploration-cases.cjs')().seeds]:'0x5eedc0de'),cases:results,controls},null,2));
 const shell=await page.content(),style=shell.match(/<style>([\s\S]*?)<\/style>/)[1];
 fs.writeFileSync(path.join(out,'index.html'),`<!doctype html><meta charset="utf-8"><title>Layout audit</title><style>${style}article{height:auto;min-height:580px}.links{font-size:12px}.review-note{font-size:14px;line-height:1.5;color:#24364b;margin:12px 0}.review-summary{margin:16px;padding:20px;background:white;border-radius:8px}.review-summary li{margin:12px 0}</style><section class="review-summary"><h1>Layout audit: ${cases.length} concepts and seeded graphs</h1>${exploration?`<p>${results.filter(r=>r.status==='rendered').length} rendered; ${results.filter(r=>r.status!=='rendered').length} diagnostics; ${results.reduce((sum,r)=>sum+(r.semanticFailures?.length||0),0)} missing required captions. Geometry checks alone do not establish visual or semantic correctness.</p>`:""}<p>Current visual pass: ${results.filter(r=>r.metrics?.minFitTextPx<8).length} cases have text below 8 px only when fully fitted into this thumbnail. Normal viewers retain a readable starting scale with panning. Geometry checks and visual clarity are reviewed separately.</p><ul>${(report.engineFingerprint===engineFingerprint?report.currentReview?.remainingPriorities||[]:[]).map(f=>`<li><strong>${esc(f.priority+": "+f.problem)}</strong> — ${esc(f.nextStep)}<br>Cases: ${esc(f.cases.join(", "))}</li>`).join("")}</ul></section><p>Each source and SVG is saved beside this gallery. Full measurements: <a href="results.json">results.json</a>.</p><main class="grid">${cards.map((c,i)=>c.replace('</article>',`<p class="review-note">${esc(reviews.get(results[i].id)||"")}</p><div class="links"><a href="${results[i].id}.diagram">Source</a> · ${results[i].status==='rendered'?`<a href="${results[i].id}.svg">Full SVG</a>`:'<span>No SVG emitted</span>'}${detailIds.includes(results[i].id)?` · <a href="detail-${results[i].id}.png">Native-size inspection</a>`:''}</div></article>`)).join('')}</main><h1>Diagnostic isolation controls</h1><main class="grid">${controlCards.join('')}</main>`);
 await browser.close();
 console.log(JSON.stringify({cases:results.length,rendered:results.filter(r=>r.status==='rendered').length,diagnostics:results.filter(r=>r.status==='diagnostic').map(r=>({id:r.id,error:r.diagnostic})),timeouts:results.filter(r=>r.status==='timeout').length,nonDeterministic:results.filter(r=>r.deterministic===false).map(r=>r.id),triviaDifferences:results.filter(r=>r.triviaInvariant===false).map(r=>r.id),routeFailures:results.filter(r=>r.routeFailures?.length).map(r=>({id:r.id,failures:r.routeFailures})),semanticFailures:results.filter(r=>r.semanticFailures?.length).map(r=>({id:r.id,failures:r.semanticFailures})),presentationFailures:results.filter(r=>r.presentationFailures?.length).map(r=>({id:r.id,failures:r.presentationFailures})),paintFailures:results.filter(r=>r.paintFailures?.length).map(r=>({id:r.id,failures:r.paintFailures})),overlaps:results.filter(r=>r.metrics&&(r.metrics.nodeOverlaps.length||r.metrics.captionOverlaps.length)).map(r=>({id:r.id,nodes:r.metrics.nodeOverlaps,captions:r.metrics.captionOverlaps})),gallery:path.relative(root,path.join(out,'index.html'))},null,2));
 const baseline=report.knownDiagnostics;
 const failures=results.filter(r=>r.status==='timeout'||r.deterministic===false||r.triviaInvariant===false||r.paintFailures?.length||r.routeFailures?.length||r.presentationFailures?.length||r.semanticFailures?.length||r.metrics?.finiteGeometry===false||r.metrics?.nodeOverlaps.length||r.metrics?.captionOverlaps.length||(r.status==='diagnostic'&&(process.argv.includes('--strict')||baseline[r.id]!==r.diagnostic)));
 if(controls.some(r=>r.status!=='rendered'||r.paintFailures?.length))throw Error('Diagnostic-isolation control regression');
 if(failures.length){console.error('New regression or strict diagnostic:',failures.map(r=>r.id).join(', '));process.exitCode=1} else console.log('Audit checks passed: semantic completeness, route separation/locality, geometry, determinism and caption overlap.');
})().catch(e=>{console.error(e);process.exit(1)});
