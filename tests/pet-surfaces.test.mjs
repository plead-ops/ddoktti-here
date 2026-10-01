import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import ts from '../apps/desktop/node_modules/typescript/lib/typescript.js';
const source=fs.readFileSync(new URL('../apps/desktop/src/pet-surfaces.ts',import.meta.url),'utf8');
const code=ts.transpileModule(source.replace("import {climbDistance} from './pet-climb';",fs.readFileSync(new URL('../apps/desktop/src/pet-climb.ts',import.meta.url),'utf8')),{compilerOptions:{module:ts.ModuleKind.ESNext}}).outputText;
const {SurfaceMotion,exposedLedges}=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));
const windowRect=(id,x,y,width=350,height=450)=>({id,x,y,width,height});
const world=(windows=[],x=400,y=700)=>({monitor:'main',width:1000,height:700,size:100,x,y,windows});
const input={walk:0,autonomous:false,reduced:false};
const advance=(m,seconds,opts={})=>{for(let t=0;t<seconds;t+=1/60)m.step(1/60,{...input,...opts});};

test('occluded ledges are split; title bars with no room above are excluded',()=>{
 const ps=exposedLedges(world([windowRect('front',350,100,200,300),windowRect('back',100,300,700),windowRect('top',0,20)]));
 assert.deepEqual(ps.filter(p=>p.id==='back').map(p=>[p.left,p.right]),[[100,350],[550,800]]);
 assert(!ps.some(p=>p.id==='top'));
});
test('drop lands on highest exposed ledge crossed, not desktop',()=>{
 const m=new SurfaceMotion(world([windowRect('high',250,260),windowRect('low',200,500)],400,100));
 advance(m,2);assert.equal(m.supportId,'high');assert.equal(m.y,260);assert.equal(m.motion,'hurt');advance(m,2);assert.equal(m.motion,'grounded');
});
test('both feet can support the pet close to a window edge; unsupported toes fall',()=>{
 for(const [x,y,id] of [[260,260,'a'],[253,700,null]]) {
 const m=new SurfaceMotion(world([windowRect('a',250,260)],x,100));advance(m,3);assert.equal(m.y,y);assert.equal(m.supportId,id);
 }
});
test('stable window ID follows move and resize, including during an alert',()=>{
 const m=new SurfaceMotion(world([windowRect('a',200,300,400)],400,300));
 m.updateWorld(world([windowRect('a',300,250,500)]));advance(m,.1);
 assert.equal(m.x,550);assert.equal(m.y,250);assert.equal(m.supportId,'a');
});
test('closing/minimizing or covering the supporting window starts falling',()=>{
 for(const windows of [[],[windowRect('front',100,100,700),windowRect('a',200,300)]]){
  const m=new SurfaceMotion(world([windowRect('a',200,300)],400,300));m.updateWorld(world(windows));assert.equal(m.motion,'fall');advance(m,3);assert.equal(m.y,700);
 }
});
test('walk to a side, grip, alternate climb, pull over corner, settle on top',()=>{
 const m=new SurfaceMotion(world([windowRect('a',250,250,450,450)],190,700),()=>.9);
 const states=new Set();for(let i=0;i<700;i++){m.step(1/60,{...input,walk:70,autonomous:true});states.add(m.motion);if(m.supportId==='a'&&m.motion==='grounded')break;}
 for(const state of ['grab','climb','pull','grounded'])assert(states.has(state),state);
 assert(!states.has('land'),'no bracing after a climb');assert(m.climbed);
 assert.equal(m.y,250);assert.equal(m.x,250+m.foot+3);
});
test('support window moves during climbing; lost wall triggers fall',()=>{
 const m=new SurfaceMotion(world([windowRect('a',250,250,450,450)],200,700));advance(m,.2,{walk:70,autonomous:true});
 assert.equal(m.motion,'grab');m.updateWorld(world([windowRect('a',280,230,450,450)]));assert.equal(m.x,234);
 m.updateWorld(world([]));assert.equal(m.motion,'fall');advance(m,3);assert.equal(m.y,700);
});
test('autonomous travel jump has preparation and lands on another window',()=>{
 const m=new SurfaceMotion(world([windowRect('a',100,500,300,200),windowRect('b',480,420,350,280)],350,500),()=>0);
 const states=new Set();for(let i=0;i<600;i++){m.step(1/60,{...input,autonomous:true});states.add(m.motion);if(m.supportId==='b'&&m.motion==='grounded')break;}
 for(const state of ['prepare','jump','land'])assert(states.has(state),state);
 assert.equal(m.supportId,'b');assert.equal(m.y,420);
});
test('removed jump destination does not cause teleport or floating',()=>{
 const m=new SurfaceMotion(world([windowRect('a',100,500,300),windowRect('b',480,420)],350,500),()=>0);
 for(let i=0;i<600&&m.motion!=='jump';i++)m.step(1/60,{...input,autonomous:true});assert.equal(m.motion,'jump');
 m.updateWorld(world([windowRect('a',100,500,300)]));advance(m,3);assert.equal(m.y,700);
});
test('edge wobble normally recovers; occasional slip falls',()=>{
 for(const r of [.9,0]){
 const m=new SurfaceMotion(world([windowRect('a',100,300,300)],392,300),()=>r);
 advance(m,.1,{walk:60,autonomous:true});assert.equal(m.motion,'wobble');advance(m,.7,{autonomous:true});
 if(r===.9){assert.equal(m.motion,'grounded');assert.equal(m.direction,-1);}else assert.equal(m.motion,'fall');
 }
});
test('reduced motion resolves a removed surface instantly and stops voluntary travel',()=>{
 const m=new SurfaceMotion(world([windowRect('a',100,300,400)],300,300));m.updateWorld(world([]));advance(m,.1,{reduced:true,walk:100,autonomous:true});assert.equal(m.y,700);assert.equal(m.x,300);assert.equal(m.motion,'grounded');
});
test('menu and alerts prevent new jumps while an airborne motion still completes',()=>{
 const m=new SurfaceMotion(world([windowRect('a',100,500,300),windowRect('b',480,420)],350,500),()=>0);
 advance(m,20);assert.equal(m.motion,'grounded');assert.equal(m.x,350);
 for(let i=0;i<1000&&m.motion!=='jump';i++)m.step(1/60,{...input,autonomous:true});assert.equal(m.motion,'jump');advance(m,3);assert.equal(m.supportId,'b');assert.equal(m.motion,'grounded');
});
test('monitor or size change resets stale support and respects bounds',()=>{
 const m=new SurfaceMotion(world([windowRect('a',100,300,400)],300,300));m.updateWorld({...world([],40,100),monitor:'secondary',width:700,size:180});advance(m,3);assert.equal(m.supportId,null);assert.equal(m.y,700);assert(m.x>=m.half);
});
test('delayed frames cannot tunnel through a ledge',()=>{
 const m=new SurfaceMotion(world([windowRect('a',100,550,600)],300,100));for(let i=0;i<25;i++)m.step(.1,input);assert.equal(m.supportId,'a');assert.equal(m.y,550);
});

