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
 advance(m,2);assert.equal(m.supportId,'high');assert.equal(m.y,260);assert.equal(m.motion,'grounded');
});
test('a toe alone is not enough support; fall continues to floor',()=>{
 const m=new SurfaceMotion(world([windowRect('a',250,260)],260,100));advance(m,3);assert.equal(m.y,700);assert.equal(m.supportId,null);
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
 for(const state of ['grab','climb','pull','land','grounded'])assert(states.has(state),state);
 assert.equal(m.y,250);assert(m.x>250+46);
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
 const m=new SurfaceMotion(world([windowRect('a',100,300,300)],353,300),()=>r);
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
