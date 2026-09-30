import { createHmac, timingSafeEqual } from 'node:crypto';
export function verifySlack(body,timestamp,signature,secret,now=Date.now()/1000){
 if(!/^\d+$/.test(timestamp??'')||Math.abs(now-Number(timestamp))>300||!/^v0=[a-f0-9]{64}$/.test(signature??''))return false;
 const expected='v0='+createHmac('sha256',secret).update(`v0:${timestamp}:`).update(body).digest('hex');
 return timingSafeEqual(Buffer.from(expected),Buffer.from(signature));
}
export function classify(event,self,groups=[],filter={}){
 if(event.type!=='message'||event.user===self||event.hidden||['message_changed','message_deleted','channel_join','channel_leave','message_replied'].includes(event.subtype))return null;
 if(!event.user&&!event.bot_id)return null;
 if((filter.exclude??[]).includes(event.user)||(filter.exclude??[]).includes(event.bot_id))return null;
 if(event.channel_type==='im')return filter.dm===false?null:'dm';
 if(event.channel_type!=='channel')return null;
 const text=event.text??'';
 if(filter.mention!==false&&text.includes(`<@${self}>`))return 'mention';
 if(filter.broadcast!==false&&/<!\s*(?:channel|here)(?:\|[^>]+)?>/.test(text))return 'channel';
 if(filter.group!==false&&groups.some(id=>new RegExp(`<!subteam\\^${id}(?:\\|[^>]+)?>`).test(text)))return 'mention';
 return null;
}
export function plain(text){return String(text??'').replace(/<https?:\/\/[^|>]+\|([^>]+)>/g,'$1').replace(/<!subteam\^[^|>]+(?:\|([^>]+))?>/g,(_,name)=>'@'+(name??'그룹')).replace(/<@[^|>]+(?:\|([^>]+))?>/g,(_,name)=>'@'+(name??'멤버')).replace(/<!(channel|here)(?:\|[^>]+)?>/g,'@$1').replace(/&lt;/g,'<').replace(/&gt;/g,'>').replace(/&amp;/g,'&').slice(0,4000);}
