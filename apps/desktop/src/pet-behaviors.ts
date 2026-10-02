export interface Behavior { duration:number;frameMs:number;loop?:boolean;jump?:boolean;speed?:number;jumpHeight?:number }
export const behaviors:Record<string,Behavior>={
 run:{duration:2500,frameMs:115,loop:true,speed:65},
 bored:{duration:5000,frameMs:1100},
 sleepy:{duration:9000,frameMs:1700},
 jump:{duration:1000,frameMs:250,jump:true},
 excited:{jumpHeight:38,duration:2400,frameMs:200,loop:true,jump:true},
 greeting:{duration:2600,frameMs:400},
 proud:{duration:2800,frameMs:600},
 shy:{duration:3500,frameMs:750},
 curious:{duration:4000,frameMs:900},
 surprised:{duration:1800,frameMs:400},
 playful:{duration:2600,frameMs:550},
 sulking:{duration:2600,frameMs:550},
 cheering:{duration:2600,frameMs:550},
 relieved:{duration:2300,frameMs:450},
};
// Calm actions dominate; active surprises cannot run back-to-back.
const calm=['idle','idle','idle','bored','sleepy','curious'];
const active=['walk','walk','run','jump','excited','greeting','proud','shy','surprised','playful','sulking','cheering'];
export function chooseBehavior(previous:string,random=Math.random()):string {const pool=calm.includes(previous)?active:calm;return pool[Math.min(pool.length-1,Math.max(0,Math.floor(random*pool.length)))]!;}
export function behaviorPose(b:Behavior,elapsed:number,reduced:boolean){const time=Math.max(0,elapsed);const cycle=b.frameMs*4;const phase=(b.loop?time%cycle:Math.min(time,cycle-1))/cycle;return {frame:reduced?3:Math.min(3,Math.floor(phase*4)),lift:reduced||!b.jump?0:(phase>.25&&phase<.75?Math.sin((phase-.25)*Math.PI*2)*(b.jumpHeight??25):0)};}

/** Alert poses after the 2 s intro: alternate the row's two closing drawings at a per-row pace. Mirrors art.rs alert_loop. */
export function alertLoop(row:number,seconds:number){const [a,b,period]=row===0?[3,1,1]:row===1?[3,2,.9]:row===2?[3,2,.8]:[3,2,1.3];const t=seconds-2;return Math.floor(t/period)%2===0?a:b;}
/** Idle loop on the head-tilt sheet (front, tilt left, tilt right, blink). Mirrors art.rs idle_pose. */
export function idlePose(seconds:number){const cycle=seconds%7.2;const frame=cycle>=1.4&&cycle<2.8?1:cycle>=4&&cycle<5.4?2:0;const blink=seconds%4.1;return frame===0&&blink>=3.55&&blink<3.7?3:frame;}
/** Dozing loop after the yawn: nod between the two eyes-closed drawings (row offset). Mirrors art.rs nap_pose. */
export function napPose(napSeconds:number){return Math.floor(napSeconds/1.4)%2===0?2:3;}
export function clickBehavior(mode:string,dragged:boolean,hasAlert:boolean):"wake"|"tickle"|"none" {return dragged?"none":(mode==="sleepy"||mode==="asleep")&&!hasAlert?"wake":"tickle";}
