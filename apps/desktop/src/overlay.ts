/** HTML popups only. Native Rust owns the pet, input, movement and animation. */
import {type AlertGroup,groupAlerts,groupText} from './alert-groups';
import {BubbleEntrance} from './bubble-entrance';
import {layoutPet} from './pet-layout';
import {invoke} from '@tauri-apps/api/core';
import {listen} from '@tauri-apps/api/event';
import {openUrl} from '@tauri-apps/plugin-opener';
import {NotificationPayload} from '@ddoktti/shared';
import type {Snapshot} from './companion-settings';
const $=<T extends HTMLElement=HTMLElement>(id:string)=>document.getElementById(id) as T;
interface PetState {busy:boolean;dragging:boolean;menu:boolean;reaction:string|null;mode:string}
let pet:PetState={busy:false,dragging:false,menu:false,reaction:null,mode:'idle'};
let state:Snapshot|undefined,current:NotificationPayload|undefined,group:AlertGroup|undefined,choice:string|undefined,showRecent=false;
let cfg={sound:true,scale:1.7,reduce_motion:false},anchor={x:200,y:450},changedAt=0,signature='';
let reportedChoice:string|undefined;const seen=new Set<string>();
document.documentElement.classList.add('native-renderer');
const entrance=new BubbleEntrance();
const clock=(seconds:number)=>{const m=Math.floor(seconds/60),s=Math.floor(seconds%60);return m>=60?`${Math.floor(m/60)}시간 ${m%60}분`:`${m}:${String(s).padStart(2,'0')}`;};
function renderMenu(){if(!state)return;const quietUntil=state.preferences.quiet_until,quiet=quietUntil>Date.now()/1000;$('pet-quiet-label').textContent=quiet?'알림 다시 켜기 · '+new Date(quietUntil*1000).toLocaleTimeString('ko-KR',{hour:'2-digit',minute:'2-digit'})+'까지 꺼짐':'30분 동안 알림 끄기';
 const t=state.timer,running=!t.completed&&(t.deadline!==null||t.remaining>0);$('pet-timer-running').hidden=!running;$('pet-timer').hidden=running;
 if(running){const left=t.deadline?Math.max(0,t.deadline-Date.now()/1000):t.remaining;$('pet-timer-left').textContent=(t.deadline?'타이머 ':'일시정지 ')+clock(left)+' 남음';$('pet-timer-pause').textContent=t.deadline?'일시정지':'이어서';}
 const minutes=$<HTMLInputElement>('pet-minutes');if(document.activeElement!==minutes)minutes.value=String(state.preferences.timer_minutes??25);
 $('pet-held').hidden=!state.held;$('pet-held').textContent=`모아 둔 Slack 알림 ${state.held??0}건 · 끝나면 보여드려요`;
 renderRecent();layout();}
function renderRecent(){if(!state)return;const items=[...state.recent??[]].reverse().slice(0,10),list=$('pet-recent-list');$('pet-recent-toggle').setAttribute('aria-expanded',String(showRecent));list.hidden=!showRecent;if(!showRecent)return;
 const hm=(ms:number)=>new Date(ms).toLocaleTimeString('ko-KR',{hour:'2-digit',minute:'2-digit'});
 list.replaceChildren(...(items.length?items.map(a=>{const row=document.createElement('li'),b=document.createElement('button');b.type='button';const head=document.createElement('span');head.textContent=`${hm(a.createdAt)} · ${a.title??(a.source==='calendar'?'일정':'Slack')}`;b.append(head);
  if(!state!.preferences.private_content&&a.body){const body=document.createElement('small');body.textContent=a.body;b.append(body);}
  b.disabled=!safe(a.deepLink);b.onclick=()=>run(async()=>{await openUrl(a.deepLink!);menu(false);});row.append(b);return row;}):[Object.assign(document.createElement('li'),{className:'empty',textContent:'아직 받은 알림이 없어요.'})]));}
