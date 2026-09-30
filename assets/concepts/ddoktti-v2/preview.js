/* Standalone visual study. No application commands or live notifications. */
const reduced = matchMedia('(prefers-reduced-motion: reduce)');
const toggle = document.querySelector('#toggle');
const speed = document.querySelector('#speed');
const status = document.querySelector('#status');
const cards = [...document.querySelectorAll('article[data-kind]')];
let playing = !reduced.matches;
let time = 0;
let last = null;
let ready = false;
const images = {};
const files = {
  walk: 'walk-sheet-v3.png', calendar: 'calendar-sheet-v2.png',
  edge: 'edge-sheet-v1.png', original: 'behavior-sheet-v1.png',
  slack: 'slack-sheet-v2.png',
};
function loadImage(src) {
  return new Promise((resolve, reject) => {
    const img = new Image(); img.onload = () => resolve(img);
    img.onerror = () => reject(new Error(`이미지를 열 수 없습니다: ${src}`)); img.src = src;
  });
}
function sheetFrame(ctx, img, index, cols, rows, x, y, width, flip = false, sourceOverride) {
  const sw = img.naturalWidth / cols, sh = img.naturalHeight / rows;
  const [sx,sy,cw,ch] = sourceOverride || [(index%cols)*sw, Math.floor(index/cols)*sh, sw, sh];
  const height = width * ch / cw;
  ctx.save(); ctx.translate(x, y); if (flip) ctx.scale(-1,1);
  ctx.drawImage(img,sx,sy,cw,ch,-width/2,0,width,height); ctx.restore();
}
// Measured opaque silhouette bounds in walk-sheet-v3.png (1774 × 887).
// The second row was drawn higher and slightly smaller. Register each pose to
// a shared foot baseline AND silhouette height, preserving its aspect ratio.
// These bounds include the antenna and planted shoe, not the lifted swing foot.
const WALK_REGISTRATION = [
  { top:40, bottom:405 }, { top:39, bottom:405 },
  { top:40, bottom:405 }, { top:39, bottom:404 },
  { top:28, bottom:384 }, { top:28, bottom:384 },
  { top:28, bottom:384 }, { top:28, bottom:385 },
];
const WALK_FLOOR = 264;
const WALK_HEIGHT = 148;
function drawWalkingFrame(ctx, img, index, x, flip) {
  const col=index%4, row=Math.floor(index/4);
  const sx=Math.round(col*img.naturalWidth/4), sy=Math.round(row*img.naturalHeight/2);
  const sw=Math.round((col+1)*img.naturalWidth/4)-sx;
  const sh=Math.round((row+1)*img.naturalHeight/2)-sy;
  const bounds=WALK_REGISTRATION[index];
  const scale=WALK_HEIGHT/(bounds.bottom-bounds.top);
  ctx.save(); ctx.translate(x,WALK_FLOOR-bounds.bottom*scale);
  if(flip) ctx.scale(-1,1);
  ctx.drawImage(img,sx,sy,sw,sh,-sw*scale/2,0,sw*scale,sh*scale);
  ctx.restore();
}
function sceneAt(kind, t) {
  if(kind === 'walk') {
    const phase = (t%8000)/8000;
    return {frame:Math.floor(t/120)%8, x:90+300*(phase<.5?phase*2:2-phase*2), flip:phase>=.5};
  }
  if(kind === 'wobble') {
    const p=t%3600;
    return {frame:p<600?3:p<1000?0:p<1600?1:p<2200?2:3, x:p<600?262:p<1600?278:p<2200?264:250};
  }
  if(kind === 'fall') {
    const p=t%3200;
    if(p<550) return {frame:4,y:12,label:'놓이기 전'};
    if(p<1250) { const u=(p-550)/700; return {frame:u<.5?4:5,y:12+134*u*u,label:'가속 낙하'}; }
    if(p<1570) return {frame:6,y:146,label:'착지 · 무릎 굽힘'};
    return {frame:7,y:146,label:'일어서기 · 안도'};
  }
  const interval = kind==='slack'?400:kind==='stretch'?550:320;
  return {frame:Math.floor(t/interval)%4};
}
function render() {
  toggle.textContent=playing?'일시정지':'재생';
  if(!ready) return;
  for(const card of cards) {
    const kind=card.dataset.kind, canvas=card.querySelector('canvas'), ctx=canvas.getContext('2d');
    ctx.setTransform(2,0,0,2,0,0); ctx.clearRect(0,0,480,290);
    const state=sceneAt(kind,time), f=state.frame;
    ctx.fillStyle=document.body.classList.contains('dark')?'#728d73':'#bbccb1';
    if(kind==='walk') { ctx.fillRect(20,WALK_FLOOR,440,2); drawWalkingFrame(ctx,images.walk,f,state.x,state.flip); }
    else if(kind==='wobble') { ctx.fillRect(40,253,243,7); sheetFrame(ctx,images.edge,f,4,2,state.x,75,185); }
    else if(kind==='fall') { ctx.fillRect(20,264,440,3); sheetFrame(ctx,images.edge,f,4,2,240,state.y,125); }
    else if(kind==='slack') sheetFrame(ctx,images.slack,f,4,1,240,0,208);
    else if(kind==='calendar') sheetFrame(ctx,images.calendar,f,4,1,240,6,208);
    else if(kind==='stretch') sheetFrame(ctx,images.original,4+f,4,4,240,24,240);
    else if(kind==='timer') {
      // Third antenna starts above the nominal last-row boundary; include it.
      const cell=images.original.naturalWidth/4;
      sheetFrame(ctx,images.original,12+f,4,4,240,18,240,false,[f*cell,923,cell,images.original.naturalHeight-923]);
    }
    card.querySelector('.phase').textContent=state.label || `프레임 ${f+1} / ${kind==='walk'?8:4}`;
  }
}
function animate(now) {
  if(last!==null && playing && ready && !document.hidden) time+=Math.min(now-last,64)*Number(speed.value);
  last=now; render(); requestAnimationFrame(animate);
}
toggle.addEventListener('click',()=>{playing=!playing;render();});
document.querySelector('#step').addEventListener('click',()=>{playing=false;time+=120;render();});
document.querySelector('#restart').addEventListener('click',()=>{time=0;render();});
document.querySelector('#background').addEventListener('click',(event)=>{
  const dark=document.body.classList.toggle('dark'); event.currentTarget.setAttribute('aria-pressed',String(dark));
  event.currentTarget.textContent=dark?'밝은 배경':'어두운 배경'; render();
});
document.addEventListener('visibilitychange',()=>{last=null;});
reduced.addEventListener('change',()=>{playing=!reduced.matches;render();});
Promise.all(Object.entries(files).map(async([key,file])=>{images[key]=await loadImage(file);})).then(()=>{
  ready=true;status.textContent='7개 동작 준비 완료 · 짧은 다리 산책 + 새 슬랙 알림'; render();
}).catch(error=>{status.textContent=error.message;playing=false;render();});
requestAnimationFrame(animate);
