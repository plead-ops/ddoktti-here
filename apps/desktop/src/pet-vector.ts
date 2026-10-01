import {gaitFrame} from './pet-gait';
import {climbFrame} from './pet-climb';
/** SVG paths traced from the original poses, with reviewed crops, palette and proportions.
 * Preview selection and alignment follow native_pet/art.rs; locomotion uses shared gait data.
 */
import frames from './pet-vector-frames.json';
import artwork from './pet-vector-paths.json';
import { behaviors, behaviorPose } from './pet-behaviors';
export const VECTOR_POSES=['chase','petted','hurt','hang','peek','enough','ack','idle','walk','run','bored','sleepy','jump','excited','greeting','proud','shy','curious','surprised','playful','sulking','cheering','relieved','tickle','drag','dizzy','wobble','fall','land','grab','climb','pull','prepare','travel-jump','slack','calendar','timer','stretch'] as const;
export type VectorPose=typeof VECTOR_POSES[number];
export interface VectorOptions {direction?:number;reduced?:boolean;x?:number;y?:number;scale?:number}
type Sheet=keyof typeof frames;
const behaviorSheets:Record<string,[Sheet,number]>={run:['run-v3',0],bored:['behaviors-v2',1],sleepy:['behaviors-v2',2],jump:['behaviors-v2',3],excited:['emotions-v1',0],greeting:['emotions-v1',1],proud:['emotions-v1',2],shy:['emotions-v1',3],curious:['emotions-v1',4],surprised:['emotions-v2',0],playful:['emotions-v2',1],sulking:['emotions-v2',2],cheering:['emotions-v2',3],relieved:['emotions-v2',4]};
const n=(value:number)=>Number(value.toFixed(3));
export function vectorFrame(pose:string,elapsed=0,reduced=false){
 const t=reduced?0:Math.max(0,Number.isFinite(elapsed)?elapsed:0);
 let sheet:Sheet='edge',index=3,lift=0,offset=0;
 const b=behaviors[pose],registered=behaviorSheets[pose];
 if(pose==='run'||pose==='chase'){sheet=pose==='chase'?'chase-v1':'run-v3';index=gaitFrame('run',t,reduced);}
 else if(b&&registered){const p=behaviorPose(b,t,reduced);sheet=registered[0];index=registered[1]*4+p.frame;lift=p.lift;}
 else switch(pose){
  case 'hurt':sheet='hurt-v1';index=t<350?0:t<1150?1:t<1750?2:t<2300?3:4;break;
  case 'petted':sheet='petted-v1';index=Math.floor(t/450)%4;offset=reduced?0:Math.sin(t/1000*Math.PI*2)*2;break;
  case 'hang':sheet='surfaces-v2';index=Math.floor(t/600)%2===0?13:15;lift=index===15?3.8:0;break;
  case 'peek':sheet='surfaces-v2';index=12;break;
  case 'enough':sheet='emotions-v2';index=8+Math.min(2,Math.floor(t/200));break;
  case 'ack':sheet='emotions-v1';index=8+Math.min(3,Math.floor(t/400));break;
  case 'idle':sheet='emotions-v2';index=18;break;
  case 'walk':sheet='walk';index=gaitFrame('walk',t,reduced);break;
  case 'tickle':sheet='interactions-v2';index=Math.floor(t/160)%4;break;
  case 'drag':sheet='interactions-v2';index=4+Math.floor(t/230)%4;lift=12;break;
  case 'dizzy':sheet='dizzy-v1';index=Math.floor(t/300)%4;lift=12;break;
  case 'wobble':index=Math.min(3,Math.floor(t/250));break;
  case 'fall':sheet='edge-v2';index=5;break;
  case 'land':sheet='edge-v2';index=6;break;
  case 'grab':case 'climb':case 'pull':case 'prepare':case 'travel-jump':
   sheet='surfaces-v2';
   index=pose==='grab'?12+Math.min(3,Math.floor(t/100)):pose==='climb'?climbFrame(t/1000):pose==='pull'?4+Math.min(3,Math.floor(t/175)):pose==='prepare'?8:t<150?9:10;
   break;
  case 'slack':case 'calendar':case 'timer':case 'stretch':
   sheet='alerts';index=['slack','calendar','timer','stretch'].indexOf(pose)*4+(reduced?3:Math.min(3,Math.floor(t/500)));
   if(!reduced&&t>800&&t<1400)lift=Math.sin((t-800)/600*Math.PI)*18;
   break;
 }
 const frame=frames[sheet].frames[index]!;
 const scale=1;
 if(['grab','climb','pull'].includes(pose))offset=(119.6-(frame.right-200))*(pose==='pull'?1-Math.min(1,t/700):1);
 return {sheet,index,frame,scale,left:-200+offset,top:-250-lift};
}
function content(frame:ReturnType<typeof vectorFrame>['frame']){
 const rects=frame.hit.map(([x,y,w,h])=>`<rect data-hit="true" x="${x}" y="${y}" width="${w}" height="${h}" fill="none"/>`).join('');
 return artwork[frame.key as keyof typeof artwork]+rects;
}
function placement(pose:string,elapsed:number,options:VectorOptions){
 const frame=vectorFrame(pose,elapsed,options.reduced),direction=options.direction===-1?-1:1,scale=options.scale??1;
 return {frame,outer:`translate(${n(options.x??200)} ${n(options.y??250)}) scale(${n(direction*scale)} ${n(scale)})`,inner:`translate(${n(frame.left)} ${n(frame.top)})`};
}
export function vectorPetMarkup(pose:string,elapsed=0,options:VectorOptions={}){
 const p=placement(pose,elapsed,options);
 return `<g transform="${p.outer}"><g transform="${p.inner}">${content(p.frame.frame)}</g></g>`;
}
export function vectorPetSvg(pose:string,elapsed=0,options:VectorOptions={}){
 return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 260">${vectorPetMarkup(pose,elapsed,options)}</svg>`;
}
const NS='http://www.w3.org/2000/svg';
interface RenderState {root:SVGGElement;position:SVGGElement;frames:Map<string,SVGGElement>;active:SVGGElement|null}
const mounted=new WeakMap<SVGSVGElement,RenderState>();
/** Cache inline path groups in each owning document, including srcdoc previews.
 * Frame switches are synchronous visibility changes: no image fetch/decode or DOM replacement.
 */
export function renderVectorPet(target:SVGSVGElement,pose:string,elapsed=0,options:VectorOptions={}){
 let state=mounted.get(target);
 if(!state){
  const root=target.ownerDocument.createElementNS(NS,'g'),position=target.ownerDocument.createElementNS(NS,'g');
  root.append(position);target.replaceChildren(root);
  state={root,position,frames:new Map(),active:null};mounted.set(target,state);
 }
 const p=placement(pose,elapsed,options),key=p.frame.frame.key;
 let group=state.frames.get(key);
 if(!group){
  group=target.ownerDocument.createElementNS(NS,'g');group.dataset.poseFrame=key;
  group.innerHTML=content(p.frame.frame);group.style.display='none';
  state.position.append(group);state.frames.set(key,group);
 }
 if(state.active!==group){
  if(state.active)state.active.style.display='none';
  group.style.display='';state.active=group;
 }
 state.root.setAttribute('transform',p.outer);state.position.setAttribute('transform',p.inner);
}
