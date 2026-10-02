import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import ts from '../node_modules/typescript/lib/typescript.js';
const root=new URL('../',import.meta.url);
const read=path=>readFile(new URL(path,root),'utf8');
const frames=JSON.parse(await read('apps/desktop/src/pet-vector-frames.json'));
const artwork=JSON.parse(await read('apps/desktop/src/pet-vector-paths.json'));
const palette=new Set(Object.values(JSON.parse(await read('apps/desktop/src/pet-palette.json'))));
const source=(await read('apps/desktop/src/pet-vector.ts'))
 .replace("import {gaitFrame} from './pet-gait';",(await read('apps/desktop/src/pet-gait.ts')).replace("import gaits from './pet-gaits.json';",'const gaits='+ await read('apps/desktop/src/pet-gaits.json')+';'))
 .replace("import {climbFrame,descendFrame} from './pet-climb';",await read('apps/desktop/src/pet-climb.ts'))
 .replace("import artwork from './pet-vector-paths.json';",`const artwork=${JSON.stringify(artwork)};`)
 .replace("import frames from './pet-vector-frames.json';",`const frames=${JSON.stringify(frames)};`)
 .replace("import { behaviors, behaviorPose, napPose, idlePose, alertLoop } from './pet-behaviors';",await read('apps/desktop/src/pet-behaviors.ts'));
const js=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ESNext}}).outputText;
const {VECTOR_POSES,vectorFrame}=await import('data:text/javascript;base64,'+Buffer.from(js).toString('base64'));
test('all traced frames are real vector paths with no embedded bitmaps',async()=>{
 let count=0;
 for(const sheet of Object.values(frames))for(const frame of sheet.frames){
  const svg=await read('apps/desktop/public'+frame.url);
  assert.match(svg,/<path /);assert.doesNotMatch(svg,/<image|data:image|<script/i);
  assert(frame.hit.length>0);count++;
 }
 assert.equal(count,149);
});
test('shared palette and measurements keep all original and recovery frames consistent',()=>{
 for(const sheet of Object.values(frames))for(const frame of sheet.frames){
  assert.equal(frame.eyeSpan,90);assert(frame.bodyHeight>0);
  assert.equal(frame.bottom,250);
  for(const match of artwork[frame.key].matchAll(/fill="(#[A-Fa-f0-9]{6})"/g))assert(palette.has(match[1]),match[1]);
  assert(frame.source.width>0&&frame.source.height>0);
 }
});
test('surface crops follow real row gaps and missing antennas are repaired',()=>{
 const surface=frames['surfaces-v2'].frames;
 assert.notEqual(surface[12].source.y,927);
 assert.deepEqual(surface.flatMap((f,i)=>f.antennaRepaired?[i]:[]),[4,13,15]);
 for(const frame of surface){assert(frame.top>0);assert(frame.left>0&&frame.right<400);}
});
test('walking feet stay grounded; running retains four original registered frames',()=>{
 for(let i=0;i<8;i++){
  const f=vectorFrame('walk',i*130);
  assert.equal(f.index,i);assert(Math.abs(f.top+f.frame.bottom*f.scale)<.001);
 }
 for(let i=0;i<8;i++){
  const f=vectorFrame('run',i*115);
  assert.equal(f.sheet,'run-v3');assert.equal(f.index,i%4);
  assert(Math.abs(f.top+f.frame.bottom*f.scale)<.001);
 }
});
test('every pose has finite rendering data and reduced motion holds the same frame',()=>{
 for(const pose of VECTOR_POSES)for(const t of [-100,0,150,600,1800,90000]){
  const f=vectorFrame(pose,t);assert(f.frame);assert(Number.isFinite(f.left+f.top+f.scale));
  assert.deepEqual(vectorFrame(pose,t,true),vectorFrame(pose,0,true));
 }
 assert.equal(vectorFrame('greeting',800).index,6);
 assert.equal(vectorFrame('slack',0).index,0);
 assert.equal(vectorFrame('calendar',2000).index,7);
});

test('all traced curves fit inside the frame viewport, including repaired antennas',()=>{
 for(const [key,svg] of Object.entries(artwork))for(const match of svg.matchAll(/d="([^"]+)"/g)){
  const numbers=[...match[1].matchAll(/-?\d+\.\d+/g)].map(m=>Number(m[0]));
  for(let i=0;i<numbers.length;i+=2){
   assert(numbers[i]>=0&&numbers[i]<=400,`${key} clipped horizontally`);
   assert(numbers[i+1]>=0&&numbers[i+1]<=260,`${key} clipped vertically`);
  }
 }
});

test('every full pose retains its source aspect ratio with one uniform scale',()=>{
 for(const sheet of Object.values(frames))for(const frame of sheet.frames){
  const [l,t,r,b]=frame.sourceBounds;
  const sx=(frame.right-frame.left)/(r-l),sy=(frame.bottom-frame.top)/(b-t);
  assert(Math.abs(sx-sy)<1e-9,`${frame.key}: non-uniform body scaling`);
  assert(Math.abs(sx-frame.uniformScale)<1e-9);
  const expectedBody=(b-frame.source.neck)*frame.uniformScale;
  assert(Math.abs(frame.bodyHeight-expectedBody)<.001,`${frame.key}: compressed lower body`);
 }
});

test('rope hands regenerate from reviewed source coordinates with the same uniform transform',async()=>{
 const guides=JSON.parse(await read('assets/concepts/pet-climb-hand-guides.json'));
 const hands=JSON.parse(await read('apps/desktop/src/pet-climb-hands.json'));
 for(const [i,points] of Object.entries(guides)){
  const f=frames['surfaces-v2'].frames[Number(i)];
  const transformed=points.map(([x,y])=>[Number((200+(x-f.source.headCenter)*f.uniformScale).toFixed(2)),Number((250+(y-f.sourceBounds[3])*f.uniformScale).toFixed(2))]);
  assert.deepEqual(hands[i],transformed);
 }
});

test('landing, hurt and sleepy use traced imagegen sources without synthetic body or mouth overlays',async()=>{
 const updated=[frames['edge-v2'].frames[6],...frames['hurt-v1'].frames,...frames['behaviors-v2'].frames.slice(8,12)];
 assert.equal(updated.length,10);
 for(const frame of updated){
  assert.match(frame.sourceImage,/^assets\/concepts\/generated\/.+\.png$/);
  const png=await readFile(new URL(frame.sourceImage,root));
  assert.equal(png.subarray(1,4).toString(),'PNG');
  assert.doesNotMatch(artwork[frame.key],/hurt-head|<ellipse|<circle/);
 }
});
test('belly flop progresses from prone rest to standing without replaying impact',()=>{
 const times=[0,350,1150,1750,2300,2750];
 assert.deepEqual(times.map(t=>vectorFrame('hurt',t).index),[0,1,2,3,4,4]);
 for(const f of frames['hurt-v1'].frames)assert(f.sourceImage.endsWith('landing-flop-v3.png'));
});