test('caller mutation cannot erase previous window position before reconciliation',()=>{
 const w=world([windowRect('a',200,300,400)],400,300),m=new SurfaceMotion(w);
 w.windows[0].x+=40;w.windows[0].y-=30;m.updateWorld(w);assert.equal(m.x,440);assert.equal(m.y,270);assert.equal(m.supportId,'a');
});
test('fully covered wall cannot be climbed',()=>{
 const m=new SurfaceMotion(world([windowRect('front',100,100,500,600),windowRect('back',250,250,450,450)],190,700),()=>.9);
 advance(m,1,{walk:70,autonomous:true});assert.notEqual(m.supportId,'back');assert.notEqual(m.motion,'climb');
});
test('notification arriving during preparation cancels jump before takeoff',()=>{
 const m=new SurfaceMotion(world([windowRect('a',100,500,300),windowRect('b',480,420)],350,500),()=>0);
 for(let i=0;i<800&&m.motion!=='prepare';i++)m.step(1/60,{...input,autonomous:true});assert.equal(m.motion,'prepare');advance(m,.1);assert.equal(m.motion,'grounded');assert.equal(m.supportId,'a');
});

test('rope anchor follows its window, with climbing pauses between pulls',()=>{
 const m=new SurfaceMotion(world([windowRect('a',250,250,450,450)],200,700));
 advance(m,.2,{walk:70,autonomous:true});assert.deepEqual(m.ropeAnchor,{x:250,y:250});
 m.updateWorld(world([windowRect('a',280,230,450,470)]));assert.deepEqual(m.ropeAnchor,{x:280,y:230});
 while(m.motion==='grab')advance(m,1/60);const y=m.y;advance(m,.1);assert.equal(m.y,y);advance(m,.4);assert(m.y<y);
 m.reset(500,400);assert.equal(m.ropeAnchor,null);
});


