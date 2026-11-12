// Larger concepts and three independent, reproducible grammar seeds.
module.exports = (options = {}) => {
 const cases=[], add=(id,concept,source)=>cases.push({id,concept,source:source.trim()+'\n'});
 const nodes=(prefix,n,label,shape='rect')=>Array.from({length:n},(_,i)=>`${prefix}${i}: ${label} ${i+1} [shape: ${shape}]`).join('\n');
 add('01-dispatch-collect','One dispatcher sends work to five workers; all results converge at a store',`api: API\nqueue: Work queue [shape: queue]\nstore: Results [shape: database]\n${nodes('w',5,'Worker')}\napi -> queue: submit\n${Array.from({length:5},(_,i)=>`queue -> w${i}: dispatch\nw${i} -> store: result`).join('\n')}`);
 for(const [id,a,b]of[['02-planar-mesh',2,4],['03-dense-mesh',4,4]])add(id,`${a} independent services each access ${b} stores`,`${nodes('a',a,'Service')}\n${nodes('b',b,'Store','database')}\n${Array.from({length:a},(_,i)=>Array.from({length:b},(_,j)=>`a${i} -> b${j}: access`).join('\n')).join('\n')}`);
 add('04-sixteen-stages','Sixteen sequential transformations should retain an obvious reading order',`${nodes('s',16,'Stage')}\n${Array.from({length:16},(_,i)=>`s${i}`).join(' -> ')}: data`);
 add('05-balanced-tree','A complete four-level hierarchy with fifteen nodes',`${nodes('n',15,'Service')}\n${Array.from({length:14},(_,i)=>`n${Math.floor(i/2)} -> n${i+1}: delegate`).join('\n')}`);
 add('06-unbalanced-tree','Three independent branches have very different depths',`root: Authorization\nroot -> a: regional\nroot -> b: global\nroot -> c: emergency\na -> a1 -> a2 -> a3 -> a4: verify\nb -> b1: record\nb -> b2: count\nc -> c1: approve`);
 add('07-multiple-retries','An eight-stage pipeline returns failures to three different earlier stages',`${nodes('s',8,'Stage')}\ns0 -> s1 -> s2 -> s3 -> s4 -> s5 -> s6 -> s7: process\ns3 -> s1: transient retry\ns5 -> s2: revalidate\ns7 -> s0: restart\ns4 -> s4: retry locally`);
 add('08-cross-ownership','Two regions share storage and a trusted scope while ownership remains distinct',`client: Browser\nwest: West { api: Regional API; worker: Worker; api -> worker: jobs }\neast: East { api: Regional API; worker: Worker; api -> worker: jobs }\ncentral: Shared services { ledger: Ledger [shape: database]; keys: Key service; ledger -> keys: decrypt }\nclient -> west.api: HTTPS\nclient -> east.api: failover\nwest.worker -> central.ledger: write\neast.worker -> central.ledger: write\nzone trusted "Shared trusted scope" around [west.api,east.api,central.ledger,central.keys] { padding: 1.5g }`);
 add('09-many-record-slots','Three services target six table rows with distinct operations',`checkout: Checkout\naudit: Audit service\nreport: Reporting\norders: Orders [shape: sql_table] {\n${['id','customer_id','status','created_at','total','audit_id'].map((p,i)=>`port ${p}: ${p.includes('id')?'uuid':p==='total'?'decimal':'text'} [side: W,pos: ${i+1}]`).join('\n')}\n}\ncheckout -> orders#id: insert\ncheckout -> orders#customer_id: identify\ncheckout -> orders#status: update\naudit -> orders#created_at: timestamp\naudit -> orders#audit_id: correlate\nreport -> orders#total: aggregate`);
 add('10-many-named-ports','Four publishers use explicit input and acknowledgement ports on one queue',`${nodes('p',4,'Publisher')}\nqueue: Event log [shape: queue] {\n${Array.from({length:4},(_,i)=>`port in${i}: input ${i+1} [side: W,ratio: ${(i+1)/9}]\nport ack${i}: ack ${i+1} [side: E,ratio: ${(i+5)/9}]`).join('\n')}\n}\n${Array.from({length:4},(_,i)=>`p${i} -> queue#in${i}: publish ${i+1}\nqueue#ack${i} -> p${i}: accepted ${i+1}`).join('\n')}`);
 add('11-mixed-typography','Different scripts, explicit line breaks and long phrases preserve hierarchy',`client: "Customer service / 東京入口" [shape: actor]\npolicy: ${JSON.stringify("Regional authorization\nand identity policy")} [shape: diamond]\ndb: ${JSON.stringify("Organisation und Mitgliedschaft\n履歴ストア")} [shape: database]\nlog: "Event ingestion and durable audit journal" [shape: queue]\nclient -> policy: "認証 / authorize membership"\npolicy -> db: "Read current organization membership and entitlement status"\npolicy -> log: "Record the decision and its verification evidence"`);
 add('12-independent-systems','Two independent pipelines and an observer should form coherent components',`observer: Observer\n${nodes('a',5,'Checkout stage')}\n${nodes('b',4,'Search stage')}\na0 -> a1 -> a2 -> a3 -> a4: order\nb0 -> b1 -> b2 -> b3: query`);
 const seeds=options.seeds || [0xc0ffee01,0xa11ce002,0x5eed1203];
 for(const seed of seeds){
  let state=seed;const random=()=>{state^=state<<13;state^=state>>>17;state^=state<<5;return(state>>>0)/4294967296},pick=a=>a[Math.floor(random()*a.length)],tag=seed.toString(16);
  for(const family of ['dag','cycle','parallel','tree','two-stage','sparse-mesh','branch-join','scoped']){
   const lines=[],seen=new Set(),n=9+Math.floor(random()*5);
   const edge=(a,b)=>{const k=`${a},${b}`;if(seen.has(k)&&family!=='parallel')return;seen.add(k);lines.push(`${a} -> ${b}: ${pick(['send','ack','read through','validate organization access','retry',''])}`)};
   if(['dag','cycle','parallel','tree'].includes(family)){
    for(let i=0;i<n;i++)lines.push(`n${i}: ${JSON.stringify(pick(['API','Worker','Regional gateway','Organization membership service','Event journal','東京 API'])+' '+i)} [shape: ${pick(['rect','queue','database','diamond','cache'])}]`);
    for(let i=1;i<n;i++)edge(`n${family==='tree'?Math.floor((i-1)/2):i-1}`,`n${i}`);
    if(family==='dag')for(let i=0;i<n;i++)for(let j=i+2;j<n;j++)if(random()<.17)edge('n'+i,'n'+j);
    if(family==='cycle'){edge('n'+(n-1),'n0');for(let i=0;i<4;i++)edge('n'+Math.floor(random()*n),'n'+Math.floor(random()*n));}
    if(family==='parallel')for(let i=0;i<6;i++)edge('n'+Math.floor(random()*(n-1)),'n'+(n-1));
   }else if(family==='two-stage'){
    lines.push('root: Dispatch','a: Region A','b: Region B',nodes('w',6,'Worker'));
    edge('root','a');edge('root','b');for(let i=0;i<6;i++)edge(i<3?'a':'b','w'+i);
   }else if(family==='sparse-mesh'){
    lines.push(nodes('a',5,'Service'),nodes('b',5,'Store','database'));
    for(let i=0;i<5;i++)for(let j=0;j<5;j++)if(i===j||random()<.42)edge('a'+i,'b'+j);
   }else if(family==='branch-join'){
    lines.push('entry: Entry','exit: Exit');for(let i=0;i<3;i++){lines.push(`d${i}: Allowed ${i+1}? [shape: diamond]`,`yes${i}: Process ${i+1}`,`no${i}: Skip ${i+1}`,`join${i}: Merge ${i+1}`);edge(i?'join'+(i-1):'entry','d'+i);edge('d'+i,'yes'+i);edge('d'+i,'no'+i);edge('yes'+i,'join'+i);edge('no'+i,'join'+i);}edge('join2','exit');
   }else {
    lines.push('client: Client','left: West { api: API; queue: Work queue [shape: queue]; worker: Worker; api -> queue -> worker: jobs }','right: East { api: API; queue: Work queue [shape: queue]; worker: Worker; api -> queue -> worker: jobs }');
    edge('client','left.api');edge('client','right.api');edge('left.worker','right.queue');edge('right.worker','left.queue');
   }
   add(`${tag}-${family}`,`${family} concept; seed 0x${tag}`,lines.join('\n'));
  }
 }
 add('control-scoped-no-captions','Same scoped feedback graph with all captions removed',`client: Client
left: West { api: API; queue: Work queue [shape: queue]; worker: Worker; api -> queue -> worker }
right: East { api: API; queue: Work queue [shape: queue]; worker: Worker; api -> queue -> worker }
client -> left.api
client -> right.api
left.worker -> right.queue
right.worker -> left.queue`);
 add('control-scoped-flat','Same feedback topology without ownership containers',`client -> left_api: request\nclient -> right_api: request\nleft_api -> left_queue -> left_worker: jobs\nright_api -> right_queue -> right_worker: jobs\nleft_worker -> right_queue: send\nright_worker -> left_queue: retry`);
 add('control-scoped-pinned','A feasible pinned witness for the scoped feedback topology',`client: Client @pin(grid: -50,0)
left: West @pin(grid: 0,0,lock: [w: 65g,h: 32g]) {
 api: API @pin(grid: -24,0)
 queue: Work queue [shape: queue] @pin(grid: 0,0)
 worker: Worker @pin(grid: 24,0)
 api -> queue -> worker: jobs
}
right: East @pin(grid: 100,0,lock: [w: 65g,h: 32g]) {
 api: API @pin(grid: 76,0)
 queue: Work queue [shape: queue] @pin(grid: 100,0)
 worker: Worker @pin(grid: 124,0)
 api -> queue -> worker: jobs
}
client -> left.api: request
client -> right.api: request
left.worker -> right.queue: send
right.worker -> left.queue: retry`);
 return {seeds:seeds.map(s=>'0x'+s.toString(16)),cases};
};
