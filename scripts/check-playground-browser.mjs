import {createServer} from 'node:http';import {readFile,writeFile,mkdtemp,mkdir} from 'node:fs/promises';import {join,extname} from 'node:path';import {tmpdir} from 'node:os';import {fileURLToPath} from 'node:url';import {spawn} from 'node:child_process';import {once} from 'node:events';
const root=fileURLToPath(new URL('../apps/desktop/dist/',import.meta.url));const artifacts=join(tmpdir(),'ddoktti-playground-review');await mkdir(artifacts,{recursive:true});const server=createServer(async(req,res)=>{try{const path=new URL(req.url,'http://x').pathname;const file=join(root,path==='/'?'index.html':path);const data=await readFile(file);res.setHeader('Content-Type',({'.js':'text/javascript','.css':'text/css','.html':'text/html','.png':'image/png','.svg':'image/svg+xml'})[extname(file)]??'application/octet-stream');res.end(data);}catch{res.writeHead(404);res.end();}});server.listen(0,'127.0.0.1');await once(server,'listening');const port=server.address().port,profile=await mkdtemp(join(tmpdir(),'ddoktti-chrome-'));const chrome=spawn(process.env.CHROME_PATH??(process.platform==='darwin'?'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome':process.platform==='win32'?'C:/Program Files/Google/Chrome/Application/chrome.exe':'google-chrome'),['--headless=new','--disable-gpu','--remote-debugging-port=0',`--user-data-dir=${profile}`,'--no-first-run','--no-default-browser-check','about:blank'],{stdio:['ignore','ignore','pipe']});
let ws;
try{const debug=await new Promise((resolve,reject)=>{let out='';chrome.stderr.on('data',b=>{out+=b;const m=out.match(/DevTools listening on ws:\/\/127\.0\.0\.1:(\d+)/);if(m)resolve(m[1]);});chrome.once('exit',()=>reject(Error('chrome exited')));setTimeout(()=>reject(Error('chrome startup timeout')),10000).unref();});const tabs=await(await fetch(`http://127.0.0.1:${debug}/json`)).json();ws=new WebSocket(tabs.find(t=>t.type==='page').webSocketDebuggerUrl);await once(ws,'open');let n=0;const waiting=new Map();ws.onmessage=e=>{const r=JSON.parse(e.data);if(r.id){const p=waiting.get(r.id);waiting.delete(r.id);r.error?p.reject(Error(JSON.stringify(r.error))):p.resolve(r.result);}};const call=(method,params={})=>new Promise((resolve,reject)=>{console.log(method);setTimeout(()=>reject(Error('CDP timeout '+method)),10000).unref();const id=++n;waiting.set(id,{resolve,reject});ws.send(JSON.stringify({id,method,params}));});await call('Page.enable');await call('Runtime.enable');


const evaluate=async(expression)=>{const r=await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
await call('Emulation.setDeviceMetricsOverride',{width:1440,height:1100,deviceScaleFactor:1,mobile:false});
await call('Page.navigate',{url:`http://127.0.0.1:${port}/playground.html`});
await new Promise(r=>setTimeout(r,1200));
console.log(await evaluate(`(()=>{
 const get=id=>document.getElementById(id);
 const set=(id,value)=>{get(id).value=value;get(id).dispatchEvent(new Event('input'));};
 get('play').click();
 const results=[];
 for(const b of document.querySelectorAll('[data-scene]')){
  b.click();
  for(const time of [0,1,3,6,9,12]){
   set('timeline',Math.min(time,+get('timeline').max));
   const svg=get('pet');
   if(svg.querySelectorAll('[data-pose-frame]:not([style*="display: none"])').length!==1)throw Error('Missing frame: '+b.dataset.scene);
   if(svg.innerHTML.includes('NaN'))throw Error('Invalid geometry');
  }
  results.push(b.dataset.scene);
 }
 document.querySelector('[data-scene="fall"]').click();set('height','2.6');set('timeline','2.5');
 if(!get('pet').querySelector('[data-pose-frame="hurt-v1-1"], [data-pose-frame="hurt-v1-2"], [data-pose-frame="hurt-v1-3"]'))throw Error('Dedicated hurt animation missing');
 if(get('art-before').innerHTML===get('art-after').innerHTML)throw Error('Hem comparison missing');
 set('size','5');if(get('size-value').textContent!=='5.0×')throw Error('Size limit');
 document.querySelector('[data-scene="tour"]').click();set('layout','fullscreen');
 if(get('pet').style.visibility!=='hidden')throw Error('Fullscreen hide');
 get('hide-fullscreen').checked=false;get('hide-fullscreen').dispatchEvent(new Event('input'));
 if(get('pet').style.visibility==='hidden')throw Error('Fullscreen show');
 set('layout','max');set('size','1.7');
 get('reduced').checked=true;get('reduced').dispatchEvent(new Event('input'));
 if(get('play').textContent!=='재생')throw Error('Reduced motion pause');
 get('reduced').checked=false;get('reduced').dispatchEvent(new Event('input'));
 document.querySelector('[data-scene="edge"]').click();set('timeline','6');
 return {scenes:results,size5:true,fullscreen:true,reducedMotion:true};
})()`));
await writeFile(join(artifacts,'edge.png'),Buffer.from((await call('Page.captureScreenshot')).data,'base64'));
console.log(await evaluate(`(()=>{
 document.querySelector('[data-scene="cursor"]').click();
 const stage=document.getElementById('stage'),r=stage.getBoundingClientRect();
 stage.dispatchEvent(new PointerEvent('pointermove',{clientX:r.left+600/1200*r.width,clientY:r.top+560/680*r.height,bubbles:true}));
 window.beforeFollow=document.querySelector('#pet>g:not([data-rope])').getAttribute('transform');
 document.getElementById('play').click();return 'follow started';
})()`));
await new Promise(r=>setTimeout(r,900));
console.log(await evaluate(`(()=>{document.getElementById('play').click();const after=document.querySelector('#pet>g:not([data-rope])').getAttribute('transform');if(after===window.beforeFollow)throw Error('Cursor follow did not move');return {cursorFollow:true};})()`));
await evaluate(`document.querySelector('.art-review').scrollIntoView()`);
await writeFile(join(artifacts,'art.png'),Buffer.from((await call('Page.captureScreenshot')).data,'base64'));
await call('Emulation.setDeviceMetricsOverride',{width:390,height:844,deviceScaleFactor:1,mobile:false});
console.log(await evaluate(`(()=>{if(document.documentElement.scrollWidth>window.innerWidth+1)throw Error('Mobile overflow');return {mobile:true};})()`));
await call('Emulation.setDeviceMetricsOverride',{width:1440,height:1100,deviceScaleFactor:1,mobile:false});
await call('Page.navigate',{url:`http://127.0.0.1:${port}/ui-preview.html`});
await new Promise(r=>setTimeout(r,1200));
console.log(await evaluate(`(async()=>{
 const settings=document.getElementById('settings-frame').contentDocument,pet=document.getElementById('pet-frame').contentDocument;
 const wait=ms=>new Promise(resolve=>setTimeout(resolve,ms));
 settings.getElementById('pref-resident').checked=false;settings.getElementById('pref-resident').dispatchEvent(new Event('change'));
 settings.getElementById('timer-start').click();await wait(100);
 if(pet.getElementById('sprite').dataset.previewPose!=='ack'||pet.getElementById('pet-root').hidden)throw Error('Timer acknowledgement absent in alert-only mode');
 if(!pet.getElementById('pet-reaction').textContent.includes('시간이 되면'))throw Error('Acknowledgement message missing');
 pet.getElementById('pet').click();pet.getElementById('pet').click();pet.getElementById('pet').click();pet.getElementById('pet').click();await wait(100);
 if(pet.getElementById('sprite').dataset.previewPose!=='enough')throw Error('Repeated tickle does not set a boundary');
 pet.getElementById('pet').click();await wait(100);
 if(pet.getElementById('sprite').dataset.previewPose!=='enough')throw Error('Tickle ignores cooldown');
 return {timerAcknowledgement:true,alertOnlyAcknowledgement:true,tickleBoundary:true,tickleCooldown:true};
})()`));
await writeFile(join(artifacts,'tickle-enough.png'),Buffer.from((await call('Page.captureScreenshot')).data,'base64'));
await evaluate(`document.getElementById('settings-frame').contentDocument.getElementById('timer-start').click()`);
await new Promise(r=>setTimeout(r,150));
await writeFile(join(artifacts,'timer-ack.png'),Buffer.from((await call('Page.captureScreenshot')).data,'base64'));
}finally{ws?.close();chrome.kill();server.close();}
