// Single-instance relay. OAuth credentials stay here; desktop stores only its device session.
import { createServer } from 'node:http';
import { randomBytes,createHash,createCipheriv,createDecipheriv,timingSafeEqual } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';
import { mkdirSync,chmodSync } from 'node:fs';
import { dirname } from 'node:path';
import { verifySlack,classify,plain } from './core.mjs';
const env=process.env;
for(const key of ['PUBLIC_URL','SLACK_CLIENT_ID','SLACK_CLIENT_SECRET','SLACK_SIGNING_SECRET','TOKEN_ENCRYPTION_KEY'])if(!env[key])throw Error(`${key} is required`);
const base=new URL(env.PUBLIC_URL);if(base.protocol!=='https:')throw Error('PUBLIC_URL must use HTTPS');
const key=Buffer.from(env.TOKEN_ENCRYPTION_KEY,'base64');if(key.length!==32)throw Error('TOKEN_ENCRYPTION_KEY must be 32 bytes, base64 encoded');
const dbPath=env.DATABASE_PATH??'./data/slack.sqlite';mkdirSync(dirname(dbPath),{recursive:true,mode:0o700});
const db=new DatabaseSync(dbPath);chmodSync(dbPath,0o600);db.exec('PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS devices (id TEXT PRIMARY KEY, team TEXT NOT NULL, user TEXT NOT NULL, name TEXT NOT NULL, token TEXT NOT NULL, expires INTEGER NOT NULL, filters TEXT NOT NULL)');
const random=()=>randomBytes(32).toString('base64url'),hash=s=>createHash('sha256').update(s).digest('base64url');
function encrypt(value){const iv=randomBytes(12),c=createCipheriv('aes-256-gcm',key,iv);return Buffer.concat([iv,c.update(value),c.final(),c.getAuthTag()]).toString('base64');}
function decrypt(value){const b=Buffer.from(value,'base64'),d=createDecipheriv('aes-256-gcm',key,b.subarray(0,12));d.setAuthTag(b.subarray(-16));return Buffer.concat([d.update(b.subarray(12,-16)),d.final()]).toString();}
const pending=new Map(),queues=new Map(),seen=new Map(),metadata=new Map(),limits=new Map();
setInterval(()=>{const t=Date.now();for(const [id,p]of pending)if(p.until<t)pending.delete(id);for(const [id,at]of seen)if(at<t-3600000)seen.delete(id);for(const [id,v]of metadata)if(v.until<t)metadata.delete(id);for(const [id,v]of limits)if(v.until<t)limits.delete(id);db.prepare('DELETE FROM devices WHERE expires < ?').run(t);},60000).unref();
async function slack(method,token,data={}){const r=await fetch(`https://slack.com/api/${method}`,{method:'POST',headers:{Authorization:`Bearer ${token}`,'Content-Type':'application/x-www-form-urlencoded'},body:new URLSearchParams(data),signal:AbortSignal.timeout(10000)});if(!r.ok)throw Error(r.status===429?'rate_limited':'slack_unavailable');const json=await r.json();if(!json.ok)throw Error(json.error??'slack_error');return json;}
async function cached(id,fn,ttl=60000){let hit=metadata.get(id);if(hit&&hit.until>Date.now())return hit.value;const value=await fn();metadata.set(id,{value,until:Date.now()+ttl});return value;}
function json(res,status,value){res.writeHead(status,{'Content-Type':'application/json','Cache-Control':'no-store','X-Content-Type-Options':'nosniff'});res.end(JSON.stringify(value));}
async function body(req){const chunks=[];let size=0;for await(const c of req){size+=c.length;if(size>1048576)throw Error('body_too_large');chunks.push(c);}return Buffer.concat(chunks);}
function device(req){const t=req.headers.authorization?.replace(/^Bearer /,'');if(!t||t===req.headers.authorization)return null;return db.prepare('SELECT * FROM devices WHERE id=? AND expires>?').get(hash(t),Date.now());}
function purge(id){db.prepare('DELETE FROM devices WHERE id=?').run(id);queues.delete(id);for(const k of metadata.keys())if(k.startsWith(id+':'))metadata.delete(k);}
async function deliver(payload){const e=payload.event;if(e?.type!=='message'||!['channel','im'].includes(e.channel_type))return;
 const devices=db.prepare('SELECT * FROM devices WHERE team=? AND expires>?').all(payload.team_id,Date.now());
 for(const d of devices){try{
  const token=decrypt(d.token),filter=JSON.parse(d.filters);
  const conv=await cached(`${d.id}:channel:${e.channel}`,()=>slack('conversations.info',token,{channel:e.channel}),30000);
  // A public message must be in a joined channel. DM must belong to this user token.
  if(e.channel_type==='channel'&&(!conv.channel?.is_member||conv.channel?.is_private))continue;
  if(e.channel_type==='im'&&(!conv.channel?.is_im||(e.user&&e.user!==d.user&&conv.channel.user!==e.user)))continue;
  let groups=[];if(!classify(e,d.user,[],filter)&&(e.text??'').includes('<!subteam^')){const g=await cached(`${d.id}:groups`,()=>slack('usergroups.list',token,{include_users:'true'}));groups=(g.usergroups??[]).filter(g=>!g.date_delete&&g.users?.includes(d.user)).map(g=>g.id);}
  const trigger=classify(e,d.user,groups,filter);if(!trigger)continue;
  if(trigger==='channel'&&(e.text??'').includes('<!here>')&&!(e.text??'').includes('<!channel>')){const p=await cached(`${d.id}:presence`,()=>slack('users.getPresence',token,{user:d.user}),15000);if(p.presence!=='active')continue;}
  const dnd=await cached(`${d.id}:dnd`,()=>slack('dnd.info',token,{user:d.user}),15000);const now=Date.now()/1000;
  if(dnd.snooze_enabled||(dnd.dnd_enabled&&now>=dnd.next_dnd_start_ts&&now<dnd.next_dnd_end_ts))continue;
  let title=conv.channel?.name??'새 DM';if(e.user){const user=await cached(`${d.id}:user:${e.user}`,()=>slack('users.info',token,{user:e.user}),300000);title=user.user?.profile?.display_name||user.user?.real_name||title;}
  const message={id:`slack:${payload.team_id}:${payload.event_id}`,source:'slack',trigger,title,body:plain(e.text),createdAt:Date.now(),deepLink:`slack://channel?team=${encodeURIComponent(d.team)}&id=${encodeURIComponent(e.channel)}&message=${encodeURIComponent(e.ts)}`};
  // A disconnect during an API request must not recreate its queue.
  if(!db.prepare('SELECT id FROM devices WHERE id=?').get(d.id))continue;
  const queue=queues.get(d.id)??[];queue.push(message);queues.set(d.id,queue.slice(-50));
 }catch(err){if(['invalid_auth','token_revoked','account_inactive'].includes(err.message))purge(d.id);/* Never log message bodies or credentials. */}}
}
let inflight=0;
const server=createServer(async(req,res)=>{try{
 const u=new URL(req.url,base),path=u.pathname;
 if(req.method==='POST'&&path==='/slack/events'){
  const raw=await body(req);if(!verifySlack(raw,req.headers['x-slack-request-timestamp'],req.headers['x-slack-signature'],env.SLACK_SIGNING_SECRET))return json(res,401,{error:'invalid_signature'});
  const p=JSON.parse(raw);if(p.type==='url_verification')return json(res,200,{challenge:p.challenge});
  if(p.type!=='event_callback')return json(res,200,{ok:true});
  if(seen.has(p.event_id))return json(res,200,{ok:true});if(inflight>=100)return json(res,503,{error:'busy'});
  seen.set(p.event_id,Date.now());inflight++;json(res,200,{ok:true});void deliver(p).catch(()=>{}).finally(()=>inflight--);return;
 }
 if(req.method==='POST'&&path==='/v1/connect'){
  const ip=req.socket.remoteAddress;let limit=limits.get(ip)??{count:0,until:Date.now()+60000};if(++limit.count>20||pending.size>=1000)return json(res,429,{error:'try_later'});limits.set(ip,limit);
  const data=JSON.parse(await body(req));if(!/^[A-Za-z0-9_-]{43}$/.test(data.challenge??''))return json(res,400,{error:'invalid_challenge'});
  const id=random(),state=random();pending.set(id,{state,challenge:data.challenge,until:Date.now()+180000});return json(res,200,{id,loginUrl:new URL(`/slack/login?id=${id}`,base).href});
 }
 if(req.method==='GET'&&path==='/slack/login'){
  const p=pending.get(u.searchParams.get('id'));if(!p||p.until<Date.now())return json(res,410,{error:'expired'});
  const url=new URL('https://slack.com/oauth/v2/authorize');url.search=new URLSearchParams({client_id:env.SLACK_CLIENT_ID,user_scope:'channels:history,channels:read,im:history,im:read,users:read,usergroups:read,dnd:read',redirect_uri:new URL('/slack/callback',base).href,state:p.state});res.writeHead(302,{Location:url.href,'Cache-Control':'no-store','Referrer-Policy':'no-referrer'});res.end();return;
 }
 if(req.method==='GET'&&path==='/slack/callback'){
  const p=[...pending.values()].find(p=>p.state===u.searchParams.get('state')&&p.until>Date.now());if(!p||p.used)return json(res,400,{error:'invalid_state'});p.used=true;
  if(u.searchParams.has('error')){p.error='authorization_denied';}else{try{
   const token=await slack('oauth.v2.access','',{client_id:env.SLACK_CLIENT_ID,client_secret:env.SLACK_CLIENT_SECRET,code:u.searchParams.get('code')??'',redirect_uri:new URL('/slack/callback',base).href});
   const user=token.authed_user;if(!user?.access_token||!user.id||!token.team?.id)throw Error('missing_user_access');
   if(token.authed_user.refresh_token)throw Error('token_rotation_not_configured');
   const identity=await slack('auth.test',user.access_token);if(identity.user_id!==user.id||identity.team_id!==token.team.id)throw Error('identity_mismatch');
   p.result={team:token.team.id,user:user.id,name:`${token.team.name??identity.team} · ${identity.user}`,token:encrypt(user.access_token)};
  }catch{p.error='authorization_failed';}}
  res.writeHead(200,{'Content-Type':'text/html; charset=utf-8','Cache-Control':'no-store','Referrer-Policy':'no-referrer','Content-Security-Policy':"default-src 'none'"});res.end('<!doctype html><meta charset="utf-8"><title>똑띠 계정 연결</title><h1>똑띠로 돌아가 주세요.</h1><p>앱에서 연결 결과와 계정을 확인할 수 있어요.</p>');return;
 }
 if(req.method==='POST'&&/^\/v1\/connect\/[A-Za-z0-9_-]+\/claim$/.test(path)){
  const id=path.split('/')[3],p=pending.get(id),data=JSON.parse(await body(req));if(!p||p.until<Date.now())return json(res,410,{error:'expired'});
  if(typeof data.verifier!=='string'||data.verifier.length>128||!timingSafeEqual(Buffer.from(hash(data.verifier)),Buffer.from(p.challenge)))return json(res,401,{error:'invalid_verifier'});
  if(p.error){pending.delete(id);return json(res,400,{error:p.error});}if(!p.result)return json(res,202,{pending:true});
  const token=random(),d=p.result;db.prepare('INSERT INTO devices VALUES (?,?,?,?,?,?,?)').run(hash(token),d.team,d.user,d.name,d.token,Date.now()+90*86400000,'{}');pending.delete(id);return json(res,200,{sessionToken:token,account:d.name});
 }
 const d=device(req);if(!d)return json(res,401,{error:'reconnect_required'});
 if(req.method==='GET'&&path==='/v1/status')return json(res,200,{account:d.name,filters:JSON.parse(d.filters)});
 if(req.method==='POST'&&path==='/v1/notifications'){
  const data=JSON.parse(await body(req)),ack=new Set(Array.isArray(data.ack)?data.ack.filter(x=>typeof x==='string').slice(0,50):[]);const q=(queues.get(d.id)??[]).filter(a=>!ack.has(a.id)&&a.createdAt>Date.now()-15*60000);queues.set(d.id,q);return json(res,200,{alerts:q});
 }
 if(req.method==='PATCH'&&path==='/v1/filters'){
  const f=JSON.parse(await body(req));for(const k of Object.keys(f))if(!['dm','mention','broadcast','group','exclude'].includes(k))return json(res,400,{error:'invalid_filter'});
  for(const k of ['dm','mention','broadcast','group'])if(k in f&&typeof f[k]!=='boolean')return json(res,400,{error:'invalid_filter'});
  if('exclude'in f&&(!Array.isArray(f.exclude)||f.exclude.length>100||f.exclude.some(id=>typeof id!=='string'||! /^[UWBA][A-Z0-9]+$/.test(id))))return json(res,400,{error:'invalid_filter'});
  db.prepare('UPDATE devices SET filters=? WHERE id=?').run(JSON.stringify({...JSON.parse(d.filters),...f}),d.id);return json(res,200,{ok:true});
 }
 if(req.method==='DELETE'&&path==='/v1/session'){purge(d.id);return json(res,200,{ok:true});}
 json(res,404,{error:'not_found'});
}catch{if(!res.headersSent)json(res,400,{error:'request_failed'});else res.end();}}).listen(Number(env.PORT??8787),env.HOST??'127.0.0.1',()=>console.log(`Slack relay listening ${server.address().port}`));
