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

export function clickBehavior(mode:string,dragged:boolean,hasAlert:boolean):"wake"|"tickle"|"none" {return dragged?"none":mode==="sleepy"&&!hasAlert?"wake":"tickle";}
