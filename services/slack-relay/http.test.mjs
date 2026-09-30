import test from 'node:test';import assert from 'node:assert/strict';import {spawn} from 'node:child_process';import {mkdtemp,rm} from 'node:fs/promises';import {tmpdir} from 'node:os';import {join} from 'node:path';import {randomBytes,createHash,createHmac} from 'node:crypto';import {once} from 'node:events';
test('relay HTTP authorization, claim binding, Slack challenge',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'ddoktti-relay-'));
 const child=spawn(process.execPath,['services/slack-relay/server.mjs'],{env:{...process.env,PORT:'0',PUBLIC_URL:'https://relay.example',SLACK_CLIENT_ID:'test',SLACK_CLIENT_SECRET:'test',SLACK_SIGNING_SECRET:'sign',TOKEN_ENCRYPTION_KEY:randomBytes(32).toString('base64'),DATABASE_PATH:join(dir,'test.sqlite')},stdio:['ignore','pipe','pipe']});
 try{
 const port=await new Promise((resolve,reject)=>{let out='';const timeout=setTimeout(()=>reject(Error('startup timeout')),10000);child.stdout.on('data',b=>{out+=b;const m=out.match(/listening (\d+)/);if(m){clearTimeout(timeout);resolve(m[1]);}});child.once('exit',()=>{clearTimeout(timeout);reject(Error('server exited'));});child.stderr.on('data',b=>{if(String(b).includes('EACCES')||String(b).includes('EPERM')){clearTimeout(timeout);reject(Error(String(b)));}});});
 const base=`http://127.0.0.1:${port}`;assert.equal((await fetch(base+'/v1/status')).status,401);
 const verifier=randomBytes(32).toString('base64url'),challenge=createHash('sha256').update(verifier).digest('base64url');
 const started=await fetch(base+'/v1/connect',{method:'POST',body:JSON.stringify({challenge})});assert.equal(started.status,200);const {id,loginUrl}=await started.json();assert.equal(new URL(loginUrl).origin,'https://relay.example');
 const claim=v=>fetch(`${base}/v1/connect/${id}/claim`,{method:'POST',body:JSON.stringify({verifier:v})});assert.equal((await claim('wrong')).status,401);assert.equal((await claim(verifier)).status,202);
 const raw=JSON.stringify({type:'url_verification',challenge:'hello'}),ts=String(Math.floor(Date.now()/1000)),signature='v0='+createHmac('sha256','sign').update(`v0:${ts}:${raw}`).digest('hex');
 assert.equal((await fetch(base+'/slack/events',{method:'POST',body:raw})).status,401);
 const verified=await fetch(base+'/slack/events',{method:'POST',headers:{'x-slack-request-timestamp':ts,'x-slack-signature':signature},body:raw});assert.deepEqual(await verified.json(),{challenge:'hello'});
 const denied=await fetch(base+'/slack/callback?state=wrong&code=fake');assert.equal(denied.status,400);
 }finally{if(child.exitCode===null&&child.signalCode===null){const exit=once(child,'exit');child.kill();await exit;}await rm(dir,{recursive:true,force:true});}
});
