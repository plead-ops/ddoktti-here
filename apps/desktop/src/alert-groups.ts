/** One bubble per conversation: Slack messages from the same channel/DM, or calendar reminders starting together. */
import type {NotificationPayload} from '@ddoktti/shared';
import {leadTitle,timeRange} from './calendar-lead';
export interface AlertGroup {key:string;items:NotificationPayload[];lead:NotificationPayload}
export function groupKey(a:NotificationPayload){
 if(a.source==='slack'&&a.deepLink){try{const id=new URL(a.deepLink).searchParams.get('id');if(id)return 'slack:'+id;}catch{}}
 if(a.source==='calendar'&&typeof a.startsAt==='number')return 'calendar:'+a.startsAt;
 return 'alert:'+a.id;
}
/** Keeps the order of `sorted`. A Slack group leads with its newest message, which replies and links target. */
export function groupAlerts(sorted:NotificationPayload[]):AlertGroup[]{
 const groups=new Map<string,AlertGroup>();
 for(const a of sorted){const key=groupKey(a),g=groups.get(key);if(!g){groups.set(key,{key,items:[a],lead:a});continue;}g.items.push(a);if(a.source==='slack'&&a.createdAt>=g.lead.createdAt)g.lead=a;}
 return [...groups.values()];
}
/** Bubble wording for a group; `hidden` replaces message content when the user hides it. */
export function groupText(g:AlertGroup,now:number,hidden:boolean){
 const n=g.items.length,a=g.lead;
 if(a.source==='calendar'&&typeof a.startsAt==='number'){
  const title=leadTitle(a.startsAt,now);
  return {title:n>1?title.replace('일정이',`일정 ${n}개가`):title,body:hidden?'내용을 보려면 해당 앱을 열어주세요.':`${timeRange(a.startsAt,a.endsAt)} · ${g.items.map(i=>i.body??'').join(', ')}`};
 }
 if(hidden)return {title:n>1?`새 알림 ${n}건이 도착했어요`:'새 알림이 도착했어요',body:'내용을 보려면 해당 앱을 열어주세요.'};
 const title=a.title??'새 소식이 있어요';
 return {title:n>1?`${title} · ${n}건`:title,body:a.body??''};
}
