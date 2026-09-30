import { SurfaceMotion, type SurfaceWorld } from './pet-surfaces';
import { renderVectorPet } from './pet-vector';
import { layoutPet } from './pet-layout';
import { behaviors, chooseBehavior, clickBehavior } from './pet-behaviors';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { openUrl } from '@tauri-apps/plugin-opener';
import { NotificationPayload } from '@ddoktti/shared';
import type { Snapshot } from './companion-settings';
const $=<T extends Element=HTMLElement>(id:string)=>document.getElementById(id) as unknown as T;
const native='__TAURI_INTERNALS__' in window;
const sprite=$<SVGSVGElement>('sprite');
let cfg={speed:1,sound:true,reduce_motion:false,scale:1.7};
let alertChoice:string|undefined,seenAlerts=new Set<string>(),anchor={x:200,y:450},petSize=180;
let state:Snapshot|undefined, current:NotificationPayload|undefined;
let changedAt=performance.now(),mode='greeting',modeUntil=2600,modeStarted=0,direction=1,hover=false,dragging=false,moving=false,fallVelocity=0,lastFrame=0;
function fail(e:unknown){$('pet-error').hidden=false;$('pet-error').textContent=String(e);setTimeout(()=>{$('pet-error').hidden=true;regions();},5000);regions();}
function run(task:()=>Promise<unknown>){void task().catch(fail);}
let lastRegions='';
function regions(){if(!native)return;const rects:number[][]=[];if(!$('pet-root').hidden){for(const part of sprite.querySelectorAll<SVGGraphicsElement>('[data-hit]')){const r=part.getBoundingClientRect();if(r.width&&r.height)rects.push([r.x,r.y,r.width,r.height]);}for(const id of ['bubble','pet-menu','pet-error']){const el=$(id);if(!el.hidden){const r=el.getBoundingClientRect();rects.push([r.x,r.y,r.width,r.height]);}}}const signature=JSON.stringify([rects,dragging]);if(signature===lastRegions)return;lastRegions=signature;void invoke('overlay_regions',{regions:rects,interacting:dragging}).catch(()=>{});}
function layout(){layoutPet(document,anchor,petSize);regions();}
function beep(){if(!cfg.sound)return;try{const audio=new AudioContext();const o=audio.createOscillator(),g=audio.createGain();o.frequency.value=720;g.gain.setValueAtTime(.05,audio.currentTime);g.gain.exponentialRampToValueAtTime(.001,audio.currentTime+.2);o.connect(g).connect(audio.destination);o.start();o.stop(audio.currentTime+.21);o.onended=()=>void audio.close();}catch{}}
const priority=(a:NotificationPayload)=>a.source==='preview'?0:a.source==='calendar'&&(a.startsAt??Infinity)<Date.now()/1000+300?1:a.source==='timer'?2:a.source==='calendar'?3:a.source==='stretch'?5:4;
function render(s:Snapshot,rotate=false){if(state&&state.preferences.resident!==s.preferences.resident)resetSurface();state=s;const alerts=s.alerts.map(a=>NotificationPayload.safeParse(a)).filter(p=>p.success).map(p=>p.data!);alerts.sort((a,b)=>priority(a)-priority(b)||a.createdAt-b.createdAt);const urgent=alerts[0];const chosen=alerts.find(a=>a.id===alertChoice);const next=!rotate&&urgent&&chosen&&priority(urgent)<priority(chosen)?urgent:chosen??urgent;alertChoice=next?.id;if(next?.id!==current?.id){changedAt=performance.now();if(next&&!seenAlerts.has(next.id)){if(!(s.fullscreen&&s.preferences.hide_fullscreen))beep();seenAlerts.add(next.id);if(seenAlerts.size>200)seenAlerts=new Set(alerts.map(a=>a.id));}else if(!next&&current?.source==='timer')setMode('proud');}current=next;
 $('pet-root').hidden=!(s.preferences.onboarded&&s.preferences.resident)&&!next&&!dragging;
 $('bubble').hidden=!next||dragging||mode==='fall'||mode==='land'||mode==='tickle'||!!surface?.busy||!$('pet-menu').hidden;
 if(next){$('title').textContent=s.preferences.private_content?'새 알림이 도착했어요':next.title??'새 소식이 있어요';$('body').textContent=s.preferences.private_content?'내용을 보려면 해당 앱을 열어주세요.':next.body??'';$('count').textContent=alerts.length>1?`${alerts.findIndex(a=>a.id===next.id)+1}/${alerts.length}`:'';$('next-alert').hidden=alerts.length<2;$('open').hidden=!safe(next.deepLink);$('meeting').hidden=!safe(next.meetingUrl);$('snooze').hidden=!(next.source==='stretch'||(next.source==='calendar'&&(next.startsAt??0)>Date.now()/1000+300));}
 layout();}
