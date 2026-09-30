(() => {
  const $ = (selector) => document.querySelector(selector);
  const data = {
    slack: { kind:'slack', row:0, label:'SLACK · 새 메시지', title:'슬랙 왔어요!', body:'지민 · #제품팀\n“새 디자인 시안, 함께 확인해볼까요?”', detail:'방금 도착한 예시 메시지', privateBody:'새 메시지가 도착했어요. 슬랙에서 확인해 주세요.', actions:[{id:'open',label:'슬랙 열기 ↗'},{id:'dismiss',label:'닫기'}] },
    calendar: { kind:'calendar', row:1, label:'GOOGLE CALENDAR · 5분 전', title:'곧 만날 시간이에요', body:'팀 디자인 리뷰\n오늘 오후 2:00 – 2:30', detail:'Google Meet · 예시 일정', privateBody:'5분 뒤 일정이 시작돼요. 캘린더에서 확인해 주세요.', actions:[{id:'open',label:'회의 참여 ↗'},{id:'snooze',label:'1분 뒤'}] },
    timer: { kind:'timer', row:2, label:'TIMER · 집중 시간 완료', title:'25분, 해냈어요!', body:'한 가지에 집중한 당신, 멋져요.\n이제 잠깐 숨을 돌려볼까요?', detail:'집중 타이머 · 25:00 완료', privateBody:'설정한 타이머가 끝났어요. 잠깐 쉬어도 좋아요.', actions:[{id:'restart',label:'다시 25분'},{id:'dismiss',label:'확인'}] },
    stretch: { kind:'stretch', row:3, label:'TAKE A BREAK · 작은 쉼표', title:'우리, 쭉— 펴볼까요?', body:'벌써 50분째 함께 일했네요.\n어깨를 내리고, 팔을 위로 쭉!', detail:'PC 사용 시간을 기준으로 한 예시 안내', privateBody:'잠깐 몸을 움직여 보세요. 똑띠와 기지개 한 번!', actions:[{id:'done',label:'함께 쉬었어요 ✓'},{id:'snooze',label:'10분 뒤'}] },
  };
  const canvas = $('#pet'), ctx = canvas.getContext('2d');
  const companion = $('#companion'), phase = $('#phase'), feedback = $('#feedback');
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  $('#motion').checked = reduced.matches;
  let selected = 'slack', elapsed = 0, last = null, raf = 0, loaded = false, closed = false;
  const sheet = new Image(), idle = new Image();
  const bubble = createSpeechBubble($('#bubble-host'), handleAction);

  function handleAction(action) {
    const messages = {
      open: selected === 'slack' ? '데모: 실제 앱에서는 슬랙을 엽니다.' : '데모: 실제 앱에서는 일정의 회의 링크를 엽니다.',
      dismiss:'알림을 닫았어요. 똑띠는 잠깐 쉬고 있어요.',
      snooze: selected === 'calendar' ? '데모: 1분 뒤 다시 알림을 선택했어요. 실제 예약은 하지 않습니다.' : '데모: 10분 뒤 다시 알림을 선택했어요. 실제 예약은 하지 않습니다.',
      restart:'데모: 25분 타이머 다시 시작을 선택했어요. 실제 타이머는 실행하지 않습니다.',
      done:'좋아요. 잠깐 쉬고, 다시 천천히 시작해요.',
    };
    feedback.textContent = messages[action] || messages.dismiss;
    closed = true; cancelAnimationFrame(raf); bubble.hide(); resetTransform(); drawIdle();
    phase.textContent = '알림 종료 · 쉬는 중';
    // Restore keyboard focus to a visible control when bubble buttons disappear.
    $('#replay').focus({preventScroll:true});
  }
  function drawFrame(img, col, row, cols, rows) {
    ctx.setTransform(2,0,0,2,0,0);ctx.clearRect(0,0,240,240);
    const sw=img.naturalWidth/cols, sh=img.naturalHeight/rows;
    const w=232, h=w*sh/sw;
    ctx.drawImage(img,col*sw,row*sh,sw,sh,4,236-h,w,h);
  }
  function drawIdle() { if(loaded) drawFrame(idle,3,0,4,2); }
  function resetTransform() { canvas.style.transform='none'; $('.pet-shadow').style.transform='none'; }
  function poseAt(t) {
    if(selected==='stretch') return t<600?0:t<1250?1:t<2050?2:3;
    if(selected==='calendar') return t<550?0:t<1000?1:t<1850?2:3;
    if(selected==='timer') return t<500?0:t<850?1:t<1700?2:3;
    return t<450?0:t<750?1:t<1100?2:t<1300?1:t<1700?2:3;
  }
  function motionAt(t) {
    let y=0, rotate=0, sx=1, sy=1;
    if(selected==='slack') {
      if(t<450) { sx=1.04;sy=.94; }
      else if(t>=750 && t<1700) y=-Math.abs(Math.sin((t-750)/950*Math.PI*2))*29;
    } else if(selected==='timer') {
      if(t>=500 && t<850) { sx=1.09;sy=.87; }
      if(t>=850 && t<1700) y=-Math.sin((t-850)/850*Math.PI)*43;
      if(t>=1700 && t<1900) { sx=1.07;sy=.93; }
    } else if(selected==='calendar' && t>=1000 && t<1850) {
      y=-Math.abs(Math.sin((t-1000)/110))*7; rotate=Math.sin((t-1000)/95)*3;
    } else if(selected==='stretch' && t>=1250 && t<2800) {
      rotate=t<2050?-3:3;
    }
    return {y,rotate,sx,sy};
  }
  function render(t) {
    const quiet=$('#motion').checked;
    const pose=quiet?3:poseAt(t);
    if(selected==='stretch' && (quiet || t>=2800)) drawIdle();
    else drawFrame(sheet,pose,data[selected].row,4,4);
    const m=quiet?{y:0,rotate:0,sx:1,sy:1}:motionAt(t);
    canvas.style.transform=`translateY(${m.y}px) rotate(${m.rotate}deg) scale(${m.sx},${m.sy})`;
    $('.pet-shadow').style.transform=`scale(${1+m.y/150})`;
    const bubbleTime=selected==='stretch'?2200:1850;
    if((quiet||t>=bubbleTime) && !bubble.visible) bubble.show(data[selected],$('#privacy').checked);
    phase.textContent=quiet||t>=2800?'말풍선 · 확인 기다리는 중':t<700?'소식 발견 · 준비 동작':t<bubbleTime?'똑띠가 알려주는 중':'말풍선 등장 · 동작 마무리';
  }
  function tick(now) {
    if(last!==null && !document.hidden) elapsed+=Math.min(now-last,64);
    last=now;render(elapsed);
    if(elapsed<2800 && !closed && !$('#motion').checked) raf=requestAnimationFrame(tick);
    else resetTransform();
  }
  function replay() {
    if(!loaded) return;
    cancelAnimationFrame(raf);closed=false;elapsed=0;last=null;bubble.hide();feedback.textContent='';
    document.body.classList.toggle('reduce-motion',$('#motion').checked);
    render(0); if(!$('#motion').checked) raf=requestAnimationFrame(tick);
  }
  for(const button of document.querySelectorAll('[data-kind]')) {
    button.addEventListener('click',()=>{
      selected=button.dataset.kind;
      for(const option of document.querySelectorAll('.alert-option')) option.setAttribute('aria-pressed',String(option===button));
      replay();
    });
  }
  $('#replay').addEventListener('click',replay);
  $('#privacy').addEventListener('change',()=>{if(bubble.visible) bubble.show(data[selected],$('#privacy').checked);});
  $('#motion').addEventListener('change',replay);
  $('#left').addEventListener('change',()=>companion.classList.toggle('on-left',$('#left').checked));
  document.addEventListener('visibilitychange',()=>{last=null;});
  reduced.addEventListener('change',()=>{$('#motion').checked=reduced.matches;replay();});
  function load(img,src) {return new Promise((resolve,reject)=>{img.onload=resolve;img.onerror=()=>reject(new Error(`이미지를 불러오지 못했어요: ${src}`));img.src=src;});}
  Promise.all([load(sheet,'alert-poses-v1.png'),load(idle,'edge-sheet-v1.png')]).then(()=>{loaded=true;replay();}).catch(error=>{phase.textContent='이미지 로드 실패';feedback.textContent=error.message;});
})();
