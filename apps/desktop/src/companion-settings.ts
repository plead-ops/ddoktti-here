import {ReminderEditor} from './calendar-reminders';
export function syncPlacementLabels(resident:boolean,doc:Document=document){for(const id of ['alert-position-row','alert-monitor-row']){const el=doc.getElementById(id);if(el)el.hidden=resident;}for(const id of ['activity-scope-row','follow-cursor-row']){const el=doc.getElementById(id);if(el)el.hidden=!resident;}}
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
export interface Preferences { onboarded:boolean; resident:boolean; hide_fullscreen:boolean; hide_presenting:boolean; private_content:boolean; stretch:boolean; stretch_minutes:number; quiet_until:number; timer_during_quiet:boolean; calendar_minutes:number; calendar_reminders?:number[]; }
export interface Snapshot { preferences:Preferences; timer:{duration:number;deadline:number|null;remaining:number;completed:boolean}; fullscreen?:boolean; presenting?:boolean; hidden?:boolean; alerts:import('@ddoktti/shared').NotificationPayload[]; now:number }
const native='__TAURI_INTERNALS__' in window;
const get=<T extends HTMLElement=HTMLElement>(id:string)=>document.getElementById(id) as T;
let state:Snapshot;
let reminders:ReminderEditor;
let saving=Promise.resolve();
const connectedAccounts={slack:false,calendar:false};
export function syncOnboardingConnections(slackConnected:boolean,calendarConnected:boolean,doc:Document=document){
 doc.getElementById('onboard-finish')?.classList.toggle('primary',slackConnected&&calendarConnected);
 for(const [id,connected] of [['onboard-slack',slackConnected],['onboard-google',calendarConnected]] as const){
  const button=doc.getElementById(id) as HTMLButtonElement|null;
  if(button){button.classList.toggle('primary',!connected);if(connected){button.textContent='연결됨';button.disabled=true;}}
 }
}
const error=(e:unknown)=>{get('companion-status').textContent=String(e);get('onboard-status').textContent=String(e);};
function run(task:()=>Promise<unknown>){void task().catch(error);}
function patch(value:Partial<Preferences>){saving=saving.then(()=>invoke('set_preferences',{patch:value})).then(()=>undefined).catch(error);}
function render(s:Snapshot){state=s; const ps=document.getElementById('presenting-status');if(ps)ps.textContent=s.presenting?'지금 발표 중으로 감지되어 숨겨져 있어요.':''; for(const key of ['resident','hide_fullscreen','hide_presenting','private_content','stretch','timer_during_quiet'] as const)get<HTMLInputElement>('pref-'+key).checked=s.preferences[key];
 for(const key of ['stretch_minutes'] as const)get<HTMLInputElement>('pref-'+key).value=String(s.preferences[key]);
 reminders.render(s.preferences.calendar_reminders?.length?s.preferences.calendar_reminders:[s.preferences.calendar_minutes]);
 get('quiet-status').textContent=s.preferences.quiet_until>Date.now()/1000?'쉬는 중 · '+new Date(s.preferences.quiet_until*1000).toLocaleTimeString('ko-KR',{hour:'2-digit',minute:'2-digit'}):'알림을 받고 있어요';
 get<HTMLInputElement>('pref-stretch_minutes').disabled=!s.preferences.stretch;
 syncPlacementLabels(s.preferences.resident);get('onboarding').hidden=s.preferences.onboarded;get('app').inert=!s.preferences.onboarded;
 get('timer-pause').textContent=s.timer.deadline?'일시정지':'이어서';
 get<HTMLButtonElement>('timer-pause').disabled=(!s.timer.deadline&&!s.timer.remaining)||s.timer.completed;
 tick();}
