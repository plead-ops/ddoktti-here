/** Wording for a calendar reminder relative to the event start, in seconds. Mirrors calendar.rs lead_title. */
export function leadTitle(startsAt:number,now:number){
 const diff=Math.round(startsAt-now);
 if(Math.abs(diff)<=60)return '지금 일정이 시작돼요';
 if(diff<0)return `${spanText(-diff)} 전에 일정이 시작됐어요`;
 return `${spanText(diff)} 뒤 일정이 시작돼요`;
}
/** "5분", "1시간", "1시간 30분" from seconds, rounded to the minute. */
export function spanText(seconds:number){
 const minutes=Math.max(1,Math.round(seconds/60));
 const h=Math.floor(minutes/60),m=minutes%60;
 return h?(m?`${h}시간 ${m}분`:`${h}시간`):`${m}분`;
}
/** "14:00–14:30" in local time; end omitted when unknown or equal. */
export function timeRange(startsAt:number,endsAt?:number){
 const hm=(s:number)=>{const d=new Date(s*1000);return `${String(d.getHours()).padStart(2,'0')}:${String(d.getMinutes()).padStart(2,'0')}`;};
 return endsAt&&endsAt>startsAt?`${hm(startsAt)}–${hm(endsAt)}`:hm(startsAt);
}