test('high accidental drops hurt and recover; low drops only land',()=>{
 for(const [y,expected] of [[100,'hurt'],[650,'land']]) {
  const m=new SurfaceMotion(world([],400,y));
  while(m.motion==='fall')m.step(1/60,input);
  assert.equal(m.motion,expected);assert.equal(m.y,700);
  advance(m,3);assert.equal(m.motion,'grounded');
 }
});
test('voluntary descending jumps never play the injury reaction',()=>{
 const m=new SurfaceMotion(world([windowRect('a',100,350,300),windowRect('b',480,540)],350,350),()=>0);
 for(let i=0;i<900&&m.motion!=='jump';i++)m.step(1/60,{...input,autonomous:true});
 assert.equal(m.motion,'jump');
 while(m.motion==='jump')m.step(1/60,input);
 assert.equal(m.motion,'land');assert.equal(m.supportId,'b');
});
test('small front window and exposed inactive back window both accept landings',()=>{
 const windows=[windowRect('front',350,200,40,100),windowRect('inactive',100,360,700,340)];
 for(const [x,id,y] of [[370,'front',200],[200,'inactive',360]]) {
  const m=new SurfaceMotion(world(windows,x,100));advance(m,2);
  assert.equal(m.supportId,id);assert.equal(m.y,y);
 }
 const m=new SurfaceMotion(world(windows,200,360));
 m.updateWorld(world([...windows].reverse()));assert.equal(m.supportId,'inactive');assert.equal(m.motion,'grounded');
});
test('60 and 120 Hz movement keep comparable fall paths and identical walking speed',()=>{
 const run=(hz,fall)=>{const m=new SurfaceMotion(world([],200,fall?100:700));for(let i=0;i<hz*.5;i++)m.step(1/hz,{...input,walk:80,autonomous:!fall});return m;};
 assert(Math.abs(run(60,true).y-run(120,true).y)<1e-8);
 assert(Math.abs(run(60,false).x-run(120,false).x)<1e-8);
});


test('narrow exposed strip supports climbing and jumping without impossible extra margins',()=>{
 const m=new SurfaceMotion(world([windowRect('front',266,100,450,400),windowRect('back',250,250,350,450)],204,700));
 for(let i=0;i<1000&&!m.climbed;i++)m.step(1/60,{...input,walk:70,autonomous:true});
 assert.equal(m.supportId,'back');assert.equal(m.motion,'grounded');assert.equal(m.x,259);
 const j=new SurfaceMotion(world([windowRect('source',100,500,300),windowRect('narrow',480,420,16)],350,500),()=>0);
 for(let i=0;i<900&&j.motion!=='jump';i++)j.step(1/60,{...input,autonomous:true});
 assert.equal(j.motion,'jump');advance(j,2);assert.equal(j.supportId,'narrow');assert.equal(j.y,420);
});