function tick(){if(!state)return;get('quiet-status').textContent=state.preferences.quiet_until>Date.now()/1000?'알림 중지 중 · '+new Date(state.preferences.quiet_until*1000).toLocaleTimeString('ko-KR',{hour:'2-digit',minute:'2-digit'})+'까지':'알림을 받고 있어요';const t=state.timer;const sec=t.deadline?Math.max(0,t.deadline-Math.floor(Date.now()/1000)):t.remaining;get('timer-clock').textContent=t.completed?'시간이 됐어요!':`${Math.floor(sec/60).toString().padStart(2,'0')}:${(sec%60).toString().padStart(2,'0')}`;}
interface CalendarState { configured:boolean;connected:boolean;busy:boolean;status:string;events:{title:string;start:number;end:number}[] }
function calendar(s:CalendarState){connectedAccounts.calendar=s.connected;get('onboard-status').textContent=s.configured?s.status:'Google 연결은 배포 설정 후 사용할 수 있어요.';get<HTMLButtonElement>('onboard-google').disabled=!s.configured||s.busy||s.connected;get('onboard-google').textContent=s.busy?'Google 연결 중…':'Google Calendar 연결';syncOnboardingConnections(connectedAccounts.slack,connectedAccounts.calendar);get('calendar-status').textContent=s.configured?s.status:'Google 연결 설정이 필요한 개발 빌드예요';get<HTMLButtonElement>('calendar-connect').disabled=!s.configured||s.busy;get('calendar-connect').textContent=s.connected?'다시 연결':'Google Calendar 연결';get('calendar-disconnect').hidden=!s.connected&&!s.busy;get('calendar-refresh').hidden=!s.connected;const list=get('calendar-events');list.replaceChildren();for(const e of s.events.filter(e=>e.end>Date.now()/1000).slice(0,5)){const row=document.createElement('li');row.textContent=`${new Date(e.start*1000).toLocaleString('ko-KR',{month:'short',day:'numeric',hour:'2-digit',minute:'2-digit'})} · ${e.title}`;list.append(row);}}
interface SlackState {configured:boolean;connected:boolean;busy:boolean;account:string;status:string;filters:Record<string,unknown>}
function slack(s:SlackState){connectedAccounts.slack=s.connected;get('slack-status').textContent=s.configured?`${s.account ? s.account+' · ':''}${s.status}`:'Slack 연결 서버 설정이 필요한 개발 빌드예요';for(const id of ['slack-connect','onboard-slack']){const button=get<HTMLButtonElement>(id);button.disabled=!s.configured||s.busy||s.connected;button.textContent=s.busy?'Slack 연결 중…':s.connected?'연결됨':'Slack에 추가하고 연결';}syncOnboardingConnections(connectedAccounts.slack,connectedAccounts.calendar);get('slack-disconnect').hidden=!s.connected&&!s.busy;get('slack-filters').hidden=!s.connected;if(s.busy)get('onboard-status').textContent=s.status;
 if(!get('slack-filters').contains(document.activeElement)){for(const key of ['dm','mention','broadcast','group','thread'])get<HTMLInputElement>('slack-'+key).checked=s.filters?.[key]!==false;get<HTMLInputElement>('slack-exclude').value=Array.isArray(s.filters?.exclude)?s.filters.exclude.join(', '):'';}}
export async function setupCompanion(){
 if(!native){get('companion-status').textContent='브라우저에서는 화면만 미리 볼 수 있어요. 계정 연결과 타이머는 데스크톱 앱에서 사용할 수 있어요.';return;}
 reminders=new ReminderEditor(document,values=>patch({calendar_reminders:values}));
 await listen<SlackState>('slack-state',e=>slack(e.payload));slack(await invoke<SlackState>('slack_status'));
 await listen<Snapshot>('companion-state',e=>render(e.payload));await listen<CalendarState>('calendar-state',e=>calendar(e.payload));
 render(await invoke<Snapshot>('snapshot'));calendar(await invoke<CalendarState>('calendar_status'));
 for(const key of ['resident','hide_fullscreen','hide_presenting','private_content','stretch','timer_during_quiet'] as const)get<HTMLInputElement>('pref-'+key).addEventListener('change',e=>patch({[key]:(e.target as HTMLInputElement).checked}));
 for(const key of ['stretch_minutes'] as const)get<HTMLInputElement>('pref-'+key).addEventListener('change',e=>patch({[key]:Number((e.target as HTMLInputElement).value)}));
 for(const min of [0,30,60])get('quiet-'+min).onclick=()=>patch({quiet_until:min?Math.floor(Date.now()/1000)+min*60:0});
 get('timer-start').onclick=()=>run(()=>invoke('timer_action',{action:'start',minutes:Number(get<HTMLInputElement>('timer-minutes').value)}));
 get('timer-pause').onclick=()=>run(()=>invoke('timer_action',{action:state.timer.deadline?'pause':'resume'}));get('timer-cancel').onclick=()=>run(()=>invoke('timer_action',{action:'cancel'}));
 for(const action of ['connect','disconnect','refresh'])get('calendar-'+action).onclick=()=>run(()=>invoke('calendar_'+action));
 get('onboard-next').onclick=()=>{get('onboard-mode').hidden=true;get('onboard-connect').hidden=false;};
 get('onboard-back').onclick=()=>{get('onboard-mode').hidden=false;get('onboard-connect').hidden=true;};
 for(const id of ['slack-connect','onboard-slack'])get(id).onclick=()=>run(()=>invoke('slack_connect'));
 get('slack-disconnect').onclick=()=>run(()=>invoke('slack_disconnect'));
 get('slack-save').onclick=()=>run(async()=>{const filters:Record<string,unknown>={};for(const key of ['dm','mention','broadcast','group','thread'])filters[key]=get<HTMLInputElement>('slack-'+key).checked;filters.exclude=get<HTMLInputElement>('slack-exclude').value.split(',').map(s=>s.trim()).filter(Boolean);await invoke('slack_filters',{filters});get('companion-status').textContent='Slack 알림 설정을 저장했어요.';});
 get('onboard-google').onclick=()=>run(()=>invoke('calendar_connect'));
 get('onboard-finish').onclick=()=>run(async()=>{await invoke('set_preferences',{patch:{onboarded:true,resident:get<HTMLInputElement>('onboard-resident').checked}});});
 setInterval(tick,1000);
}
