/** Seconds and logical pet height. Keep in sync with native_pet/physics.rs. */
export function climbDistance(age:number,size:number){
 const cycles=Math.max(0,age)/.8,t=Math.max(0,Math.min(1,((cycles%1)-.25)/.5));
 return size*.4*(Math.floor(cycles)+t*t*(3-2*t));
}
export function climbFrame(seconds:number){return [1,0,2,3][Math.floor(Math.max(0,seconds)/.2)%4]!;}
/** Rope descent replays the climb cycle backwards. */
export function descendFrame(seconds:number){return [3,2,0,1][Math.floor(Math.max(0,seconds)/.2)%4]!;}