function safe(value:string|undefined){if(!value)return false;try{const u=new URL(value);return u.protocol==='https:'||u.protocol==='slack:';}catch{return false;}}
async function dismiss(snoozeSeconds?:number){if(current)await invoke('dismiss_alert',{id:current.id,snoozeSeconds});}
$('dismiss').onclick=()=>run(()=>dismiss());$('snooze').onclick=()=>run(()=>dismiss(300));
for(const [id,key] of [['open','deepLink'],['meeting','meetingUrl']] as const)$(id).onclick=()=>run(async()=>{const link=current?.[key];if(safe(link)){await openUrl(link!);await dismiss();}});
let pendingMenu=false,pendingTickle=false;
function menu(show:boolean){if(show&&surface?.busy){pendingMenu=true;return;}pendingMenu=false;$('pet-menu').hidden=!show;if(state)render(state);layout();}
$('pet').oncontextmenu=e=>{e.preventDefault();menu($('pet-menu').hidden === true);};
$('pet').onmouseenter=()=>{hover=true;};$('pet').onmouseleave=()=>{hover=false;};
let press:{x:number;y:number}|undefined,wasDragged=false;
$('pet').onpointerdown=e=>{if(e.button!==0)return;press={x:e.clientX,y:e.clientY};wasDragged=false;};
$('pet').onpointermove=e=>{if(!press||dragging)return;if(Math.hypot(e.clientX-press.x,e.clientY-press.y)>4){press=undefined;wasDragged=true;dragging=true;resetSurface();menu(false);if(state)render(state);run(async()=>{try{await invoke('pet_drag_start');}catch(e){dragging=false;if(state)render(state);throw e;}});}};
window.addEventListener('pointerup',()=>{press=undefined;});
$('pet').onclick=()=>{const action=clickBehavior(mode,wasDragged,!!current);if(action==='wake'){menu(false);setMode('surprised');$('pet-reaction').textContent='앗, 깼어요!';$('pet-reaction').hidden=false;setTimeout(()=>{$('pet-reaction').hidden=true;},1800);}else if(action==='tickle')tickle();};
window.addEventListener('keydown',e=>{if(e.key==='Escape')menu(false);});
function tickle(){if(surface?.busy){pendingTickle=true;return;}menu(false);setMode('tickle',2200);$('pet-reaction').textContent='간지러워요!';$('pet-reaction').hidden=false;if(state)render(state);}
document.querySelectorAll<HTMLButtonElement>('[data-action]').forEach(b=>b.onclick=()=>{menu(false);switch(b.dataset.action){case 'quiet':run(()=>invoke('set_preferences',{patch:{quiet_until:Math.floor(Date.now()/1000)+1800}}));break;case 'settings':run(()=>invoke('open_settings'));break;}});
function nextAlert(){if(!state)return;const list=[...state.alerts].sort((a,b)=>priority(a)-priority(b)||a.createdAt-b.createdAt);const index=list.findIndex(a=>a.id===current?.id);alertChoice=list[(index+1)%list.length]?.id;render(state,true);}
$('next-alert').onclick=nextAlert;
setInterval(()=>{if(state&&state.alerts.length>1&&performance.now()-changedAt>12000&&!dragging&&$('pet-menu').hidden&&!$('bubble').matches(':hover')&&!$('bubble').contains(document.activeElement))nextAlert();},500);
$('pet-timer').onsubmit=e=>{e.preventDefault();run(async()=>{await invoke('timer_action',{action:'start',minutes:Number($<HTMLInputElement>('pet-minutes').value)});menu(false);});};
function draw(pose:string,elapsed=0,facing=direction){renderVectorPet(sprite,pose,elapsed,{direction:facing,reduced:cfg.reduce_motion});}
async function move(dx:number,dy:number){if(state?.preferences.resident)return;if(moving||!native||dragging)return;moving=true;try{const r=await invoke<{edge:boolean;grounded:boolean}>('move_pet',{dx,dy});if(mode==='fall'&&r.grounded){setMode('land',600);}else if((mode==='walk'||mode==='run')&&r.edge){direction*=-1;setMode('wobble',1000);}}catch(e){mode='idle';fail(e);}finally{moving=false;}}
function setMode(next:string,duration?:number){mode=next;modeStarted=performance.now();modeUntil=modeStarted+(duration??behaviors[next]?.duration??7000)/cfg.speed;}
function behavior(t:number,dt:number){const b=behaviors[mode];if(!b)return false;
 draw(mode,(t-modeStarted)*cfg.speed,b.speed?direction:1);
 if(b.speed&&!cfg.reduce_motion&&!hover&&$('pet-menu').hidden)void move(direction*b.speed*dt*cfg.speed,0);return true;
}
// One outstanding native operation, shared by sensing and movement, prevents stale writes.
let surface:SurfaceMotion|undefined,surfaceBusy=false,lastSense=0,lastSurfaceFrame=0,surfaceVersion=0;
let previousSurfaceBusy=false,surfaceErrorShown=false,nextSurfaceAttempt=0;
function resetSurface(){surfaceVersion++;surface=undefined;lastSense=0;lastSurfaceFrame=0;previousSurfaceBusy=false;pendingMenu=false;pendingTickle=false;}
function advanceSurface(t:number){
 if(!native||!state?.preferences.resident||dragging||surfaceBusy||t<nextSurfaceAttempt||state.fullscreen&&state.preferences.hide_fullscreen)return;
 const version=surfaceVersion;surfaceBusy=true;
 void (async()=>{
  if(!surface||t-lastSense>=150){
   const w=await invoke<SurfaceWorld>('pet_world');
   if(version!==surfaceVersion||dragging)return;
   if(surface)surface.updateWorld(w);else surface=new SurfaceMotion(w);
   lastSense=t;surfaceErrorShown=false;
  }
  const dt=lastSurfaceFrame?Math.min((t-lastSurfaceFrame)/1000,.1):1/30;lastSurfaceFrame=t;
  const autonomous=!current&&!hover&&$('pet-menu').hidden&&(mode==='walk'||mode==='run')&&!pendingMenu&&!pendingTickle;
  const walkSpeed=mode==='walk'?24:behaviors[mode]?.speed??0;
  const p=surface!.step(dt,{walk:walkSpeed*cfg.speed,autonomous,reduced:cfg.reduce_motion});
  direction=p.direction;
  const accepted=await invoke<boolean>('surface_move',{monitor:surface!.world.monitor,x:p.x,y:p.y});
  if(version!==surfaceVersion||dragging)return;
  if(!accepted){resetSurface();return;}
  const settled=!surface!.busy&&(mode==='fall'||mode==='land');
  if(settled)setMode('relieved');
  if(previousSurfaceBusy&&!surface!.busy){
   if(pendingTickle){pendingTickle=false;tickle();}else if(pendingMenu){pendingMenu=false;menu(true);}
  }
  if((previousSurfaceBusy!==surface!.busy||settled)&&state)render(state);
  previousSurfaceBusy=surface!.busy;
 })().catch(e=>{nextSurfaceAttempt=t+1000;if(!surfaceErrorShown){surfaceErrorShown=true;fail(e);}if(surface)surface.updateWorld({...surface.world,windows:[]});}).finally(()=>{surfaceBusy=false;});
}
function frame(t:number){requestAnimationFrame(frame);const dt=Math.min((t-lastFrame)/1000,.06);if(t-lastFrame<33)return;lastFrame=t;regions();if($('pet-root').hidden)return;advanceSurface(t);
 const elapsed=(t-modeStarted)*cfg.speed,reduced=cfg.reduce_motion;if(mode!=='tickle'&&mode!=='surprised')$('pet-reaction').hidden=true;
 if(dragging){draw('drag',elapsed);return;}
 if(state?.preferences.resident&&surface?.busy){draw(surface.motion==='jump'?'travel-jump':surface.motion,surface.age*1000,surface.direction);return;}
 if(mode==='fall'&&!state?.preferences.resident){draw('fall',elapsed);fallVelocity=Math.min(fallVelocity+dt*900,600);void move(0,reduced?40:fallVelocity*dt);return;}
 if(mode==='land'){if(t<modeUntil){draw('land',elapsed);return;}setMode('relieved');if(state)render(state);}
 if(mode==='tickle'){if(t<modeUntil){draw('tickle',elapsed,1);return;}setMode('shy');if(state)render(state);}
 if(!$('pet-menu').hidden){draw('idle');return;}
 if(current){draw(current.source==='preview'?'slack':current.source??'slack',(t-changedAt)*cfg.speed,1);return;}
 if(mode==='wobble'){if(t<modeUntil){draw('wobble',elapsed);return;}setMode('relieved');}
 if(reduced){draw(behaviors[mode]?mode:'idle',0,1);return;}
 if(t>=modeUntil)setMode(mode==='surprised'?'curious':chooseBehavior(mode));
 if(hover&&['run','walk'].includes(mode))setMode('curious');
 if(behavior(t,dt))return;
 if(mode==='walk'&&!hover){draw('walk',(t-modeStarted)*cfg.speed);void move(direction*dt*24*cfg.speed,0);}else draw('idle',(t-modeStarted)*cfg.speed,1);
}
function display(c:typeof cfg){resetSurface();const now=performance.now(),age=(now-modeStarted)*cfg.speed,remaining=(modeUntil-now)*cfg.speed;cfg=c;modeStarted=now-age/c.speed;modeUntil=now+remaining/c.speed;petSize=Math.min(245,Math.max(110,180*c.scale/1.7));if(c.reduce_motion&&(mode==='fall'||mode==='land'))setMode('idle');if(state)render(state);layout();}
async function start(){if(!native){$('pet-error').hidden=false;$('pet-error').textContent='데스크톱 앱에서 똑띠를 시작해 주세요.';return;}await listen<Snapshot>('companion-state',e=>render(e.payload));await listen<typeof cfg>('display-settings',e=>display(e.payload));await listen<{x:number;y:number}>('pet-layout',e=>{anchor=e.payload;layout();});await listen('pet-drag-ended',()=>{run(async()=>{dragging=false;resetSurface();fallVelocity=0;if(state?.preferences.resident&&!cfg.reduce_motion){setMode('fall');}else{await invoke('move_pet',{dx:0,dy:0});await invoke('persist_overlay_position');setMode('idle');}if(state)render(state);});});display(await invoke('get_display_settings'));render(await invoke<Snapshot>('overlay_ready'));new ResizeObserver(layout).observe(document.body);setMode('greeting');requestAnimationFrame(frame);}
void start().catch(fail);
