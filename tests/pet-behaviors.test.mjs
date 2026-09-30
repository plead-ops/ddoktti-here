import test from 'node:test';import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';import ts from '../node_modules/typescript/lib/typescript.js';
const source=await readFile(new URL('../apps/desktop/src/pet-behaviors.ts',import.meta.url),'utf8');
const code=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ESNext}}).outputText;
const {behaviors,chooseBehavior,behaviorPose,clickBehavior}=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));
test('click wakes only a sleeping pet; drag release never wakes or opens menu',()=>{assert.equal(clickBehavior('sleepy',false,false),'wake');assert.equal(clickBehavior('sleepy',true,false),'none');assert.equal(clickBehavior('idle',false,false),'tickle');assert.equal(clickBehavior('sleepy',false,true),'tickle');});
test('active behaviors alternate with calm behaviors',()=>{const calm=new Set(['idle','bored','sleepy','curious']);for(const name of ['run','jump','excited','greeting','proud','shy','surprised','playful','sulking','cheering'])for(const value of [0,.3,.7,.999])assert(calm.has(chooseBehavior(name,value)));});
test('frame bounds and reduced motion suppress all jump lift',()=>{for(const b of Object.values(behaviors))for(const time of [-1,0,199,500,1500,90000]){const p=behaviorPose(b,time,false);assert(p.frame>=0&&p.frame<=3);assert(p.lift>=0);assert.equal(behaviorPose(b,time,true).lift,0);assert.equal(behaviorPose(b,time,true).frame,3);}assert(behaviorPose(behaviors.excited,400,false).lift>0);});
