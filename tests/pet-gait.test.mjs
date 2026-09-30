import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import ts from '../node_modules/typescript/lib/typescript.js';
const read=path=>readFile(new URL('../apps/desktop/src/'+path,import.meta.url),'utf8');
const profile=JSON.parse(await read('pet-gaits.json'));
const source=(await read('pet-gait.ts')).replace("import gaits from './pet-gaits.json';",`const gaits=${JSON.stringify(profile)};`);
const code=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ESNext}}).outputText;
const {gaitSpeed,gaitElapsed,gaitFrame}=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));
test('one art cycle travels one scaled stride at every size and speed',()=>{
 for(const mode of ['walk','run'])for(const size of [110,180,245])for(const speed of [.5,1,2]){
  const cycle=profile[mode].frameMs*profile[mode].frames;
  const distance=gaitSpeed(mode,size,speed)*cycle/1000/speed;
  assert(Math.abs(distance-profile[mode].stride*size/260)<1e-8);
  assert(Math.abs(gaitElapsed(mode,distance,size)-cycle)<1e-8);
 }
});
test('stopping distance stops the gait clock; wall time is not an input',()=>{
 const distance=23,size=180;
 const stopped=gaitElapsed('walk',distance,size);
 assert.equal(gaitFrame('walk',stopped),gaitFrame('walk',gaitElapsed('walk',distance,size)));
 assert.equal(gaitSpeed('idle',size),0);assert.equal(gaitElapsed('walk',0,size),0);
 assert.equal(gaitFrame('run',1000,true),3);assert.equal(gaitFrame('walk',1000,true),0);
});
test('running covers more distance per cycle and per second than walking',()=>{
 assert(profile.run.stride>profile.walk.stride);assert(gaitSpeed('run',180)>gaitSpeed('walk',180));
});
