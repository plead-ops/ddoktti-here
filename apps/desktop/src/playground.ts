import {renderVectorPet, vectorFrame} from './pet-vector';
import {renderPetRope} from './pet-rope';
import {gaitElapsed,gaitSpeed} from './pet-gait';
import hands from './pet-climb-hands.json';

// Interactive choreography using the same SVG poses as the desktop runtime.
const $=<T extends Element=HTMLElement>(id:string)=>document.getElementById(id) as unknown as T;
const ns='http://www.w3.org/2000/svg';
const icons={rope:'M7 3h10M12 3v12q0 6-5 6',hang:'M3 5h18M7 5v7m10-7v7M7 12q5 8 10 0',peek:'M16 3v18M16 7C3 4 3 20 16 17M11 10v1m0 3v1',split:'M3 4h18v16H3zM12 4v16',cursor:'M5 3l4 17 4-7 7-3z',fall:'M12 3v10m-4-4 4 4 4-4M5 20h14',edge:'M3 17h12v4M8 5v5m-3-2 3 2 3-2',monitors:'M2 5h8v11H2zM14 5h8v11h-8zM6 16v3m12-3v3',overlap:'M3 4h13v13H3zM10 10h11v11H10z'};
const scenes=[
 {id:'tour',icon:'rope',title:'줄 타고 화면 한 바퀴',desc:'올라가서 매달리고, 잠깐 쉬었다가 줄을 타고 내려와요.',duration:24,notes:'당기는 순간에만 상승하는지, 정상에서 손이 이어지는지, 내려갈 때 속도가 편안한지 확인해 주세요.'},
 {id:'ceiling',icon:'hang',title:'상단에 매달려 놀기',desc:'두 손으로 옆으로 이동해요. 위쪽으로 마우스를 가져오면 잠깐 따라가 눈을 맞춰요.',duration:12,notes:'머리·안테나는 화면 안에 두고 손만 상단 턱에 닿습니다. 실제 앱과 같은 매달리기 프레임으로 손을 번갈아 옮깁니다.'},
 {id:'peek',icon:'peek',title:'옆에서 빼꼼',desc:'옆으로 숨었다가 얼굴을 내밀고, 몸까지 나타나 인사해요.',duration:10,notes:'일부러 숨는 구간과 의도하지 않은 잘림을 구분해 주세요. 등장 후에는 손발과 안테나가 모두 보여야 합니다.'},
 {id:'split',icon:'split',title:'분할 경계 구경하기',desc:'두 창 사이 경계를 잡고 왼쪽과 오른쪽을 살펴봐요.',duration:12,notes:'실제 앱은 분할 경계가 사라지면 손을 놓고 내려옵니다. 여기서는 경계에서 구경하는 모습을 반복해서 볼 수 있어요.'},
 {id:'cursor',icon:'cursor',title:'마우스와 눈 맞추기',desc:'화면 안에서 마우스를 움직여 보세요. 가까이 오면 따라오다가 멈춰요.',duration:20,notes:'가까운 커서만 따라오고 일정 거리를 남겨둡니다. 위쪽 커서에는 호기심만 보이며 허공으로 따라가지 않습니다.'},
 {id:'fall',icon:'fall',title:'높은 곳에서 떨어지면',desc:'낮으면 사뿐. 높으면 털썩 앉았다가 아야, 털고 일어나요.',duration:8,notes:'아래 낙하 높이를 바꿔 비교해 주세요. 실제 앱에 사용하는 전용 그림으로 눈을 질끈 감고 아픈 곳을 문지른 뒤 회복합니다.'},
 {id:'edge',icon:'edge',title:'발끝까지, 아슬아슬',desc:'창 끝까지 걸어가 아래를 보고 움찔, 두 발을 다시 안으로 옮겨요.',duration:12,notes:'점선은 기존 몸체 여백 기준, 작은 초록 점은 새 발 지지점입니다. 멈칫 위치와 뒷걸음 간격을 비교해 주세요.'},
 {id:'monitors',icon:'monitors',title:'두 화면 사이 산책',desc:'활동 범위를 바꾸면 메인 화면에 머물거나 다른 화면까지 걸어가요.',duration:18,notes:'이 장면은 나란히 연결된 두 화면의 이동을 보여줍니다. 실제 앱은 활동 범위와 화면의 연결 위치·배율·높이를 확인해서 이동합니다.'},
 {id:'overlap',icon:'overlap',title:'겹친 창 · 비활성 창',desc:'앞쪽 작은 창 위로 올라간 뒤, 뒤쪽 비활성 창의 노출된 윗부분으로 이동해요.',duration:16,notes:'활성 여부가 아니라 보이는 윗변과 발을 디딜 폭을 기준으로 삼습니다. 색 표시된 선이 발판 후보이며 가려진 구간은 사용하지 않습니다.'},
] as const;
type Scene=typeof scenes[number];
let scene:Scene=scenes[0],time=0,paused=matchMedia('(prefers-reduced-motion: reduce)').matches,last=0;
let follower={x:380,direction:1,distance:0,nearFor:0,rest:0};
let pointer={x:650,y:565,inside:false};
let upperX=360,upperNear=false;
const stage=$<SVGSVGElement>('stage'),pet=$<SVGSVGElement>('pet');
const clamp=(v:number,a:number,b:number)=>Math.max(a,Math.min(b,v));
const ease=(t:number)=>{t=clamp(t,0,1);return t*t*(3-2*t);};
const mix=(a:number,b:number,t:number)=>a+(b-a)*t;
const icon=(name:keyof typeof icons)=>`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="${icons[name]}"/></svg>`;
for(const s of scenes){const b=document.createElement('button');b.innerHTML=icon(s.icon)+`<span>${s.title}</span>`;b.dataset.scene=s.id;b.onclick=()=>select(s);$('scenes').append(b);}
const value=(id:string)=>$<HTMLInputElement>(id).value;
const checked=(id:string)=>$<HTMLInputElement>(id).checked;
function select(s:Scene){scene=s;time=0;upperX=360;upperNear=false;follower={x:380,direction:1,distance:0,nearFor:0,rest:0};$('scene-title').textContent=s.title;$('scene-desc').textContent=s.desc;$('review-notes').textContent=s.notes;$<HTMLInputElement>('timeline').max=String(s.duration);document.querySelectorAll<HTMLButtonElement>('[data-scene]').forEach(b=>b.setAttribute('aria-pressed',String(b.dataset.scene===s.id)));history.replaceState(null,'',`?scene=${s.id}`);world();draw(0);}
function windowMarkup(x:number,y:number,w:number,h:number,label:string,active=true){const dark=value('background')==='dark';return `<g><rect x="${x}" y="${y}" width="${w}" height="${h}" rx="10" fill="${dark?'#2c3942':'#fffef9'}" stroke="${active?'#94ae98':'#bac5bc'}" stroke-width="2"/><path d="M${x+12} ${y+38}H${x+w-12}" stroke="${dark?'#46515a':'#e6eadd'}"/><circle cx="${x+19}" cy="${y+19}" r="4" fill="${active?'#85b895':'#bac5bc'}"/><text x="${x+34}" y="${y+24}" fill="${dark?'#bfcfc2':'#75877b'}">${label}</text><path d="M${x+28} ${y+76}h${w*.35}m-${w*.35} 20h${w*.66}m-${w*.66} 20h${w*.54}" stroke="${dark?'#384851':'#edf0e5'}" stroke-width="8" stroke-linecap="round"/></g>`;}
function world(){const mode=value('layout');const dark=value('background')==='dark';let html=`<rect x="16" y="18" width="1168" height="620" rx="14" fill="${dark?'#202b33':value('background')==='grid'?'transparent':'#e3eadb'}"/>`;
 if(scene.id==='edge')html+=windowMarkup(220,450,590,188,'작업 중인 창');
 else if(scene.id==='overlap'){html+=windowMarkup(180,310,810,328,'뒤쪽 창 · 비활성',false)+windowMarkup(400,455,310,183,'앞쪽 작은 창 · 활성');html+='<path d="M180 310H990M400 455H710" stroke="#73aa83" stroke-width="4" stroke-dasharray="8 5"/>';}
 else if(scene.id==='monitors'){html+=windowMarkup(30,68,558,570,'메인 모니터')+windowMarkup(612,68,558,570,'보조 모니터',false);html+='<path d="M600 18V638" stroke="#8daa97" stroke-dasharray="5 7"/>';}
 else if(scene.id==='split'||mode==='split'){html+=windowMarkup(30,68,565,570,'왼쪽 창 · 활성')+windowMarkup(605,68,565,570,'오른쪽 창 · 비활성',false);}
 else if(mode==='floating')html+=windowMarkup(280,295,650,343,'떠 있는 창');
 else html+=windowMarkup(30,68,1140,570,mode==='fullscreen'?'전체화면 콘텐츠':'최대화된 작업 창');
 html+='<path d="M16 630H1184" stroke="#819a85" stroke-width="3"/><path d="M24 42H1176" stroke="#698d75" stroke-width="5" stroke-linecap="round"/>';
 $('world').innerHTML=html;$('stage-wrap').dataset.background=value('background');document.querySelector<HTMLElement>('.art-pair')!.dataset.background=value('background');
 $('screen-caption').textContent=scene.id==='monitors'?'활동 범위: '+(value('scope')==='all'?'모든 모니터':'메인 모니터만'):'실제 창을 건드리지 않는 가상 화면 · 화면 안에서 마우스를 움직여 보세요';
}
interface Pose{x:number;y:number;pose:string;t:number;d:number;phase:string;anchor:{x:number;y:number}|null;effect:string;occluder:string;guide:string}
function snapshot(t:number,k:number):Pose{const floor=628,ceiling=44;let p:Pose={x:650,y:floor,pose:'idle',t:t*1000,d:1,phase:'잠깐 쉬기',anchor:null,effect:'',occluder:'',guide:''};
 const hanging=(x:number,age:number)=>{p.pose='hang';p.t=age*1000;const f=vectorFrame(p.pose,p.t);const pts=hands[String(f.index) as keyof typeof hands];p.x=x;p.y=ceiling-(f.top+Math.min(pts[0]![1]!,pts[1]![1]!))*k;p.phase='상단을 잡고 옆으로';};
 if(scene.id==='tour'){
  const top=44+186.58*k,right=1070-85*k;
  if(t<2){p.x=mix(right-70,right,ease(t/2));p.pose='walk';p.t=gaitElapsed('walk',p.x-(right-70),k*260);p.phase='줄 앞으로';}
  else if(t<10){const age=t-2,cycle=age/.8;const progress=(Math.floor(cycle)+ease((cycle%1-.25)/.5))/10;p.x=right;p.y=mix(floor,top,clamp(progress,0,1));p.pose='climb';p.t=age*1000;p.anchor={x:right+90*k,y:ceiling};p.phase='당길 때 올라가기';}
  else if(t<16){hanging(mix(right,650,ease((t-10)/6)),t-10);}
  else if(t<18){hanging(650,0);p.phase='매달려 잠깐 쉬기';}
  else if(t<22){p.x=650;p.y=mix(top,floor,ease((t-18)/4));p.pose='climb';p.t=(22-t)*1000;p.anchor={x:650+90*k,y:ceiling};p.phase='줄 타고 내려오기';}
  else{p.x=650;p.pose=t<22.5?'land':'relieved';p.t=(t-22)*1000;p.phase='착지하고 숨 고르기';}
 }
 if(scene.id==='ceiling'){hanging(upperX,upperNear?0:t<8?t:0);p.phase=upperNear?'위쪽에서 마우스를 발견했어요':t<8?'손을 옮기며 이동':'두 손으로 버티며 쉬기';}
 if(scene.id==='peek'){const amount=t<2?ease(t/2):t<4?1:t<6?1+ease((t-4)/2):t<8?2:2*(1-ease((t-8)/2));p.x=1230+60*k-amount*115*k;p.y=Math.max(310,180*k+80);p.pose=amount>1.5?'greeting':'peek';p.t=(t%2)*600;p.d=-1;p.phase=amount<.5?'옆에 숨어 있기':amount<1.5?'얼굴부터 빼꼼':'몸까지 나와 인사';p.occluder='<path d="M1180 65V610" stroke="#9db49f" stroke-width="8"/>';}
 if(scene.id==='split'){const side=t<5?-1:1;const reach=Math.sin(Math.PI*clamp((t%6)/2,0,1));p.pose='climb';p.t=0;p.d=side<0?1:-1;const f=vectorFrame(p.pose,0),hand=hands[String(f.index) as keyof typeof hands][0]!;p.x=600-p.d*(f.left+hand[0]!)*k+side*reach*5;p.y=Math.max(320,230*k);p.phase=side<0?'왼쪽을 살펴보기':'오른쪽을 살펴보기';p.anchor={x:600,y:70};p.effect='<path d="M600 80V600" stroke="#b1bea6" stroke-width="3"/>';}
 if(scene.id==='cursor'){p.x=follower.x;p.pose=follower.nearFor>0?'chase':pointer.inside&&checked('follow')?'curious':'idle';p.t=p.pose==='chase'?gaitElapsed('run',follower.distance,k*260):t*1000;p.d=follower.direction;p.phase=follower.rest>0?'잠깐 쉬고 다시 놀기':follower.nearFor>0?'두 손 들고 신나게 따라오기':pointer.inside?'가까이서 눈 맞추기':'마우스를 기다려요';}
 if(scene.id==='fall'){const body=180*k;const height=Math.min(+value('height')*body,floor-body-55),start=floor-height;const high=height/body>1.2;p.x=600;p.phase='옷자락을 잡고 들기';p.y=start;p.pose='drag';if(t>1){const elapsed=t-1,fallTime=Math.sqrt(2*height/900);p.y=Math.min(floor,start+450*elapsed*elapsed);p.pose='fall';p.phase='허우적, 떨어지는 중';if(elapsed>=fallTime){const age=elapsed-fallTime;p.y=floor;p.pose=high&&age<2.8?'hurt':age<.6?'land':age<3.7?'relieved':'idle';p.t=age*1000;p.phase=age<.6?'털썩, 착지':high&&age<2.6?'철푸덕, 고개 들기':age<3.7?'털고 일어나기':'다시 괜찮아요';if(high&&age<2.8)p.effect=`<g transform="translate(${p.x+65*k} ${floor-125*k})"><path d="M0 0l5-12 5 12 12 5-12 5-5 12-5-12-12-5z" fill="#e6b953"/><text x="20" y="8">헤헤…</text></g>`;}}}
 if(scene.id==='edge'){const start=410,end=810-18*k,old=810-119.6*k;const distance=t<5?(end-start)*ease(t/5):t<8?end-start:(end-start)-60*k*ease((t-8)/4);p.x=start+distance;p.y=450;p.pose=t<5?'walk':t<8?'wobble':'walk';p.d=t<8?1:-1;p.t=t<5?gaitElapsed('walk',distance,k*260):t<8?(t-5)*400:gaitElapsed('walk',end-p.x,k*260);p.phase=t<5?'발끝까지 다가가기':t<8?'끝을 보고 멈칫':'한 걸음 뒤로';p.guide=`M${old} 235V460`;p.effect=`<circle cx="${end+18*k}" cy="450" r="5" fill="#3b9763"/><text x="${old-95}" y="480">기존 정지선</text>`;}
 if(scene.id==='monitors'){const end=value('scope')==='all'?Math.min(1060,1160-100*k):580-100*k;const progress=t<9?ease(t/9):1-ease((t-9)/9);p.x=mix(150,end,progress);p.d=t<9?1:-1;p.pose='walk';p.t=gaitElapsed('walk',(end-150)*(t<9?progress:2-progress),k*260);p.phase=p.x>600?'보조 모니터 산책':value('scope')==='all'?'메인에서 보조로':'메인 화면 안에서만';}
 if(scene.id==='overlap'){if(k>1.0){p.x=850;p.y=floor;p.phase='지금 크기로는 위 공간이 부족해요';p.pose='curious';}
 else if(t<5){p.x=330;p.y=mix(floor,455,ease(t/5));p.pose='climb';p.anchor={x:400,y:455};p.t=t*1000;p.phase='앞쪽 작은 창에 오르기';}
 else if(t<7){p.x=mix(455,520,ease((t-5)/2));p.y=455;p.pose='walk';p.t=gaitElapsed('walk',p.x-455,k*260);p.phase='작은 창 위 산책';}
 else if(t<10){const u=(t-7)/3;p.x=mix(520,820,u);p.y=mix(455,310,u)-Math.sin(u*Math.PI)*150;p.pose='travel-jump';p.t=400;p.phase='뒤쪽 비활성 창으로';}
 else {p.x=mix(820,910,ease((t-10)/6));p.y=310;p.pose=t<10.5?'land':'walk';p.t=gaitElapsed('walk',p.x-820,k*260);p.phase='비활성 창 위에서도 산책';}}
 return p;
}
function draw(dt:number){const k=+value('size')/1.7*.8;const t=checked('reduced')?0:time;const hidden=value('layout')==='fullscreen'&&checked('hide-fullscreen')&&!['edge','monitors','overlap','split'].includes(scene.id);
 if(scene.id==='cursor'&&dt>0){const dx=pointer.x-follower.x,dy=628-pointer.y,near=pointer.inside&&Math.abs(dx)<350&&dy<240*k&&dy>-20;follower.rest=Math.max(0,follower.rest-dt);const move=checked('follow')&&near&&Math.abs(dx)>100*k&&follower.rest===0;if(move){const step=Math.sign(dx)*Math.min(Math.abs(dx)-100*k,gaitSpeed('run',k*260)*dt);const next=clamp(follower.x+step,60+80*k,1140-80*k);follower.distance+=Math.abs(next-follower.x);follower.x=next;follower.direction=Math.sign(dx);follower.nearFor+=dt;if(follower.nearFor>5){follower.rest=2;follower.nearFor=0;}}else follower.nearFor=0;}
 if(scene.id==='ceiling'){const autoX=mix(360,890,ease(Math.min(t,8)/8));upperNear=checked('follow')&&pointer.inside&&pointer.y<90+220*k&&Math.abs(pointer.x-upperX)<330;if(dt>0){const target=upperNear?clamp(pointer.x-Math.sign(pointer.x-upperX)*95*k,180,1050-100*k):autoX;upperX+=clamp(target-upperX,-70*k*dt,70*k*dt);}else if(!upperNear)upperX=autoX;}
 const p=snapshot(t,k);renderVectorPet(pet,p.pose,p.t,{x:p.x,y:p.y,scale:k,direction:p.d,reduced:false});renderPetRope(pet,p.pose,p.t,{x:p.x,y:p.y,scale:k,direction:p.d,anchor:p.anchor});pet.style.visibility=hidden?'hidden':'visible';$('effects').innerHTML=hidden?'':p.effect;$('occluder').innerHTML=hidden?'':p.occluder;$('guide').setAttribute('d',hidden?'':p.guide);
 if(checked('anchors')&&!hidden){const mark=document.createElementNS(ns,'g');mark.innerHTML=`<circle cx="${p.x}" cy="${p.y}" r="5" fill="#dd704d"/><path d="M${p.x-14} ${p.y}h28M${p.x} ${p.y-14}v28" stroke="#dd704d"/>`;if(p.anchor)mark.innerHTML+=`<circle cx="${p.anchor.x}" cy="${p.anchor.y}" r="7" fill="none" stroke="#dd704d" stroke-width="2"/>`;$('effects').append(mark);}
 $('phase').textContent=hidden?'전체화면에서는 숨김':p.phase;$('live-hint').textContent=hidden?'‘전체화면 사용 중 숨기기’를 끄면 동작을 볼 수 있어요.':['cursor','ceiling'].includes(scene.id)?'마우스를 캐릭터 근처로 가져와 보세요.':scene.id==='fall'&&+value('height')*180*k>628-180*k-55?'화면 높이에 맞춰 실제 낙하 거리를 줄였어요.':'';
 $('pointer').innerHTML=pointer.inside&&['cursor','ceiling'].includes(scene.id)?`<g transform="translate(${pointer.x} ${pointer.y})"><circle r="14" fill="#b9d6a7" opacity=".4"/><path d="M0 0l3 20 5-7 8-3z" fill="#fffef8" stroke="#3e644b" stroke-width="2"/></g>`:'';
 $<HTMLInputElement>('timeline').value=String(time);$('clock').textContent=time.toFixed(1)+'초';$('play').textContent=paused?'재생':'일시정지';
}
// Per-pose candidate lines; a reviewed overlay, not a replacement body or a scaling distortion.
const hems:Record<string,{d:string;detail:string}>={idle:{d:'M170 226 Q181 231 196 226 L201 221 M204 229 Q217 230 227 226',detail:'기본 자세: 손 아래 재킷 밑단을 두 조각으로 나눠 바지와 구분합니다.'},land:{d:'M186 237 Q197 243 210 237',detail:'착지: 손과 무릎에 가려진 부분은 남겨두고 중앙의 짧은 밑단만 보완합니다.'},walk:{d:'',detail:'걷기: 원래의 재킷 밑단을 유지하고 옷 밖으로 나왔던 추가 선을 제거했습니다.'},run:{d:'',detail:'달리기: 원래의 주름과 밑단을 유지하고 겹쳐 그린 선을 제거했습니다.'}};
function art(){const pose=value('art-frame');for(const id of ['art-before','art-after'])renderVectorPet($<SVGSVGElement>(id),pose,0,{x:200,y:265,scale:1.25});
 // The baseline isolates the reviewed hem repair; both panels use the current, uniform palette.
 for(const path of $('art-before').querySelectorAll('path'))if(path.getAttribute('d')===hems[pose]!.d)path.remove();
 $('art-detail').textContent=hems[pose]!.detail+(hems[pose]!.d?' 양쪽 모두 현재의 통일된 색상을 사용하며, 왼쪽은 밑단 복원선만 제외했습니다. 오른쪽은 실제 앱의 SVG입니다.':' 두 그림 모두 불필요한 추가 선을 제거한 실제 앱의 SVG입니다.');
}
$('play').onclick=()=>{paused=!paused;draw(0);};$('restart').onclick=()=>{time=0;follower={x:380,direction:1,distance:0,nearFor:0,rest:0};draw(0);};$('step').onclick=()=>{paused=true;time=Math.min(scene.duration,time+1/12);draw(0);};$<HTMLInputElement>('timeline').oninput=()=>{paused=true;time=+value('timeline');draw(0);};
for(const id of ['size','height','layout','scope','background','follow','hide-fullscreen','reduced','anchors'])$(id).addEventListener('input',()=>{if(id==='reduced'&&checked(id))paused=true; $('size-value').textContent=(+value('size')).toFixed(1)+'×';$('height-value').textContent=`키의 ${(+value('height')).toFixed(1)}배 · ${+value('height')>1.2?'높은 낙하 후 반응':'가벼운 착지'}`;world();draw(0);});
$('art-frame').addEventListener('change',art);
stage.addEventListener('pointermove',e=>{const point=new DOMPoint(e.clientX,e.clientY).matrixTransform(stage.getScreenCTM()!.inverse());pointer={x:point.x,y:point.y,inside:true};if(paused)draw(0);});stage.addEventListener('pointerleave',()=>{pointer.inside=false;draw(0);});
$<HTMLInputElement>('reduced').checked=paused;
select(scenes.find(s=>s.id===new URLSearchParams(location.search).get('scene'))??scenes[0]);art();
function animate(now:number){const dt=last?Math.min((now-last)/1000,.06)*+value('speed'):0;last=now;if(!paused&&!document.hidden&&!checked('reduced')){time=(time+dt)%scene.duration;draw(dt);}requestAnimationFrame(animate);}requestAnimationFrame(animate);
