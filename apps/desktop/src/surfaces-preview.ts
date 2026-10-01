import {gaitSpeed,gaitElapsed} from './pet-gait';
import {renderPetRope} from './pet-rope';
import {SurfaceMotion,type SurfaceWorld,type SurfaceWindow} from './pet-surfaces';
import {renderVectorPet} from './pet-vector';
const $=<T extends Element=HTMLElement>(id:string)=>document.getElementById(id) as unknown as T;
const canvas=$<SVGSVGElement>('pet-canvas'),desktop=$('desktop');
let gaitDistance=0;
let windows:SurfaceWindow[]=[],motion:SurfaceMotion,last=0,elapsed=0,notification=false,proudUntil=0;
const names:Record<string,string>={grounded:'창과 바닥을 산책해요',fall:'아래 발판으로 떨어져요',land:'사뿐히 착지해요',prepare:'다음 창으로 갈 준비',jump:'폴짝, 창 사이를 건너요',grab:'가장자리를 붙잡아요',climb:'한 발씩 올라가요',pull:'몸을 끌어올려요',lower:'모서리에서 줄을 타요',descend:'줄 타고 내려와요',wobble:'아슬아슬, 균형을 잡아요'};
function world():SurfaceWorld{return {monitor:'preview',x:130,y:700,width:1000,height:700,size:150,windows:windows.map(w=>({...w}))};}
function paintWindows(){windows.forEach((w,i)=>{Object.assign($(w.id).style,{left:`${w.x}px`,top:`${w.y}px`,width:`${w.width}px`,height:`${w.height}px`,zIndex:String(10+windows.length-i)});});for(const id of ['window-a','window-b'])$(id).hidden=!windows.some(w=>w.id===id);motion.updateWorld(world());}
function reset(){windows=[{id:'window-b',x:580,y:290,width:340,height:355},{id:'window-a',x:260,y:440,width:300,height:260}];motion=new SurfaceMotion(world());notification=false;paintWindows();}
reset();$('reset').onclick=reset;$('restore').onclick=()=>{const saved=windows;windows=[...saved,...[{id:'window-b',x:580,y:290,width:340,height:355},{id:'window-a',x:260,y:440,width:300,height:260}].filter(w=>!saved.some(v=>v.id===w.id))];paintWindows();};
$('notify').onclick=()=>{notification=true;};$('dismiss-notice').onclick=()=>{notification=false;};
document.querySelectorAll<HTMLButtonElement>('[data-hide]').forEach(b=>b.onclick=()=>{windows=windows.filter(w=>w.id!==b.dataset.hide);paintWindows();});
let drag:{kind:'pet'|'window'|'resize';id?:string;x:number;y:number;startX:number;startY:number;width:number;height:number}|null=null;
function point(e:PointerEvent){const r=desktop.getBoundingClientRect();return {x:e.clientX-r.left,y:e.clientY-r.top};}
desktop.onpointerdown=e=>{
 if(e.button!==0||(e.target as HTMLElement).closest('button:not(.resize)'))return;
 const p=point(e);const hit=Math.abs(p.x-motion.x)<motion.half&&p.y<=motion.y&&p.y>=motion.y-motion.height;
 if(hit){drag={kind:'pet',x:p.x,y:p.y,startX:motion.x,startY:motion.y,width:0,height:0};}
 else{const el=(e.target as HTMLElement).closest<HTMLElement>('.demo-window');if(!el)return;const w=windows.find(w=>w.id===el.id)!;const resize=(e.target as HTMLElement).classList.contains('resize');if(!resize&&!(e.target as HTMLElement).closest('.titlebar'))return;drag={kind:resize?'resize':'window',id:w.id,x:p.x,y:p.y,startX:w.x,startY:w.y,width:w.width,height:w.height};windows=[w,...windows.filter(v=>v.id!==w.id)];paintWindows();}
 desktop.setPointerCapture(e.pointerId);desktop.classList.add('dragging');e.preventDefault();
};
desktop.onpointermove=e=>{if(!drag)return;const p=point(e),dx=p.x-drag.x,dy=p.y-drag.y;
 if(drag.kind==='pet'){motion.reset(drag.startX+dx,drag.startY+dy);return;}
 const w=windows.find(w=>w.id===drag!.id)!;
 if(drag.kind==='window'){w.x=Math.max(0,Math.min(1000-w.width,drag.startX+dx));w.y=Math.max(25,Math.min(660,drag.startY+dy));}
 else{w.width=Math.max(180,Math.min(1000-w.x,drag.width+dx));w.height=Math.max(120,Math.min(700-w.y,drag.height+dy));}paintWindows();
};
desktop.onpointerup=desktop.onpointercancel=()=>{if(drag?.kind==='pet')motion.reset(motion.x,motion.y);drag=null;desktop.classList.remove('dragging');};
function frame(t:number){requestAnimationFrame(frame);if(!last){last=t;return;}const dt=Math.min(.06,(t-last)/1000);last=t;elapsed+=dt;
 const reduced=$<HTMLInputElement>('reduce').checked;
 const beforeX=motion.x,wasGrounded=motion.motion==='grounded';
 if(drag?.kind!=='pet')motion.step(dt,{walk:gaitSpeed('walk',150),autonomous:!notification,reduced,paused:document.hidden});
 if(wasGrounded&&motion.motion==='grounded'&&!notification&&!reduced&&drag?.kind!=='pet')gaitDistance+=Math.abs(motion.x-beforeX);
 if(!wasGrounded&&motion.motion==='grounded')gaitDistance=0;
 if(motion.climbed){motion.climbed=false;proudUntil=elapsed+2.8;}
 const proud=motion.motion==='grounded'&&elapsed<proudUntil&&!notification&&!reduced;
 const pose=drag?.kind==='pet'?'drag':motion.motion==='grounded'?(proud?'proud':notification||reduced?'idle':'walk'):motion.motion==='jump'?'travel-jump':motion.motion;
 renderVectorPet(canvas,pose,proud?(elapsed-proudUntil+2.8)*1000:motion.motion==='grounded'?gaitElapsed('walk',gaitDistance,150):motion.age*1000,{direction:motion.direction,x:motion.x,y:motion.y,scale:150/260,reduced});
 renderPetRope(canvas,pose,motion.age*1000,{x:motion.x,y:motion.y,scale:150/260,direction:motion.direction,reduced,anchor:drag?.kind==='pet'?null:motion.ropeAnchor});
 $('state').textContent=drag?.kind==='pet'?'놓으면 가까운 발판에 착지해요':proud?'올라왔다! 뿌듯해요':names[motion.motion]!;
 const notice=$('notice');notice.hidden=!notification||motion.busy||drag?.kind==='pet';notice.style.left=`${Math.max(8,Math.min(747,motion.x-122))}px`;notice.style.top=`${Math.max(8,motion.y-motion.height-130)}px`;
}
requestAnimationFrame(frame);