function fail(error:unknown){$('pet-error').hidden=!error;$('pet-error').textContent=String(error);layout();}
const run=(task:()=>Promise<unknown>)=>{void task().catch(fail);};
function regions(){const rects:number[][]=[];for(const id of ['bubble','pet-menu','pet-error']){const el=$(id);if(!el.hidden){// Use layout bounds: the entrance transform must not shrink the native click region.
const pad=id==='bubble'?8:0;rects.push([el.offsetLeft-pad,el.offsetTop-pad,el.offsetWidth+pad*2,el.offsetHeight+pad*2]);}}const next=JSON.stringify(rects);if(next!==signature){signature=next;run(()=>invoke('overlay_regions',{regions:rects,interacting:false}));}}
function layout(){layoutPet(document,anchor,180*Math.max(.5,Math.min(5,cfg.scale))/1.7);regions();}
function beep(){if(!cfg.sound)return;try{const audio=new AudioContext(),o=audio.createOscillator(),g=audio.createGain();o.frequency.value=720;g.gain.setValueAtTime(.05,audio.currentTime);g.gain.exponentialRampToValueAtTime(.001,audio.currentTime+.2);o.connect(g).connect(audio.destination);o.start();o.stop(audio.currentTime+.21);o.onended=()=>void audio.close();}catch{}}
const priority=(a:NotificationPayload)=>a.source==='preview'?0:a.source==='calendar'&&(a.startsAt??Infinity)<Date.now()/1000+300?1:a.source==='timer'?2:a.source==='calendar'?3:a.source==='stretch'?5:4;
function safe(value:string|undefined){if(!value)return false;try{return ['https:','slack:'].includes(new URL(value).protocol);}catch{return false;}}
function render(rotate=false){document.documentElement.classList.toggle('reduce-motion',cfg.reduce_motion);if(!state)return;const alerts=state.alerts.map(a=>NotificationPayload.safeParse(a)).filter(p=>p.success).map(p=>p.data!);alerts.sort((a,b)=>priority(a)-priority(b)||a.createdAt-b.createdAt);
 const grouped=groupAlerts(alerts),urgent=grouped[0],chosen=grouped.find(g=>g.key===choice),picked=!rotate&&urgent&&chosen&&priority(urgent.lead)<priority(chosen.lead)?urgent:chosen??urgent,next=picked?.lead;choice=picked?.key;group=picked;
 if(next?.id!==current?.id){changedAt=performance.now();if(next&&!seen.has(next.id)){if(!state.hidden)beep();seen.add(next.id);if(seen.size>200){const alive=new Set(alerts.map(a=>a.id));for(const id of seen)if(!alive.has(id))seen.delete(id);}}}current=next;
 $('pet-root').hidden=false;$('pet-menu').hidden=!pet.menu;$('bubble').hidden=!next||pet.busy||pet.dragging||pet.menu||!!pet.reaction;
 $('slack-actions').hidden=next?.source!=='slack';if(next?.id!==replyFor){replyFor=next?.id;$<HTMLInputElement>('reply-text').value='';}
 entrance.show($('bubble'),next?.id,cfg.reduce_motion);
 $('pet-reaction').hidden=!pet.reaction||pet.busy||pet.dragging||pet.menu;$('pet-reaction').textContent=pet.reaction??'';
 if(picked&&next){const text=groupText(picked,Date.now()/1000,state.preferences.private_content);$('title').textContent=text.title;$('body').textContent=text.body;$('count').textContent=grouped.length>1?`${grouped.indexOf(picked)+1}/${grouped.length}`:'';$('next-alert').hidden=grouped.length<2;$('open').hidden=!safe(next.deepLink);$('meeting').hidden=!safe(meetingOf(picked));$('snooze').hidden=!(next.source==='stretch'||(next.source==='calendar'&&(next.startsAt??0)>Date.now()/1000+300));}
 if(reportedChoice!==next?.id){reportedChoice=next?.id;run(()=>invoke('native_pet_ui',{choice:reportedChoice??null}));}renderMenu();layout();
}
const meetingOf=(g:AlertGroup)=>g.items.find(a=>safe(a.meetingUrl))?.meetingUrl;
function menu(show:boolean){pet.menu=show;if(!show)showRecent=false;render();run(()=>invoke('native_pet_ui',{menu:show,choice:current?.id??null}));}
/** Closes the whole bubble: every message of the conversation it stands for. */
async function dismiss(snoozeSeconds?:number){for(const a of group?.items??[])await invoke('dismiss_alert',{id:a.id,snoozeSeconds});}
$('dismiss').onclick=()=>run(()=>dismiss());
/** "확인" also marks the conversation read in Slack (opt-out in settings); × only closes. */
let readHintShown=false;
$('ack-alert').onclick=()=>run(async()=>{const ids=group?.lead.source==='slack'&&state?.preferences.slack_mark_read?group.items.map(a=>a.id):[];await dismiss();
 if(ids.length)invoke('slack_mark_read',{ids}).catch(e=>{if(!readHintShown){readHintShown=true;fail(e);}});});$('snooze').onclick=()=>run(()=>dismiss(300));