test('restless perch ropes down its own window side and lands softly on the floor',()=>{
 for(const [x,side,edge] of [[300,1,250],[650,-1,700]]){
  const m=new SurfaceMotion(world([windowRect('a',250,150,450,550)],x,150),()=>.3);
  advance(m,1);assert.equal(m.supportId,'a');assert.equal(m.descentUrge,0);
  m.perch=200;m.cooldown=0;assert(m.descentUrge>.85);
  const states=new Set();
  for(let i=0;i<3600;i++){m.step(1/60,{...input,walk:80,autonomous:true});states.add(m.motion);
   if(m.motion==='descend'){assert.deepEqual(m.ropeAnchor,{x:edge,y:150});assert.equal(m.x,edge-side*m.half);}
   if(m.climbed)break;}
  for(const s of ['lower','descend','grounded'])assert(states.has(s),s);
  assert(!states.has('land'),'stepping off the rope is not a landing');assert(!states.has('hurt'));assert(!states.has('fall'));m.climbed=false;
  assert.equal(m.y,700);assert.equal(m.supportId,null);assert.equal(m.ropeAnchor,null);
  advance(m,1,{autonomous:true});assert.equal(m.motion,'grounded');assert.equal(m.perch,0);
 }
});
test('short drop hops from the corner instead of roping and never hurts',()=>{
 const m=new SurfaceMotion(world([windowRect('a',300,560,300,140)],560,560),()=>.3);
 advance(m,1);m.perch=200;m.cooldown=0;
 const states=new Set();
 for(let i=0;i<3600;i++){m.step(1/60,{...input,walk:80,autonomous:true});states.add(m.motion);if(m.motion==='land'||m.motion==='hurt')break;}
 for(const s of ['prepare','jump'])assert(states.has(s),s);
 assert(!states.has('lower'));assert.equal(m.motion,'land');assert.equal(m.y,700);assert.equal(m.supportId,null);assert(m.x>600);
});
test('descent needs an exposed corner, a clear wall and a real drop; the floor is never restless',()=>{
 const m=new SurfaceMotion(world([windowRect('a',250,150,450,550)],400,150));advance(m,1);
 assert(m.descentTarget(1));assert(m.descentTarget(-1));
 m.updateWorld(world([windowRect('front',60,250,120,300),windowRect('a',250,150,450,550)],400,150));
 assert.equal(m.descentTarget(1),null);assert(m.descentTarget(-1));
 m.updateWorld(world([windowRect('corner',600,100,200,200),windowRect('a',250,150,450,550)],400,150));
 assert.equal(m.descentTarget(-1),null);
 const low=new SurfaceMotion(world([windowRect('low',250,650,450,50)],400,650));advance(low,1);assert.equal(low.descentTarget(1),null);
 const floor=new SurfaceMotion(world([],400,700));advance(floor,1);floor.perch=500;assert.equal(floor.descentUrge,0);assert.equal(floor.descentTarget(1),null);
});
test('descent follows a moving window, falls when it closes and resolves under reduced motion',()=>{
 const start=()=>{const m=new SurfaceMotion(world([windowRect('a',250,150,450,550)],300,150),()=>.3);advance(m,1);m.perch=200;m.cooldown=0;
  for(let i=0;i<3600&&!(m.motion==='descend'&&m.age>.5);i++)m.step(1/60,{...input,walk:80,autonomous:true});assert.equal(m.motion,'descend');return m;};
 const m=start();const {x,y}=m;
 m.updateWorld(world([windowRect('a',280,170,450,530)],x,y));
 assert.equal(m.motion,'descend');assert.equal(m.x,x+30);assert.equal(m.y,y+20);assert.deepEqual(m.ropeAnchor,{x:280,y:170});
 advance(m,1);assert(['descend','grounded'].includes(m.motion));
 m.updateWorld(world([]));assert.equal(m.motion,'fall');assert.equal(m.ropeAnchor,null);
 const r=start();r.step(.1,{...input,walk:80,autonomous:true,reduced:true});assert.equal(r.motion,'grounded');assert.equal(r.y,700);
});
test('settled perch keeps climbing while restlessness accumulates with time and altitude',()=>{
 const m=new SurfaceMotion(world([windowRect('low',100,500,300,200),windowRect('high',500,200,300,500)],250,500));
 advance(m,1);assert.equal(m.supportId,'low');const perch=m.perch;advance(m,10);assert(Math.abs(m.perch-perch-10)<1e-6);assert.equal(m.descentUrge,0);
 m.perch=120;const urge=m.descentUrge;assert(urge>.6&&urge<.7,String(urge));m.y=200;assert(m.descentUrge>urge);
});

test('restless character escapes a window pocket to the floor instead of re-climbing (desktop snapshot)',()=>{
 const spec='289,194,2321,1522 728,286,2110,1580 1689,573,1693,1278 245,368,1427,904 1330,646,1716,1394 17,439,559,966 1998,177,1567,1023 423,950,1189,904 473,342,1919,1205 458,561,1716,1394 59,19,1665,1450 956,57,1716,1394 927,28,1716,1394 1986,404,1854,1112 1122,0,1496,1200 831,613,1716,1394 210,755,1496,1222';
 const windows=spec.split(' ').map((s,i)=>{const [x,y,width,height]=s.split(',').map(Number);return {id:'w'+i,x,y,width,height};});
 const m=new SurfaceMotion({monitor:'m',width:3840,height:2040,size:204,x:1500,y:194,windows},()=>.3);
 advance(m,2);assert.equal(m.supportId,'w0');
 m.perch=200;m.cooldown=0;
 let descended=false,reclimbs=0,floorAt=null;
 for(let i=0;i<600*60;i++){m.step(1/60,{...input,walk:65,autonomous:true});
  if(m.motion==='descend')descended=true;
  if(descended&&m.motion==='grab')reclimbs++;
  if(!m.supportId&&m.y>=2040-1&&m.motion==='grounded'){floorAt=i/60;break;}}
 assert(descended,'ropes down off the big window first');
 assert.equal(reclimbs,0,'never re-climbs the window it just left');
 assert(floorAt!==null&&floorAt<300,'reaches the floor within five minutes: '+floorAt);
 assert.notEqual(m.motion,'hurt');
});