for(const id of ['open','meeting'])$(id).onclick=()=>run(async()=>{const link=id==='open'?current?.deepLink:group&&meetingOf(group);if(safe(link)){await openUrl(link!);await dismiss();}});
let replyFor:string|undefined,sending=false;
async function act(kind:'reply'|'react',value:string){
 if(!current||sending)return;sending=true;
 const buttons=[...$('slack-actions').querySelectorAll('button')];buttons.forEach(b=>b.disabled=true);
 try{
  if(kind==='reply')await invoke('slack_reply',{id:current.id,text:value});else await invoke('slack_react',{id:current.id,name:value});
  $<HTMLInputElement>('reply-text').value='';await dismiss();
 }finally{sending=false;buttons.forEach(b=>b.disabled=false);}
}
$('slack-actions').querySelectorAll<HTMLButtonElement>('[data-react]').forEach(b=>b.onclick=()=>run(()=>act('react',b.dataset.react!)));
$('reply-form').onsubmit=e=>{e.preventDefault();const text=$<HTMLInputElement>('reply-text').value.trim();if(text)run(()=>act('reply',text));};
function groups(){return groupAlerts([...state?.alerts??[]].sort((a,b)=>priority(a)-priority(b)||a.createdAt-b.createdAt));}
function nextAlert(){const list=groups();choice=list[(list.findIndex(g=>g.key===choice)+1)%list.length]?.key;render(true);}
$('next-alert').onclick=nextAlert;
setInterval(()=>{if(pet.menu)renderMenu();},1000);
// A calendar bubble counts down: refresh its wording every half minute.
setInterval(()=>{if(current?.source==='calendar'&&!$('bubble').hidden)render();},30000);
setInterval(()=>{if(state&&state.alerts.length>1&&groups().length>1&&performance.now()-changedAt>12000&&!pet.dragging&&!pet.menu&&!pet.busy&&!$('bubble').matches(':hover')&&!$('bubble').contains(document.activeElement))nextAlert();},500);
for(const id of ['bubble','pet-menu']){const el=$(id);el.onmouseenter=()=>run(()=>invoke('native_pet_ui',{hover:true,choice:current?.id??null}));el.onmouseleave=()=>run(()=>invoke('native_pet_ui',{hover:false,choice:current?.id??null}));}
$('pet-recent-toggle').onclick=()=>{showRecent=!showRecent;renderRecent();layout();};
window.addEventListener('keydown',e=>{if(e.key==='Escape')menu(false);});
document.querySelectorAll<HTMLButtonElement>('[data-action]').forEach(b=>b.onclick=()=>{menu(false);if(b.dataset.action==='quiet'){const quiet=(state?.preferences.quiet_until??0)>Date.now()/1000;run(()=>invoke('set_preferences',{patch:{quiet_until:quiet?0:Math.floor(Date.now()/1000)+1800}}));}if(b.dataset.action==='settings')run(()=>invoke('open_settings'));});
document.querySelectorAll<HTMLButtonElement>('[data-timer]').forEach(b=>b.onclick=()=>run(async()=>{const action=b.dataset.timer==='cancel'?'cancel':state?.timer.deadline?'pause':'resume';await invoke('timer_action',{action});if(action==='cancel')menu(false);}));
const startTimer=(minutes:number)=>run(async()=>{await invoke('timer_action',{action:'start',minutes});menu(false);});
$('pet-timer').onsubmit=e=>{e.preventDefault();startTimer(Number($<HTMLInputElement>('pet-minutes').value));};
document.querySelectorAll<HTMLButtonElement>('[data-minutes]').forEach(b=>b.onclick=()=>startTimer(Number(b.dataset.minutes)));
async function start(){
 if(!('__TAURI_INTERNALS__' in window)){fail('데스크톱 앱에서 똑띠를 시작해 주세요.');return;}
 await listen<PetState>('native-pet',e=>{pet=e.payload;render();});
 await listen<string>('native-pet-error',e=>fail(e.payload));
 await listen<Snapshot>('companion-state',e=>{state=e.payload;render();});
 await listen<typeof cfg>('display-settings',e=>{cfg=e.payload;render();});
 await listen<typeof anchor>('pet-layout',e=>{anchor=e.payload;layout();});
 cfg=await invoke('get_display_settings');state=await invoke('overlay_ready');await invoke('native_pet_ui',{});render();new ResizeObserver(layout).observe(document.body);
}
void start().catch(fail);
